# Optional RMK backlight

Software candidate, 2026-10-02. No lighting image has been installed or physically validated.

The framework extension is pinned to [RMK fork `beab0774`](https://github.com/sarimabbas/rmk-nocfree/commit/beab077484db7d6bd96d19421bd4eae2466f511e), based on upstream `9607aed`. A separate small fix rejects macro splices when zero macros are configured; the board configuration exposed a constant-underflow lint in local source builds. Dependencies are locked; no uncommitted Cargo-cache patch or floating branch dependency is used.

## Behavior

- Sixteen brightness levels including off; saturating up/down, on/off/toggle and cycling. On/toggle remembers the last nonzero level. New storage defaults to off.
- Mac mode uses plain F5/F6 for keyboard brightness and Fn+F5/F6 for ordinary function keys. Generic mode uses ordinary F5/F6 and Fn for lighting. Feature-off mappings remain unchanged.
- RMK resolves actions and handles presses once. Key processing updates a bounded latest-value Watch without awaiting PWM, radio or flash. No ActionEvent subscriber is added.
- The left owns brightness. An appended absolute brightness/sleep snapshot travels over the existing split link, including on every split connection. An older right firmware can still type but cannot apply that message. Both halves need lighting builds for synchronization.
- The left restores through RMK storage and saves two seconds after the last brightness change. Peripheral lighting is not independently persisted. Sleep is temporary output-off and neither saves zero nor postpones a pending brightness save.
- Explicit user intent wins a startup storage-read race, including Off/Down at initial zero. Output skips identical effective levels. Subscribe-before-snapshot ordering retains changes made during the reconnect send.

The board supplies PWM0, vendor-published P0.20, standard GPIO drive, frequency and explicit polarity. The adapter and behavior belong to RMK. The interface is `backlight::run(output, authoritative)` with `NrfPwm::new(pwm, active_low)`. There are no animations, RGB dependencies, second transport, board-local key interpreter or Vial lighting protocol. Rebase the small fork changes when upgrading RMK and rerun framework tests plus the board build matrix.

## Hardware and migration limits

Physical polarity remains unknown. `backlight-active-low` and `backlight-active-high` are alternative explicit build choices, not interchangeable approved images. A half with the generic `backlight` feature alone fails compilation until polarity is selected. Lighting is disabled by default.

The proposed PWM setting is 8 kHz (16 MHz / 2000), not a measured factory frequency. Embassy `SimplePwm::set_duty` busy-waits for SEQEND while enabled: one nominal period is 125 µs. Interrupts remain available, but another async task on the same executor waits during that call. Coalescing and caching reduce redundant work; they do not establish an input-latency bound. Physical output and typing under brightness load require measurement.

The board's sleep timeout is zero, disabling central sleep management including explicit suspend requests. These hooks therefore **do not enable automatic idle/suspend backlight-off** in the present configuration. A sleep-policy change requires separate wake/input acceptance.

**The RMK revision/feature change changes the storage schema and resets saved settings/bonds.** Save a fresh working-state backup and expect host Bluetooth pairing again. Do not restore incompatible storage or bypass the schema check. The working receiver needs no lighting update and remains unchanged.

See [independent acceptance review](backlight-acceptance.md) for polarity, endpoints, both-half rejoin, persistence and input/release/wake/transport checks. Software tests do not establish these hardware results.

## Repeatable checks

From the board repository:

```sh
./scripts/check.sh --reclaimed-softdevice --usb-recovery-first --backlight-active-high
./scripts/check.sh --reclaimed-softdevice --usb-recovery-first --backlight-active-low
```

CI checks both polarity choices. Receiver builds omit lighting; separately cross-build the actual protected receiver without migration features. Individual board builds must run from `firmware/` to load its Cargo configuration.

For framework tests, clone the public fork, check out the exact pinned revision, install `cargo-nextest`, then run from that checkout:

```sh
cargo +1.93.1 nextest run --manifest-path rmk/Cargo.toml --no-default-features \
  --features backlight,storage,split,std,rynk,_ble --lib
```

Nextest isolates mock-clock tests; ordinary `cargo test` is unsuitable for those tests. The full suite passed 277 tests, including six backlight cases and the invalid macro-slot regression. The scanner/image host harness and all role/keymap and diagnostic cross-builds passed with the high-polarity candidate. Low-polarity halves also cross-built with both keymaps; feature-off halves and the protected receiver passed separately. Hosted CI results remain separate from local checks.

## Offline candidates

Ignored local packages bind exact ELF LOADs to BIN and page-padded UF2. Both reclaimed-half layouts start at `0x1000`, stop before settings at `0x65000`, retain the existing recovery marker and satisfy the nRF52833 address/vector/family gate. An independent parser verified all four packages; this proves structure, not device identity or installation approval.

| Role / alternative polarity | Exact BIN bytes | Padded end, exclusive |
| --- | ---: | --- |
| Left / high | 382188 | `0x5f000` |
| Left / low | 382196 | `0x5f000` |
| Right / high | 217612 | `0x37000` |
| Right / low | 217620 | `0x37000` |

Generated candidates and working/factory backups stay private. A fresh left recovery readback matched the installed working application and gap exactly; settings were saved separately in its readable image. A first lighting trial still needs a separately reviewed, owner-approved operator. Choose physically verified polarity before shipping a default.

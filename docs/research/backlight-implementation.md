# Optional RMK backlight

Initial left-only hardware result, 2026-10-02: the factory-matched 400 Hz active-high image passed owner-observed off control, three visible brightness steps in each direction and USB typing. The earlier 8 kHz image passed logical step counting but appeared full brightness at every nonzero level. The right half and receiver were unchanged in that initial trial. All 15 nonzero levels and electrical waveforms have not been individually validated. See [PWM diagnosis](backlight-pwm-diagnosis.md).

The framework extension is pinned to [RMK fork `b41bd7ca`](https://github.com/sarimabbas/rmk-nocfree/commit/b41bd7caa7de67a47f6e7380519b730630f16152), based on upstream `9607aed`. A separate small fix rejects macro splices when zero macros are configured; the board configuration exposed a constant-underflow lint in local source builds. Dependencies are locked; no uncommitted Cargo-cache patch or floating branch dependency is used.

## Behavior

- Sixteen brightness levels including off; saturating up/down, on/off/toggle and cycling. On/toggle remembers the last nonzero level. New storage defaults to off.
- Mac mode uses plain F5/F6 for keyboard brightness and Fn+F5/F6 for ordinary function keys. Generic mode uses ordinary F5/F6 and Fn for lighting. Feature-off mappings remain unchanged.
- RMK resolves actions. Taps change one step; physical Up/Down holds repeat after 350 ms at 80 ms intervals on the existing keyboard deadline scheduler. Release, sleep and endpoint saturation stop repetition. Other actions remain single-press. Key processing updates a bounded latest-value Watch without awaiting PWM, radio or flash. No ActionEvent subscriber is added. See [hold behavior](backlight-hold.md); the owner-assisted left trial passed tap, held ramp, release-stop and typing during a hold.
- The left owns brightness. An appended absolute brightness/sleep snapshot travels over the existing split link, including on every split connection. An older right firmware can still type but cannot apply that message. Both halves need lighting builds for synchronization.
- The left restores through RMK storage and saves two seconds after the last brightness change. Peripheral lighting is not independently persisted. Sleep is temporary output-off and neither saves zero nor postpones a pending brightness save.
- Explicit user intent wins a startup storage-read race, including Off/Down at initial zero. Output skips identical effective levels. Subscribe-before-snapshot ordering retains changes made during the reconnect send.

The board supplies PWM0, vendor-published P0.20, standard GPIO drive, frequency and explicit polarity. The adapter and behavior belong to RMK. The interface is `backlight::run(output, authoritative)` with `NrfPwm::new(pwm, active_low)`. There are no animations, RGB dependencies, second transport, board-local key interpreter or Vial lighting protocol. Rebase the small fork changes when upgrading RMK and rerun framework tests plus the board build matrix.

## Hardware and migration limits

Active-high off and dimming behavior is now owner-confirmed on both halves. Electrical polarity and waveforms have not been measured. `backlight-active-low` and `backlight-active-high` are alternative explicit build choices, not interchangeable approved images. A half with the generic `backlight` feature alone fails compilation until polarity is selected. Lighting is disabled by default.

The follow-up PWM setting matches the factory binary's explicit 400 Hz request (8 MHz / 20,000). The pinned [Embassy fork](https://github.com/sarimabbas/embassy-nrf-nocfree/commit/1b5fc397aa65026925d9641d88073a7e91993647) adds `BufferedPwm`, using a unique static RAM duty buffer and cooperative DMA waiting. Pending state survives cancelled or forgotten update futures; subsequent updates finish DMA before modifying the buffer. The prior synchronous API remains unchanged. Coalescing and cooperative waiting do not establish an input-latency bound; The owner confirmed left dimming and typing during a hold; waveform, measured latency and both-half behavior remain unverified.

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

## Archived first-trial packages

Ignored local packages bind exact ELF LOADs to BIN and page-padded UF2. Both reclaimed-half layouts start at `0x1000`, stop before settings at `0x65000`, retain the existing recovery marker and satisfy the nRF52833 address/vector/family gate. An independent parser verified all four packages; this proves structure, not device identity or installation approval.

| Role / alternative polarity | Exact BIN bytes | Padded end, exclusive |
| --- | ---: | --- |
| Left / high | 382188 | `0x5f000` |
| Left / low | 382196 | `0x5f000` |
| Right / high | 217612 | `0x37000` |
| Right / low | 217620 | `0x37000` |

Generated candidates and working/factory backups stay private. A fresh left recovery readback matched the installed working application and gap exactly; settings were saved separately in its readable image. The approved first left trial verified exact installed bytes and USB-first recovery, then established off control and logical step counting but failed visible dimming. A different image needs a separately reviewed, owner-approved trial; the earlier approval does not authorize a different candidate. Choose physically verified polarity before shipping a default.

The subsequent matching right lighting trial passed exact readback and USB-first recovery. The owner confirmed synchronized tap-and-hold brightness, stopping on release, responsive right typing during a hold and cross-half Shift. A slight right-side lighting delay was reported and remains unmeasured. Restarting only the right restored the left’s current brightness without another brightness-key press, as confirmed by the owner. Saved brightness across a left restart remains pending. See [hold validation](backlight-hold-validation.md).

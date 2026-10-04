# NocFree AND RMK: customization and recovery guide

For developers customizing this community firmware. Updated against committed source through `59ef775`, October 4, 2026, and the main firmware conversation. Hardware observations are identified separately from source behavior. Battery calibration and full installer roundtrip acceptance remain incomplete; this is not a claim that every factory feature or every PCB revision is supported.

## Start here

Use **NocFree RMK Companion → Recovery mode**, choose the part, and connect that part by a USB data cable. The running RMK firmware exposes a local USB DFU **runtime DETACH** interface. Companion requests the original factory bootloader, which presents the **NocFree &** drive. This flow has been exercised on the left, right and receiver, with exact application readbacks.

Save `CURRENT.UF2` and `INFO_UF2.TXT` from each part separately before changing it. Keep the part identity with its backup. A readback archive is not automatically a complete restore package: it does not include every protected region, and restoring application bytes does not guarantee compatible settings or bonds. Do not interchange left, right and receiver images.

Current RMK builds do not use the earlier Fn+Escape or startup Fn+Shift recovery shortcuts. Factory shortcuts depend on factory firmware and are not a universal hardware reset procedure. Recovery is not a factory reset: entering it alone does not intentionally erase settings.

## Architecture

```text
Right: PCA9555 inputs → board scanner → RMK split peripheral
                                      │
                                 BLE split link
                                      │
Left:  PCA9555 inputs → board scanner → RMK central/keymap
                                      ├── USB HID → computer
                                      ├── Bluetooth HID → computer
                                      └── BLE receiver link → receiver → USB HID → computer

Companion → local USB DFU DETACH → selected part's factory bootloader
Companion/Vial → USB raw HID → left's Vial service
                          └→ receiver relay → left's Vial service
```

The **left** interprets both halves' keys, layers and modifiers. The **right** forwards physical key events; its USB connection supplies management/recovery rather than a second independent keyboard. The **receiver** relays keyboard reports and Vial requests. Both wireless links use RMK's BLE implementation; this port does not implement the factory ESB radio protocol or use the left's external nRF24L01 for the split link.

RMK owns debounce, key actions, USB, BLE, profiles, configuration storage and lighting synchronization. The board scanner owns PCA9555 initialization, electrical input mapping and complete snapshots. A failed I²C read must not manufacture key releases. Backlighting uses a board-supplied PWM output and RMK's brightness/persistence/split machinery, currently at 400 Hz.

Battery reporting uses the existing RMK producer caches. Companion reads a small read-only VIA custom-value extension; the receiver forwards that request to the left. Both halves' reporting has been observed through left USB and receiver USB. Percentages remain uncalibrated, and charging state is Unknown: USB power alone does not prove charging.

### Where to customize

| Concern | Source |
| --- | --- |
| Physical input mapping and expander contract | `crates/nocfree-input/` |
| Electrical scanner and RMK debounce integration | `firmware/src/scanner.rs` |
| Keymap and Mac function/media behavior | `firmware/src/keymap.rs` |
| Role composition, peripheral ownership, PWM and ADC configuration | `firmware/src/main.rs` |
| Divider-enabled ADC sampling | `firmware/src/battery.rs` |
| Passive physical selector input | `firmware/src/mode_switch.rs` |
| Startup watchdog escape | `firmware/src/watchdog_recovery.rs` |
| Dimensions, profile count and split configuration | `firmware/keyboard.toml` |
| Companion guided recovery | `desktop/src/recovery.rs`, `desktop/src/recovery_journey.rs` |
| Companion battery transport | `desktop/src/battery.rs`, `desktop/src/battery_vial.rs` |

Dependencies and the RMK fork revision are pinned in `firmware/Cargo.toml` and `firmware/Cargo.lock`. The fork has advanced beyond the initial `c8e06c6` recovery baseline. Consult the current pin and lockfile; packaged role images can intentionally come from different reviewed checkpoints. Follow the checked-out source, rather than historical diagnostic instructions, when changing features.

## Vendor pin reference

The following GPIO facts come from the [vendor-published porting guide](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/README.md#4-pins-required-for-zmk-porting). They are published mappings, not a complete schematic or measurements of every revision. Use Nordic GPIO numbers; Arduino aliases differ between halves.

| Signal | Left | Right |
| --- | --- | --- |
| PCA9555 interrupt | P0.31 | P0.05 |
| I²C SDA / SCL | P0.11 / P1.09 | P0.11 / P1.09 |
| Bluetooth position, active low | P0.15 | Not documented |
| Receiver position, active low | P0.17 | Not documented |
| Shared red charge/status indicator | P0.09 | P0.17 |
| Backlight | P0.20 | P0.20 |
| Blue indicator | P0.10 | Not documented |
| Battery ADC | P0.04 | P0.04 |
| Divider enable, active high | P0.05 | P0.31 |

Use pull-ups for the interrupt and selector inputs. Shared charge/status pins must not be driven push-pull. PCA9555 addresses are `0x20`, `0x22`, `0x24`; read both ports in that order. KR adds `0x21/P0`. The published ADC is 12-bit with a voltage multiplier of `130/100`; calibrate rather than assuming that ratio proves accurate capacity. Factory external-radio pins on the left are SCK P0.28, MOSI P0.29, MISO P0.30, CE P0.03 and CSN P0.02. [Vendor reference](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/README.md).

The physical left selector now owns strict output selection: **top receiver, middle wired, bottom Bluetooth**. The two passive inputs are debounced for 25 ms; invalid startup input disables output and an invalid transition retains the last valid choice. Radio modes ignore USB for keyboard output while local USB recovery remains available. Fn+U and Fn+B are removed from the default keymap. Keep the last ordinary Bluetooth profile separate from the dedicated receiver bond.

Current firmware still has five Bluetooth host profiles: tap Fn+1 through Fn+5 to select, or hold for five seconds without another key to replace that profile's pairing. Fn+0 clears the selected host bond; it is not recovery. Reducing the host count to the intended three is separate work. Right USB can keep the MCU powered while its battery switch is OFF; there is no documented right switch-sense GPIO in the vendor table.


## How watchdog recovery works

There is no separate USB startup shim and no replacement bootloader in the current design. A small application pre-initialization hook runs before normal Rust RAM initialization and RMK startup:

1. On a normal start, it arms the nRF52 hardware watchdog for approximately **10 seconds**, using reload channel 0. It clears only the stale pin-reset flag needed for this bootloader's reset-reason handling.
2. RMK adopts the already-running watchdog with matching configuration. Its watchdog task feeds it every **5 seconds**, cooperatively alongside the role's normal tasks.
3. If execution stalls and feeds stop, the hardware watchdog resets the MCU independently of the stalled code.
4. On the next application entry, the hook recognizes `RESETREAS.DOG`, clears that flag, writes `GPREGRET = 0x57`, and requests a system reset.
5. The unchanged factory bootloader consumes that request and opens its recovery route. Connect USB to access the UF2 drive; battery-only bootloader timeout behavior must not be assumed to hold recovery indefinitely.

The watchdog runs during sleep and pauses during debugger halt. The hook does not reconfigure an already-running watchdog. Normal Companion recovery also requests the factory bootloader, but uses the running USB handler directly and does not wait for a watchdog timeout. See the [hook](firmware/src/watchdog_recovery.rs) and [pinned RMK watchdog runner](https://github.com/sarimabbas/rmk-nocfree/blob/c8e06c6edbda227114ab7deeb9234d2ecca671c6/rmk/src/watchdog/nrf52.rs).

**Coverage:** a deliberately hung left diagnostic demonstrated the watchdog escape. Normal integrated Companion recovery was observed on all three roles. Deliberate-crash recovery has not been separately demonstrated on the integrated right or receiver images. A task can fail while the feeder continues running; the watchdog is not a per-task health monitor. Corrupted vectors, damaged hook code, or failure before watchdog arming still require another recovery route.

## Last-resort physical reset: left and right

Use this when neither Companion nor the watchdog gives access. It requires opening the affected half. The receiver has no established equivalent closed-case physical procedure; do not transpose a keyboard-half pad layout to it.

### Identify the contacts before powering the open board

1. Unplug USB and open the back carefully, preserving the screws and avoiding strain on the battery wires. Disconnect the battery by its plug housing if accessible; do not pull the wires or force an inaccessible connector.
2. Identify **ground** and the module's **RESET** contact from a matching revision's labeled pinout or verified tracing. The inspected left's UICR selects **P0.18** for reset; that is an MCU signal name, not an instruction to touch a particular exposed pad.
3. Confirm ground by continuity to a known battery-negative reference **only with the board unpowered**. If you cannot isolate the battery, do not use continuity/resistance mode on the board. A 0 V reading does not by itself prove ground, and approximately 3.3 V does not prove RESET.

The [official right-half recovery video](https://www.youtube.com/watch?v=jGfepV1DYVE) shows repeated contacts between the fourth and bottommost large pads on the module's right-side edge, in the video's orientation. These are module edge pads, not the chip's fine-pitch legs. **This ordinal description applies only to the demonstrated orientation/revision.** It does not establish a left-half map or a universal right-half map. Do not count along the horizontal bottom row or mirror the drawing onto another module.

During this project's left recovery, an owner-operated current-limited contact test used a resistor measured at about **0.9 kΩ** between a candidate reset contact and known battery negative; the UF2 drive then appeared. The owner also physically recovered the right and its drive was verified. Those outcomes establish recovery on those units; the contacted nets were not independently traced into a publishable pad map. Do not treat the earlier guessed Ebyte module pinout as verified.

### Reset a board with confirmed RESET and ground contacts

1. Secure the board on a nonconductive surface. Use insulated leads and a resistor around **1 kΩ** in series with the RESET-to-ground connection; arrange the ground end while power is disconnected.
2. Connect USB so the host can observe recovery. Briefly pull the confirmed RESET contact toward ground through the resistor, then release it. Repeat promptly to attempt the factory bootloader's double-reset entry. Do not hold reset continuously: the MCU cannot enumerate while held in reset.
3. Check for **NocFree &** and inspect `INFO_UF2.TXT` before writing anything. If recovery does not appear, stop and recheck contact identity and the board-specific bootloader behavior; do not try random adjacent pads.
4. Remove the leads and unplug USB before reassembly. Preserve a fresh readback before installing a correctly guarded application image. Recheck normal typing separately afterward.

This method can recover without RMK executing, provided the original bootloader and its reset-entry behavior remain intact. If RESET/ground cannot be identified confidently, obtain vendor guidance or a correctly identified SWD connection instead of improvising contacts. SWD mass erase or unlock can destroy the recovery baseline and is not part of this procedure.

## Build and update boundaries

Run `./scripts/check.sh` from the repository root before changing scanner or transport behavior. It runs host checks and cross-builds supported roles without flashing. Use the guard matching the image layout: `scripts/image_guard.py` checks ordinary `0x27000` applications; the current `0x1000` halves need the exact-BIN guard in `scripts/migration_guard.py` (see below). A passing build or guard proves neither real pin wiring nor recovery on a target device.

At this snapshot, normal left/right images use application origin `0x1000`, reclaiming the old S140 region; the receiver retains S140 and starts at `0x27000`. Storage begins at `0x65000`. The receiver candidate ends at page boundary `0x64000`; that is this artifact's boundary, not spare space guaranteed for future releases. Preserve MBR, bootloader, UICR and reserved regions. Do not change the factory bootloader to implement ordinary customization.

Logging/features or an RMK revision change can reset the framework's storage schema and bonds even if the updater does not directly write settings. After an update, check recovery, ordinary startup, Vial, wired/Bluetooth/receiver input, held-key release across disconnects, simultaneous split input and wake behavior. Measure latency before claiming a bound or lossless operation. Keep factory firmware, generated images, device identities and private captures out of public commits.


## Flash and RAM map

Address intervals below are start-inclusive, end-exclusive. These are this port's layout and inspected left bootloader findings, not an invitation to write entire regions.

| Flash interval | Current use |
| --- | --- |
| `0x00000..0x01000` | MBR; preserve |
| `0x01000..0x27000` | Reclaimed by left/right RMK; retained S140 on receiver |
| `0x27000..0x65000` | Remaining application capacity; receiver starts here |
| `0x65000..0x6d000` | RMK settings/storage; not application payload |
| `0x6d000..0x74000` | Reserved factory region; preserve |
| `0x74000..0x80000` | Bootloader and upper metadata/pages; preserve |

The linker gives RMK RAM `0x20008000..0x20020000` (96 KiB), leaving the lower 32 KiB reserved. UICR is a separate configuration address space, not ordinary application flash. Do not enable DCDC, change reset-pin configuration or alter protection settings based only on another Nordic board's defaults.

The old recovery-first experiment placed magic `0x87eeb07c` at application origin plus `0x200`. Current lower-layout images reserve that word as erased `0xffffffff`, with executable entry after it. Reintroducing the old marker can make ordinary USB startup enter recovery unexpectedly. Never patch it by hand to solve a startup problem.

## Reproducible developer builds

Install the toolchain selected by `rust-toolchain.toml`, including `thumbv7em-none-eabihf`, and an Arm GNU toolchain with newlib headers. Keep `arm-none-eabi-gcc` on PATH. Build from `firmware/` so Cargo finds the target flags and keyboard configuration.

These commands compile the current normal Mac-mode role compositions; **they do not package or flash**:

```sh
cd firmware
cargo build --locked --release --bin nocfree-rmk --no-default-features \
  --features defmt-logging,left,reclaimed-softdevice,mac-keymap,backlight-active-high,runtime-recovery
cargo build --locked --release --bin nocfree-rmk --no-default-features \
  --features defmt-logging,right,reclaimed-softdevice,mac-keymap,backlight-active-high,runtime-recovery
cargo build --locked --release --bin nocfree-rmk --no-default-features \
  --features defmt-logging,receiver,runtime-recovery
```

All three write the same `firmware/target/thumbv7em-none-eabihf/release/nocfree-rmk` path. Save each role's ELF before building the next, or use separate Cargo target directories. A filename does not prove a compiled role. Record source revision, lockfile hash, exact features, ELF/BIN/UF2 hashes, guard output and device identity in a private manifest.

For a packaged receiver UF2, run from the repository root:

```sh
python3 scripts/image_guard.py --image path/to/receiver.uf2
```

For a current lower-layout half, the guard binds the UF2 to the exact linked BIN, checks vectors and rejects the old recovery marker. Its historical function name still says “shim”; that does not mean the old USB startup shim is enabled:

```python
from pathlib import Path
from scripts.migration_guard import inspect_application_shim

result = inspect_application_shim(
    Path("path/to/left.uf2").read_bytes(),
    Path("path/to/left.bin").read_bytes(),
    role="left",  # Use "right" for the right image.
)
print(result)
```

The structural role argument does not identify the compiled role or board. Independently bind the build features and actual target. The updater must additionally protect every touched page and retain a rollback derived from that part's fresh backup. Do not use `cargo run` or a generic flashing runner.

### Feature and logging traps

- `defmt-logging` is the normal RTT logging choice; viewing RTT requires suitable debug access.
- `usb-log` and `battery-adc-diagnostic` use CDC logging on the left. They cannot be combined with `defmt-logging` because the radio dependency rejects both logging backends together.
- Diagnostic logging increases flash usage. The full Mac/backlight CDC ADC diagnostic exceeded the available flash in an earlier preparation build; do not change the linker boundary to make it fit.
- Feature changes can alter RMK's settings schema. Assume bonds/settings may reset until compatibility is checked explicitly.
- `usb-recovery-first`, `application-recovery-shim` and `usb-rescue-startup` are historical experiments, not defaults to add to a current runtime-recovery image.

## USB and battery protocol reference

These development VID/PID choices are not registered production allocations. They identify the role but do not substitute for a unique device/port binding.

| Runtime product | VID:PID |
| --- | --- |
| NocFree RMK | `4c4b:4643` |
| NocFree RMK Right | `4c4b:4671` |
| NocFree RMK Receiver | `4c4b:4644` |
| Factory UF2 bootloader observed in this project | `239a:0029` |

The runtime recovery interface is class/subclass/protocol `fe/01/01`. Discover its interface number from descriptors rather than hard-coding it. DETACH is a zero-payload local class request; it does not carry application bytes. Vial commands through the receiver target the **left**, whereas local DFU DETACH targets the **receiver**. Mixing those paths can reset the wrong component.

The read-only battery extension uses Vial raw HID usage page `ff60`, usage `61`, with 32-byte reports. Request bytes are:

```text
08 7e 01 01 4e 43 42 54 [24 zero bytes]
```

The first eight bytes identify CustomGetValue, channel/value, version and `NCBT`. A successful version-1 reply retains the header:

| Offset | Meaning |
| --- | --- |
| 8 | Status: 0 success, 1 unsupported version, 2 malformed request, 3 unavailable feature |
| 9–11 | Left: availability, percentage, charging state |
| 12 | Right link: 0 disconnected, 1 connected, 2 unconfigured |
| 13–15 | Right: availability, percentage, charging state |
| 16–31 | Reserved zeroes |

Availability is 0 unavailable, 1 available, 2 invalid. Percentage is 0–100 or `ff` unknown. Charging state is 0 unknown, 1 charging, 2 discharging. Disconnected/unconfigured right entries must be unavailable. Reject inconsistent or unsupported replies; never turn them into 0%. There is no sample timestamp: a successful query returns a producer cache, not a newly measured voltage. Poll no faster than the normal 30-second sampling interval. Keep setters/save/reset commands separate from telemetry.

### Status version 2 and current mode

Companion now requests battery/status version 2 (`08 7e 01 02 4e 43 42 54`), falling back to v1 only on a valid unsupported-version reply. Bytes 8–15 keep their v1 meanings. The additional successful-reply bytes are:

| Byte | Meaning |
| --- | --- |
| 16 | Flags: bit 0 left USB power, bit 1 right USB power, bit 2 active wired route, bit 3 active Bluetooth route, bit 4 active receiver route |
| 17 | Selected policy: 0 unknown/off/automatic, 1 wired, 2 Bluetooth, 3 receiver |
| 18 | Known mask: bit 0 left power, bit 1 right power, bit 2 active route, bit 3 selected policy |
| 19–31 | Reserved zeroes |

An older right leaves right power unknown; a disconnected right clears that observation. USB power, selected policy, active typing route and recovery are different facts. Companion expires connectivity facts after 45 seconds without a successful query while retaining battery estimates separately. A receiver reply is forwarded from the left: a successful query establishes live communication, although its battery measurement can still be cached.

The read-only mode getter `08 7e 03 01 4e 43 4d 4f` (`NCMO`) returns status in byte 8 and policy in byte 9; bytes 10–31 are zero. It reads RMK's current selection rather than waiting for the next ADC observation. Wireless-only host mode is not automatically observable through these USB getters.

### Raw ADC diagnostics

The `NCAD` getter avoids adding CDC logging and retains the normal feature set. Request `08 7e 02 01 4e 43 41 44`, followed by 24 zero bytes. Its 32-byte reply retains that header:

| Offset | Meaning |
| --- | --- |
| 8 | Status: 0 available, 1 unsupported version, 2 malformed request, 3 unavailable |
| 9–10 | Signed little-endian ADC count |
| 11–14 | Saturating little-endian sample age, milliseconds |
| 15 | Switch bits: bit 0 present, bit 1 Bluetooth input high, bit 2 receiver input high |
| 16–19 | SAADC resolution register |
| 20–23 | SAADC oversampling register |
| 24–27 | Channel configuration register |
| 28–31 | Positive input-selection register |

The four registers are little-endian u32 values. Error replies contain no measurement payload. The getter observes the existing sample; it does not trigger sampling or change conversion. Switch levels are cached with the ADC observation, not a live position query. Raw observations have been collected on the left; a sample is useful only with its age, register configuration and correctly identified voltage reference. It does not prove the physical divider ratio.

## Troubleshooting without guessing

| Symptom | First useful check |
| --- | --- |
| No recovery drive | Confirm the selected part, USB data cable and runtime identity; distinguish an absent device from a rejected DETACH or an unmounted bootloader |
| Drive appears on ordinary startup | Check image history for the old recovery marker or watchdog-reset evidence; do not immediately change the bootloader |
| Left types but right does not | Check split connection and right battery power; right USB recognition is not proof of a live split link |
| Typing works but receiver telemetry is unsupported | Confirm the keyboard is actually connected to the receiver; a disconnected Vial relay can return Unhandled |
| Right battery displayed after disconnect | Check the link flag before displaying its cached value |
| Low/fluctuating battery percentage | Pair a raw ADC sample/configuration with a confirmed cell-voltage measurement; USB presence and smoothing cannot establish calibration |
| Update boots but loses pairing | Compare RMK revision, enabled features and storage schema; application-only copying can still trigger framework settings reset |
| Keys remain held after switching/disconnect | Reproduce with a held modifier across the transition; inspect RMK output release and reconnect behavior |

For an actionable bug report, include the source commit, role and feature list, host OS, exact physical steps, observed USB product, whether Companion recovery works, and whether the result repeats. Share redacted logs and hashes, not factory images, addresses, serial numbers, bond keys or private backups. A minimal reproduction is more useful than a screenshot claiming “Bluetooth connected.”


## Battery calibration: what changed and what did not

Both halves now use `BatteryProcessor::new(100, 150)`, an **effective provisional scale**, rather than the vendor-published 130/100. This must not be documented as a measured resistor ratio or accurate fuel gauge. The old left samples around 3180–3192 with registers `[2, 0, 0x20000, 3]` correspond to roughly 3.63–3.65 V at 1.3, versus 4.19–4.21 V at 1.5 under the nominal 12-bit/internal-reference/1⁄6-gain model. A 40 µs acquisition trial did not resolve the discrepancy; normal acquisition was retained.

Inspection of the supplied factory v2.4.5 images found an extra normalization against a stored/learned full reference. At its default reference, the combined software conversion approximates `raw × 4200 / 4095 × 1.3`, rather than simply applying the README's 1.3 factor to a 3.6 V ADC model. This explains why copying only that published multiplier can produce a very different display. It does not establish the actual cell voltage, charging current or hardware resistance. Factory calibration records are not a flash region to copy into RMK.

The correction makes the estimates more plausible on the owner's boards. A simultaneous confirmed cell-voltage/raw-count comparison across multiple voltages, charge-current observation, and discharge-capacity test remain necessary for real calibration. No charging assertion or battery-life claim follows from a high displayed percentage.

## Power, interrupts and lighting

- **USB power is local:** each USB-powered half stays awake regardless of selected host mode or USB suspend. The right otherwise follows the left's sleep request. Battery idle sleep is 1,800 seconds in `keyboard.toml`; it is RMK sleep coordination, not electrical power-off. The full 30-minute first-key wake acceptance and measured endurance remain pending.
- **Interrupt scanning is opt-in:** `async-scanner` makes the custom expander scanner wait on active-low INT when idle. Merely enabling RMK's direct-matrix feature cannot adapt this I²C board. Active keys, unfinished debounce and failed reads keep scanning. Both halves' interrupt candidates were installed/read back and typing/recovery observed; those results do not measure wake latency or prove absolute losslessness.
- **PCA9555 acknowledgement matters:** the adapter parks each expander's command pointer at register 2 after reading inputs, before accessing another slave. This sends only a command byte, not output data. The [TI datasheet, section 8.4.1.1](https://www.ti.com/lit/ds/symlink/pca9555.pdf) describes an interrupt-reset erratum when an input-register pointer remains selected. Preserve this behavior when replacing the scanner.
- **Zero brightness must stop PWM:** the current RMK/HAL integration waits for STOPPED and disables the generator, preserving DMA buffer lifetime across cancellation. A zero duty value alone is not proof the peripheral has stopped. Nonzero brightness/wake restarts output; stored brightness survives. Measure current before claiming savings.
- **Disconnect cancels brightness hold:** a battery-powered right darkens while seeking the left; local USB power preserves its lighting. Firmware leaves shared charger-red control alone. Left blue blinks while seeking either wireless route, stays on for 30 seconds after connecting, then turns off; wired/sleep turns it off. Older purple/double-pulse experiments are superseded.

## Companion extension points and safe repeatable journeys

Companion's authoritative workflows use **Statig**. The UI derives its screen from machine state rather than keeping a second collection of workflow flags. Parent machines reuse recovery, per-part firmware transfer, backup and pairing children. Generation tickets reject stale/duplicate completions after cancellation or advancing to another part; completing a state transition alone must not perform device I/O.

Useful modules are `desktop/src/recovery_journey.rs`, `peripheral_journey.rs`, `firmware_journey.rs`, `install_journey.rs`, `factory_source.rs` and `release.rs`. Opening a page is passive; an explicit Next starts work. Cancellation stops future work but cannot undo an already accepted pairing mutation or partially completed firmware write.

Installation orders **receiver → right → left**, retaining factory left until the receiver is migrated. Factory restoration orders **left → right → receiver**, restoring the left first so factory receiver recovery is available. Each part receives a fresh private backup before a one-shot copy and exact readback verification. Identical application bytes skip copying. A persisted pending write is reconciled against its original backup and fresh device readback; it is never retried merely because the app restarted. Retain the original factory backups across subsequent RMK updates and supplied-file restores.

The first complete factory backup contains the S140 needed to return a reclaimed half to stock. An official application-only UF2 alone is insufficient. Restore may use saved originals or separately validated supplied left/right/receiver files, using the complete backup as the system/settings donor. Preserve originals and reject unknown role/layout combinations. No factory firmware is bundled; no MBR, bootloader or UICR writes belong to these journeys.

Factory left and receiver share a USB identity. Require a role-specific, correlated connection rather than guessing from their product name. The latest checkbox-based scope selection is still being developed in the other conversation; its UI is not an installation proof. Once a journey begins, its chosen roles must remain fixed even when guided steps intentionally unplug a part.

Check pairing queries links without clearing healthy bonds. Its `NCPR` protocol begins `08 7e 04 01 4e 43 50 52`; explicit repair is a separate operation addressing the left or receiver and its selected peer. The receiver handles its pairing command locally even without a keyboard link. Completion requires fresh reciprocal peer identities and an encrypted link, plus a right split-link observation. Repair clears only dedicated receiver bonds, preserving ordinary Bluetooth host profiles; cancellation does not restore a cleared bond. Treat peer identities as private data.

The local firmware package is generated by `scripts/package_companion_release.py`; `--check` validates without accessing devices. The pinned manifest binds each role's exact UF2/BIN hashes, bounds files, rejects unknown fields and reruns image guards. It deliberately packages reviewed installed candidates rather than whichever build is newest. Bump firmware version and release identity when shipped bytes change. Version strings support preflight; exact readback is final byte proof.

The current package is a local Mac-keymap/ANSI development release, not a separately tested Windows/Linux release. Individual recovery, typing and protocol results do not establish a complete repeated factory → RMK → factory roundtrip. That remains a hardware acceptance gate. Guided all-mode checks include `qwert HJKL h` with left Shift controlling right capitals and right lowercase after release; a host connection indicator alone cannot pass them.

### macOS backup permissions

A denied removable-volume permission can make a mounted recovery drive unreadable. Report the permission error rather than waiting endlessly for a drive that already exists. If necessary, quit Companion, reset only its removable-volume decision, then reopen it and explicitly allow the next request:

```sh
tccutil reset SystemPolicyRemovableVolumes io.github.sarimabbas.nocfree-companion
```

This does not reset Bluetooth or grant access automatically. Preserve the app's bundle identity/signing across rebuilds. Do not recommend broad permission resets or disabling platform protections as a routine recovery step.

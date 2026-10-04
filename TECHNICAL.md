# NocFree AND RMK: customization and recovery guide

For developers customizing this community firmware. Hardware observations below describe checkpoint `dc671f3`, October 3, 2026. Prepared source additions through `2497d04` are identified separately. Physical-switch acceptance and battery calibration are ongoing; this is not a claim that every factory feature or every PCB revision is supported.

## Start here

Use **NocFree Companion → Recovery mode**, choose the part, and connect that part by a USB data cable. The running RMK firmware exposes a local USB DFU **runtime DETACH** interface. Companion requests the original factory bootloader, which presents the **NocFree &** drive. This flow has been exercised on the left, right and receiver, with exact application readbacks.

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
| Passive physical selector input (prepared source) | `firmware/src/mode_switch.rs` |
| Startup watchdog escape | `firmware/src/watchdog_recovery.rs` |
| Dimensions, profile count and split configuration | `firmware/keyboard.toml` |
| Companion guided recovery | `desktop/src/recovery.rs`, `desktop/src/recovery_journey.rs` |
| Companion battery transport | `desktop/src/battery.rs`, `desktop/src/battery_vial.rs` |

Dependencies and the RMK fork revision are pinned in `firmware/Cargo.toml` and `firmware/Cargo.lock`. The snapshot uses fork commit `c8e06c6edbda227114ab7deeb9234d2ecca671c6`; prepared source at `2497d04` pins `acd4689a1284f27decd4d0755a1e316fddcbb8ff`. Follow the checked-out source, rather than historical diagnostic instructions, when changing features.

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

The hardware-tested baseline still uses Fn+U for receiver selection and Fn+B for USB/BLE preference. Prepared source at `2497d04` removes those default mappings and uses the two passive switch inputs, debounced for 25 ms, to request strict RMK output selection: top receiver, middle wired, bottom Bluetooth. Invalid startup input disables output; an invalid transition retains the last valid choice. Radio modes ignore USB for keyboard output while local USB recovery remains available. This routing implementation is prepared, not yet hardware-accepted here. The right can remain powered by USB while its battery switch is OFF; there is no documented right switch-sense GPIO in the vendor table.

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

The first eight bytes identify CustomGetValue, channel/value, version and `NCBT`. A successful reply retains the header:

| Offset | Meaning |
| --- | --- |
| 8 | Status: 0 success, 1 unsupported version, 2 malformed request, 3 unavailable feature |
| 9–11 | Left: availability, percentage, charging state |
| 12 | Right link: 0 disconnected, 1 connected, 2 unconfigured |
| 13–15 | Right: availability, percentage, charging state |
| 16–31 | Reserved zeroes |

Availability is 0 unavailable, 1 available, 2 invalid. Percentage is 0–100 or `ff` unknown. Charging state is 0 unknown, 1 charging, 2 discharging. Disconnected/unconfigured right entries must be unavailable. Reject inconsistent or unsupported replies; never turn them into 0%. There is no sample timestamp: a successful query returns a producer cache, not a newly measured voltage. Poll no faster than the normal 30-second sampling interval. Keep setters/save/reset commands separate from telemetry.

### Raw ADC diagnostics (prepared source)

The newer `NCAD` getter avoids adding CDC logging and retains the normal feature set. Request `08 7e 02 01 4e 43 41 44`, followed by 24 zero bytes. Its 32-byte reply retains that header:

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

The four registers are little-endian u32 values. Error replies contain no measurement payload. The getter observes the existing sample; it does not trigger sampling or change conversion. Switch levels are cached with the ADC observation, not a live position query. This prepared contract needs device acceptance before using it as electrical evidence.

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

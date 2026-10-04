# NocFree AND RMK: customization and recovery guide

For developers customizing this community firmware. This describes the implementation and owner-device observations at checkpoint `dc671f3`, October 3, 2026. Physical-switch integration and battery calibration are ongoing; this is not a claim that every factory feature or every PCB revision is supported.

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
| Startup watchdog escape | `firmware/src/watchdog_recovery.rs` |
| Dimensions, profile count and split configuration | `firmware/keyboard.toml` |
| Companion guided recovery | `desktop/src/recovery.rs`, `desktop/src/recovery_journey.rs` |
| Companion battery transport | `desktop/src/battery.rs`, `desktop/src/battery_vial.rs` |

Dependencies and the RMK fork revision are pinned in `firmware/Cargo.toml` and `firmware/Cargo.lock`. The snapshot uses fork commit `c8e06c6edbda227114ab7deeb9234d2ecca671c6`. Follow the checked-out source, rather than historical diagnostic instructions, when changing features.

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

At this snapshot, the physical switch does **not** yet enforce transport selection in RMK. Fn+U selects the receiver; Fn+B toggles USB/BLE preference. Do not mistake these temporary conventions for the intended final switch behavior. The right can remain powered by USB while its battery switch is OFF; there is no documented right switch-sense GPIO in the vendor table.

## How watchdog recovery works

There is no separate USB startup shim and no replacement bootloader in the current design. A small application pre-initialization hook runs before normal Rust RAM initialization and RMK startup:

1. On a normal start, it arms the nRF52 hardware watchdog for approximately **10 seconds**, using reload channel 0. It clears only the stale pin-reset flag needed for this bootloader's reset-reason handling.
2. RMK adopts the already-running watchdog with matching configuration. Its watchdog task feeds it every **5 seconds**, cooperatively alongside the role's normal tasks.
3. If execution stalls and feeds stop, the hardware watchdog resets the MCU independently of the stalled code.
4. On the next application entry, the hook recognizes `RESETREAS.DOG`, clears that flag, writes `GPREGRET = 0x57`, and requests a system reset.
5. The unchanged factory bootloader consumes that request and opens its recovery route. Connect USB to access the UF2 drive; battery-only bootloader timeout behavior must not be assumed to hold recovery indefinitely.

The watchdog runs during sleep and pauses during debugger halt. The hook does not reconfigure an already-running watchdog. Normal Companion recovery also requests the factory bootloader, but uses the running USB handler directly and does not wait for a watchdog timeout. See the [hook](../firmware/src/watchdog_recovery.rs) and [pinned RMK watchdog runner](https://github.com/sarimabbas/rmk-nocfree/blob/c8e06c6edbda227114ab7deeb9234d2ecca671c6/rmk/src/watchdog/nrf52.rs).

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

Run `./scripts/check.sh` from the repository root before changing scanner or transport behavior. It runs host checks and cross-builds supported roles without flashing. Validate each packaged UF2 with `python3 scripts/image_guard.py --image PATH`. A passing build or guard proves neither real pin wiring nor recovery on a target device.

At this snapshot, normal left/right images use application origin `0x1000`, reclaiming the old S140 region; the receiver retains S140 and starts at `0x27000`. Storage begins at `0x65000`. The receiver candidate ends at page boundary `0x64000`; that is this artifact's boundary, not spare space guaranteed for future releases. Preserve MBR, bootloader, UICR and reserved regions. Do not change the factory bootloader to implement ordinary customization.

Logging/features or an RMK revision change can reset the framework's storage schema and bonds even if the updater does not directly write settings. After an update, check recovery, ordinary startup, Vial, wired/Bluetooth/receiver input, held-key release across disconnects, simultaneous split input and wake behavior. Measure latency before claiming a bound or lossless operation. Keep factory firmware, generated images, device identities and private captures out of public commits.

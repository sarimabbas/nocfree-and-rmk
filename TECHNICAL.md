# Technical guide

This guide explains the keyboard firmware, Companion app, recovery steps and build commands.

## Keyboard setup

The keyboard images use the ANSI NocFree AND layout. The right half sends keys to the left half. The left half sends them to the computer through USB, Bluetooth or the dongle. Install RMK on the dongle to use it with the RMK keyboard firmware.

Open work is tracked in [GitHub issues](https://github.com/sarimabbas/nocfree-and-rmk/issues).

## Architecture and source map

```text
Right inputs → board scanner → RMK split BLE → Left keymap
                                                ├→ USB keyboard
                                                ├→ Bluetooth keyboard
                                                └→ RMK dongle → USB keyboard

Companion → local USB recovery request → selected part’s factory bootloader
Companion/Vial → left USB, or dongle relay → left configuration
```

RMK owns key actions, debounce, transports, Bluetooth profiles, storage and lighting synchronization. Custom board code adapts the PCA9555 expanders, physical mode switch, battery ADC and startup recovery. Keep changes at those seams.

| Concern | Source |
| --- | --- |
| Expander mapping and complete input snapshots | `crates/nocfree-input/` |
| Scanner and interrupt idle behavior | `firmware/src/scanner.rs` |
| Keymap | `firmware/src/keymap.rs` |
| Firmware composition | `firmware/src/main.rs` |
| Battery sampling | `firmware/src/battery.rs` |
| Physical mode selection | `firmware/src/mode_switch.rs` |
| Startup watchdog recovery | `firmware/src/watchdog_recovery.rs` |
| Keyboard dimensions and sleep settings | `firmware/keyboard.toml` |
| Shared recovery journey | `desktop/src/recovery_journey.rs` |
| Per-part transfer and backup journeys | `desktop/src/peripheral_journey.rs`, `desktop/src/firmware_journey.rs` |
| Whole-keyboard installation | `desktop/src/install_journey.rs` |
| Factory input validation | `desktop/src/factory_source.rs` |
| Bundled firmware validation | `desktop/src/release.rs` |
| Battery/status transport | `desktop/src/battery.rs`, `desktop/src/battery_vial.rs` |

Each Cargo manifest and lockfile fixes the dependency versions. The firmware and desktop use separate Rust toolchains.

## Board reference

These GPIO mappings come from the [vendor porting guide](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/README.md#4-pins-required-for-zmk-porting). The table uses Nordic GPIO names.

| Signal | Left | Right |
| --- | --- | --- |
| PCA9555 interrupt | P0.31 | P0.05 |
| I²C SDA / SCL | P0.11 / P1.09 | P0.11 / P1.09 |
| Bluetooth selector, active low | P0.15 | Not documented |
| Dongle selector, active low | P0.17 | Not documented |
| Shared red charging/status indicator | P0.09 | P0.17 |
| Backlight | P0.20 | P0.20 |
| Blue indicator | P0.10 | Not documented |
| Battery ADC | P0.04 | P0.04 |
| Divider enable, active high | P0.05 | P0.31 |

Use pull-ups for interrupt and selector inputs. Use open-drain output for a shared charging/status line. Expanders use addresses `0x20`, `0x22`, `0x24`; read both ports in order. The vendor’s KR mapping also uses `0x21/P0`.

After input reads, the scanner parks each expander’s command pointer at register 2 before accessing another slave. This writes the register address. Preserve this behavior: [TI PCA9555 datasheet, section 8.4.1.1](https://www.ti.com/lit/ds/symlink/pca9555.pdf) documents an interrupt-reset erratum when the input register remains selected. Keep the last input state when a read fails.

The physical left switch selects top dongle, middle wired, bottom Bluetooth. Inputs are debounced for 25 ms. An invalid startup selection disables output; an invalid transition retains the last valid choice. USB attachment remains separate from the selected typing route.

Tap Fn+1 through Fn+5 to select a Bluetooth host profile. Hold the combination for five seconds without another key to replace that profile’s pairing. In RMK, Fn+0 clears the selected host pairing. Use Companion to enter recovery mode.

## Power and lighting

A USB-powered half stays awake regardless of the selected connection mode. On battery, the right follows the left’s sleep request. The configured idle period is 1,800 seconds. The board stays powered during sleep.

`async-scanner` waits for the expander interrupt when idle. Active keys, unfinished debounce and read failures continue scanning. The board scanner reads keys through I²C.

Backlights use 400 Hz PWM with 16 brightness levels, saved brightness and split synchronization. Zero brightness stops and disables the PWM generator; wake or nonzero brightness restarts it. The DMA buffer remains valid through cancellation. A battery-powered disconnected right darkens while seeking the left, and disconnect cancels brightness holds.

The left blue indicator blinks while seeking either wireless route. After connection it stays on for 30 seconds, then turns off. Wired mode uses the shared red indicator’s existing behavior. Charging can mix red with blue. The right has no host-mode indicator because it only links to the left.

Battery conversion uses `BatteryProcessor::new(100, 150)`. Calibration work is tracked in [issue #7](https://github.com/sarimabbas/nocfree-and-rmk/issues/7). To adjust the conversion, compare raw ADC counts with cell voltage and discharge measurements.

## Recovery and backup

Use **Companion → Enter recovery mode**, choose a part, and follow the instructions. Compatible RMK images expose a local USB DFU runtime DETACH interface. Companion addresses the selected part directly. The unchanged factory bootloader presents the **NocFree &** drive.

Keep a separate original backup for each half and the dongle. Backups contain the application and system settings. The MBR, bootloader and UICR stay on the board. When restoring an application-only factory UF2, Companion can use the original backup to restore system settings. Use the backup for the selected part.

USB can power the right MCU while its battery switch is OFF. If the right remains in recovery after a write, turn it OFF and remove USB before restarting as instructed. For the factory right-half Fn+0 recovery procedure, keep the left connected.

The startup hook arms a roughly 10-second hardware watchdog before normal RMK initialization. RMK feeds it every five seconds. A watchdog reset causes the hook to request the factory bootloader using `GPREGRET = 0x57`. Companion can also request recovery directly through USB. The watchdog resets the board when its feeder stops.

For physical recovery, identify RESET and ground on the board revision first. The [vendor video](https://www.youtube.com/watch?v=jGfepV1DYVE) shows the right half. Keep the bootloader, UICR and protected regions intact.

### macOS volume permission

If macOS blocks access to the recovery drive, quit Companion and reset its removable-volume permission:

```sh
tccutil reset SystemPolicyRemovableVolumes io.github.sarimabbas.nocfree-companion
```

Reopen Companion and allow access when macOS asks.

## App state machines

Companion uses Statig state machines. The UI derives the screen and available buttons from the machine state. Parent journeys reuse recovery, transfer, backup and pairing machines. Generation tickets reject stale or duplicate results after cancellation or a change of part.

Opening a page shows the first step. Click Next to move through each step. Next becomes available when the step is complete, and the spinner stops. Completed progress stays visible. A successful typing test enables Next as soon as the connection conditions are met.

RMK installation orders dongle → right → left. Factory restoration orders left → right → dongle. Selected parts remain fixed during a journey, even when a step asks for disconnection. The factory left and dongle can share USB descriptors. Companion asks you to unplug and reconnect a part to identify it.

Each transfer saves a fresh backup, writes the image once, reads it back to check the bytes, and checks normal startup. If a transfer stops, the app checks the original backup and a new readback before another write. This record persists when you reopen the app or switch pages.

Check pairing reads the connection state. Repair clears and replaces the dongle pairing when requested. It keeps the Bluetooth host profiles. A successful check confirms the left-to-dongle and left-to-right connections, their device identities and link encryption.

Whole-keyboard installation ends with wired, Bluetooth and dongle typing checks. The test `qwert HJKL h` exercises left input, right input, cross-half Shift and modifier release.

## Build and test

Install Rust through rustup and use the checked-in toolchains. Firmware also needs Arm GNU bare-metal GCC with newlib headers. Linux CI uses `gcc-arm-none-eabi` and `libnewlib-arm-none-eabi`. The P-256 dependency uses the Arm compiler.

```sh
./scripts/check.sh
```

This runs host checks and firmware cross-builds without accessing a device. For the desktop:

```sh
cd desktop
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
```

On macOS, with a validated release firmware package in `dist/companion-firmware/`:

```sh
./desktop/build-macos.sh
```

Download `companion-firmware.zip` from the firmware release and use `scripts/unpack_companion_release.py` to extract it for app builds. `scripts/package_companion_release.py` creates firmware packages from images and device test records.

Firmware roles are `left`, `right`, `receiver`; select exactly one. Production halves use the explicit `reclaimed-softdevice` layout with runtime recovery. The receiver preserves resident S140. Role builds share an output name, so use separate target directories or save each ELF before another build. Read the checked-in feature definitions before selecting a diagnostic feature.

Run `scripts/image_guard.py` for receiver images and the UF2/BIN-bound checks in `scripts/migration_guard.py` for lower-layout halves. These tools check image addresses, vectors and board family. Match the role, build features and image to the selected board before installation.

Before transport or scanner changes, run the repeatable host harness and cross-build all supported roles. Test disconnect and key release, simultaneous input, wake on the first key, recovery entry and all connection modes on the keyboard.

## USB protocol reference

The development runtime identities are `4c4b:4643` (left), `4c4b:4671` (right) and `4c4b:4644` (dongle). The observed factory UF2 identity is `239a:0029`. Discover the DFU interface number from its `fe/01/01` descriptors; do not hard-code it. DFU DETACH targets the local part, while Vial requests through the dongle target the left.

Read-only custom Vial getters use usage page `ff60`, usage `61`, and 32-byte reports. Headers identify battery/status (`NCBT`), selected mode (`NCMO`) and raw ADC (`NCAD`). Dedicated pairing uses `NCPR`. Use the versioned serializers and validators in the source. Mark an unsupported or inconsistent reply as unknown.

Status separates USB power, selected connection policy, active typing route and recovery. Battery values use cached measurements. Connection observations and battery estimates have separate expiry times. Mark a missing USB reply as unknown.

## Diagnostics and bug reports

**Help → Show logs** opens local logs. **Help → Export diagnostic logs** saves a ZIP with fixed-state events and an app-window screenshot when capture is available.

Logs rotate across four files capped at 256 KiB each. They record version, journey state, part and test mode, operation outcomes, USB changes and a 30-second heartbeat. Typed text, firmware bytes, raw error/panic payloads and device identifiers are excluded. An unclean-exit marker is reported on the next successful launch. A gap between heartbeats helps locate a hang.

If the app hangs, force quit it, reopen it and export logs. If it cannot reopen on macOS, logs are under `~/Library/Logs/NocFree RMK Companion/`. Backups are under `~/Library/Application Support/NocFree Companion/`.

Include the app version, OS, affected part, switch positions, USB connections, exact steps and whether recovery still works. An issue needs the exported logs or an explanation of why export failed.

## Release and license maintenance

Project code is MIT. Dependencies, fonts, icons and bundled SDK code keep their own licenses. Preserve `LICENSE`, `NOTICE.md`, `desktop/THIRD_PARTY_NOTICES.md`, `docs/notices/inventory.json` and the hashed source texts under `docs/notices/texts/`.

```sh
python3 scripts/companion_notices.py --inventory --check
python3 scripts/companion_notices.py /tmp/NocFree-Third-Party-Notices.md
python3 scripts/companion_notices.py /tmp/NocFree-Third-Party-Notices.md --strict --check
```

The full generated appendix belongs in distributed app archives. Strict validation rejects missing or changed recorded text and stale source graph hashes. Version-only manifest updates still require a locked graph comparison before refreshing the inventory hash. Dependency or firmware-pin changes require recapture and review against exact immutable source versions.

When an upstream archive omits a license file, the inventory stores the original license declaration and available copyright headers. It records standard SPDX terms separately.

Release jobs build the app first. A protected job signs the build with a temporary keychain in the main-branch release environment. Verify the archive checksums, bundled firmware manifest and license notices. On macOS, also check the Developer ID signature, notarization ticket and Gatekeeper result.

## Windows and Linux

All three app builds use the same keyboard images and state machines.

Linux requires a desktop with Vulkan, X11 or Wayland, the libraries listed in the archive's README.txt, BlueZ for Bluetooth status, and zip for log export. Install the included udev rule to give your desktop session access to the keyboard, then reconnect the devices. Open the recovery drive in the file manager. Run Companion as your desktop user.

Windows uses its Bluetooth connection-status API. Automatic USB recovery can require WinUSB on the DFU interface. Keep the existing HID and mass-storage drivers. If automatic entry is unavailable, use the displayed physical recovery procedure.

Backups and app data use `LOCALAPPDATA/NocFree Companion` on Windows and `XDG_DATA_HOME/NocFree Companion` (or `~/.local/share/NocFree Companion`) on Linux. Logs use `LOCALAPPDATA/NocFree RMK Companion/Logs` on Windows and `XDG_STATE_HOME/nocfree-rmk-companion` (or `~/.local/state/nocfree-rmk-companion`) on Linux. Windows uses the user profile’s file permissions. File writes use write-through handles and explicit flushing. Before another write, the app checks any incomplete transfer against the backup and a new readback.

# Technical guide

This document covers the NocFree AND RMK port and Companion app. Public source contains no factory firmware, private backups, signing credentials or device-specific acceptance records.

## Support and known limits

The shipped keyboard images target the ANSI NocFree AND. Typing, cross-half Shift, Bluetooth pairing and reconnection, dongle typing, backlight controls, and per-part recovery have been exercised on the maintainer’s keyboard with macOS. This does not establish support for every hardware revision, layout or host OS. Compilation and host tests are separate from hardware acceptance.

Battery percentages use a provisional conversion. They are not a calibrated capacity gauge. Wake observations are not measured latency, current or battery endurance.

RMK’s split BLE disconnect path can leave a key held at the host when the right half loses power while that key is down. Release recovery after this failure is not guaranteed. This is tracked as an upstream limitation; the board port does not add a second transport implementation.

The right half is a split peripheral, not an independent USB keyboard. The dongle must also run RMK; an unchanged factory dongle uses a different wireless protocol. Factory web configuration and the optional number pad are not supported by this port.

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

Dependencies are pinned in each Cargo manifest and lockfile. The firmware and desktop use separate Rust toolchains. Follow current source and build scripts, not an older release’s diagnostic commands.

## Board reference

These GPIO mappings come from the [vendor porting guide](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/README.md#4-pins-required-for-zmk-porting). They are published mappings, not measurements of every PCB revision. Nordic GPIO names differ from Arduino aliases.

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

Use pull-ups for interrupt and selector inputs. Do not drive a shared charging/status line push-pull. Expanders use addresses `0x20`, `0x22`, `0x24`; read both ports in order. The vendor’s KR mapping adds `0x21/P0`, but that layout is not accepted by this release.

After input reads, the scanner parks each expander’s command pointer at register 2 before accessing another slave. This sends a command byte, not output data. Preserve this behavior: [TI PCA9555 datasheet, section 8.4.1.1](https://www.ti.com/lit/ds/symlink/pca9555.pdf) documents an interrupt-reset erratum when the input register remains selected. Failed reads must not manufacture releases.

The physical left switch selects top dongle, middle wired, bottom Bluetooth. Inputs are debounced for 25 ms. An invalid startup selection disables output; an invalid transition retains the last valid choice. USB attachment remains separate from the selected typing route.

Tap Fn+1 through Fn+5 to select a Bluetooth host profile. Hold the combination for five seconds without another key to replace that profile’s pairing. Fn+0 clears the selected host bond in RMK; it is not an RMK recovery shortcut. Factory recovery shortcuts apply only to factory firmware.

## Power and lighting

A USB-powered half stays awake regardless of the selected connection mode. On battery, the right follows the left’s sleep request. The configured idle period is 1,800 seconds. Sleep is not electrical power-off.

`async-scanner` waits for the expander interrupt when idle. Active keys, unfinished debounce and read failures continue scanning. Enabling RMK’s direct-matrix option alone cannot adapt this I²C board.

Backlights use 400 Hz PWM with 16 brightness levels, saved brightness and split synchronization. Zero brightness stops and disables the PWM generator; wake or nonzero brightness restarts it. The DMA buffer remains valid through cancellation. A battery-powered disconnected right darkens while seeking the left, and disconnect cancels brightness holds. Current savings require measurement.

The left blue indicator blinks while seeking either wireless route. After connection it stays on for 30 seconds, then turns off. Wired mode uses the shared red indicator’s existing behavior. Charging can mix red with blue; the mixed color does not identify a separate mode. The right has no host-mode indicator because it only links to the left.

Battery conversion uses `BatteryProcessor::new(100, 150)`. This is a provisional effective scale, not a measured resistor ratio. USB power does not prove that charging current is flowing. Verify raw ADC counts against confirmed cell voltage and discharge behavior before changing or claiming calibration.

## Recovery and backup

Use **Companion → Enter recovery mode**, choose a part, and follow the instructions. Compatible RMK images expose a local USB DFU runtime DETACH interface. Companion addresses the selected part directly. The unchanged factory bootloader presents the **NocFree &** drive.

Keep separate original backups for left, right and dongle. Recovery readbacks exclude the MBR, bootloader and UICR; they are not complete-chip backups. An application-only factory UF2 can require the original backup as the system/settings donor when returning from RMK. Never interchange roles or treat an arbitrary selected file as an original backup. No factory image is bundled or downloaded.

USB can power the right MCU while its battery switch is OFF. If the right remains in recovery after a write, turn it OFF and remove USB before restarting as instructed. Factory right recovery can require a live left connection for the Fn+0 procedure; do not isolate the left before that step.

The startup hook arms a roughly 10-second hardware watchdog before normal RMK initialization. RMK feeds it every five seconds. A watchdog reset causes the hook to request the factory bootloader using `GPREGRET = 0x57`. Normal Companion recovery requests it directly through USB. Deliberate hang escape was demonstrated on the left; this is not proof of crash escape on every role or of recovery after corrupted vectors. The watchdog cannot detect a failed task while its feeder still runs.

Last-resort physical reset requires confirmed RESET and ground contacts for the exact board revision. The [vendor’s right-half recovery video](https://www.youtube.com/watch?v=jGfepV1DYVE) applies to the demonstrated orientation and board. Do not infer a left or dongle pad map from it. Do not alter the bootloader, UICR or protected regions, or use a generic erase/recover command.

### macOS volume permission

A denied removable-volume permission can leave a mounted recovery drive unreadable. Companion should show the error rather than wait for an absent drive. Quit the app and reset only this decision if needed:

```sh
tccutil reset SystemPolicyRemovableVolumes io.github.sarimabbas.nocfree-companion
```

Reopen Companion and allow its next request. This command does not grant access automatically. Do not reset unrelated permissions or disable platform protection.

## Authoritative journeys

Companion uses Statig state machines. The UI derives the screen and available buttons from the machine state. Parent journeys reuse recovery, transfer, backup and pairing machines. Generation tickets reject stale or duplicate results after cancellation or a change of part.

Opening a page is passive. Click Next to move through the journey. Conditions can enable Next, but cannot advance the screen automatically. Completed progress remains visible. A ready Next button must not accompany a waiting spinner. Typed test input enables Next immediately when the test and connection conditions hold.

RMK installation orders dongle → right → left. Factory restoration orders left → right → dongle. Selected parts remain fixed during a journey, even when a step asks for disconnection. Factory left and dongle can share USB descriptors, so an ambiguous part needs a correlated unplug/reconnect identification step; layout names alone are insufficient.

Each transfer keeps a fresh backup, writes once, verifies exact readback, and checks normal startup. A persisted incomplete transfer is reconciled against the original backup and fresh readback before another write. Reopening the app or switching pages must not replay it. A copied/flushed file, USB product name or version string is not exact installation proof.

Check pairing queries healthy links without clearing bonds. Repair is explicit and only clears dedicated dongle bonds, preserving ordinary Bluetooth profiles. Pairing completion requires reciprocal peer identity and encrypted-link observations, plus the right split link. Cancellation cannot undo a command already accepted by firmware. Keep peer identity data private.

Whole-keyboard installation ends with wired, Bluetooth and dongle typing checks. The test `qwert HJKL h` exercises left input, right input, cross-half Shift and modifier release. A connection indicator cannot pass this test in place of typed input.

## Build and test

Install Rust through rustup and use the checked-in toolchains. Firmware also needs Arm GNU bare-metal GCC with newlib headers. Linux CI uses `gcc-arm-none-eabi` and `libnewlib-arm-none-eabi`. A host compiler alone is insufficient for the P-256 dependency.

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

A clean clone has no firmware binaries or private acceptance evidence. Use the public release’s reviewed firmware input and the bounded package extractor for app builds. `scripts/package_companion_release.py` is the maintainer’s evidence-bound packaging tool; it must not create hardware acceptance from a fresh build.

Firmware roles are `left`, `right`, `receiver`; select exactly one. Production halves use the explicit `reclaimed-softdevice` layout with runtime recovery. The receiver preserves resident S140. Role builds share an output name, so use separate target directories or save each ELF before another build. Read the checked-in feature definitions before selecting a diagnostic feature.

Run `scripts/image_guard.py` for receiver images and the UF2/BIN-bound checks in `scripts/migration_guard.py` for lower-layout halves. Address, vector and family checks are structural proof only. Bind the actual role, build features, exact image and target separately. No generic `cargo run` flash runner is provided.

Before transport or scanner changes, run the repeatable host harness and cross-build all supported roles. Hardware acceptance must separately cover disconnect/release recovery, simultaneous input, first-key wake, recovery entry and all typing routes. Do not describe compilation or mock success as hardware validation.

## USB protocol reference

The development runtime identities are `4c4b:4643` (left), `4c4b:4671` (right) and `4c4b:4644` (dongle). These are not registered production allocations or unique device bindings. The observed factory UF2 identity is `239a:0029`. Discover the DFU interface number from its `fe/01/01` descriptors; do not hard-code it. DFU DETACH targets the local part, while Vial requests through the dongle target the left.

Read-only custom Vial getters use usage page `ff60`, usage `61`, and 32-byte reports. Headers identify battery/status (`NCBT`), selected mode (`NCMO`) and raw ADC (`NCAD`). Dedicated pairing uses `NCPR`. Follow the versioned serializers and validators in the source; unsupported or inconsistent replies must remain unknown, not become zero battery or a successful connection.

Status separates USB power, selected connection policy, active typing route and recovery. Battery values come from producer caches, not a new voltage measurement on every request. Connectivity observations expire independently from battery estimates. Wireless-only mode cannot be assumed from a USB getter that has no responding endpoint.

## Diagnostics and bug reports

**Help → Show logs** opens local logs. **Help → Export diagnostic logs** saves a ZIP with fixed-state events and an app-window screenshot when capture is available. No automatic upload occurs. Review the screenshot before attaching the ZIP to an issue.

Logs rotate across four files capped at 256 KiB each. They record version, journey state, part and test mode, operation outcomes, USB changes and a 30-second heartbeat. Typed text, firmware bytes, raw error/panic payloads and device identifiers are excluded. An unclean-exit marker is reported on the next successful launch. A heartbeat gap helps diagnose a hang; it is not automatic recovery.

If the app hangs, force quit it, reopen it and export logs. If it cannot reopen on macOS, logs are under `~/Library/Logs/NocFree RMK Companion/`. Private backups are under `~/Library/Application Support/NocFree Companion/`. Do not attach factory firmware, bonds, serial numbers or private backups to public issues.

Include the app version, OS, affected part, switch positions, USB connections, exact steps and whether recovery still works. An issue needs the exported logs or an explanation of why export failed.

## Release and license maintenance

Project code is MIT. Dependencies, fonts, icons and bundled SDK code keep their own licenses. Preserve `LICENSE`, `NOTICE.md`, `desktop/THIRD_PARTY_NOTICES.md`, `docs/notices/inventory.json` and the hashed source texts under `docs/notices/texts/`.

```sh
python3 scripts/companion_notices.py --inventory --check
python3 scripts/companion_notices.py /tmp/NocFree-Third-Party-Notices.md
python3 scripts/companion_notices.py /tmp/NocFree-Third-Party-Notices.md --strict --check
```

The full generated appendix belongs in distributed app archives. Strict validation rejects missing or changed recorded text and stale source graph hashes. Version-only manifest updates still require a locked graph comparison before refreshing the inventory hash. Dependency or firmware-pin changes require recapture and review against exact immutable source versions.

For upstream archives that omit license files, the inventory retains checksum-verified original declarations and available copyright headers. Supplementary canonical SPDX terms are labelled separately. Do not invent author attribution or claim a template is an upstream notice.

Release jobs build without signing secrets. Protected signing jobs use only the reviewed build artifact, an ephemeral keychain and the main-restricted release environment. Never print credential values, enable shell tracing around secrets, publish private build evidence, or replace an existing release archive/tag. Verify release checksums, the bundled firmware manifest and license appendix. For macOS, also verify the Developer ID signature, notarization staple and Gatekeeper assessment.

## Windows and Linux preview

Native builds include the same pinned keyboard images and shared journey machines. Hardware acceptance currently covers macOS only. Windows and Linux need physical USB, recovery, pairing and typing tests before equal support can be claimed.

Linux requires a desktop with Vulkan, X11 or Wayland, the libraries listed in the archive's README.txt, BlueZ for Bluetooth status, and zip for log export. Install the included narrowly scoped udev rule to give the active local desktop session access to the keyboard, reconnect the devices, and mount the recovery drive in the file manager. Do not run Companion as root.

Windows uses its Bluetooth connection-status API. Automatic USB recovery may need WinUSB bound to the DFU interface; never replace the HID or mass-storage driver. If automatic entry is unavailable, use the displayed physical recovery procedure. The Windows binary is not Authenticode-signed. App-window screenshots are currently macOS only; Windows/Linux exports still contain state logs and support context.

Private data uses `LOCALAPPDATA/NocFree Companion` on Windows and `XDG_DATA_HOME/NocFree Companion` (or `~/.local/share/NocFree Companion`) on Linux. Logs use `LOCALAPPDATA/NocFree RMK Companion/Logs` on Windows and `XDG_STATE_HOME/nocfree-rmk-companion` (or `~/.local/state/nocfree-rmk-companion`) on Linux. Windows relies on the user profile's inherited ACLs. Files use write-through handles and explicit flushing; Windows does not provide the Unix directory-fsync guarantee. Sudden power loss can lose the newest directory entry; the app must reconcile incomplete transfers before allowing another write.

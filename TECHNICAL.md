# Technical guide

The published Companion and its bundled firmware use the factory bootloader. The upstream RMK spike below is a separate draft. Its images are not release inputs. Sections marked production describe the published firmware; the source map and development checks describe this branch.

## Keyboard setup

The keyboard images use the ANSI NocFree AND layout. The right half sends keys to the left half. The left half sends them to the computer through USB, Bluetooth or the dongle. Install RMK on the dongle to use it with the RMK keyboard firmware.

## Physical layouts

Install RMK remembers the layout chosen on its setup screen. Next starts installation only when the app includes approved firmware for that layout. The current package contains ANSI firmware.

The scanner, default keys, split dimensions and Vial definition share one compile-time layout choice. ANSI is the default. Select `layout-iso`, `layout-jis` or `layout-kr` for both halves; these features are mutually exclusive. The dongle forwards reports and configuration to the left half and uses one common image.

| Layout | Left keys | Right keys |
| --- | --- | --- |
| ANSI | 37 | 47 |
| ISO | 38 | 47 |
| JIS | 37 | 48 |
| KR | 39 | 50 |

The mappings in `crates/nocfree-input/src/layout/` use the MIT-licensed [layout definitions from jhkim0218](https://github.com/jhkim0218/Nocfree-and-ZMK-rust/tree/5b0fefcff9af3cc4876bb420f86f6b471eed55ba/src/keymap). KR reads four additional right inputs at `0x21/P0`. Each layout has its own Vial identifier; ANSI keeps its existing identifier and key assignments.

The `.vil` presets in `firmware/presets/` use the ANSI Vial identity and 84-key matrix. Use each other layout's compiled defaults or remap its keys in Vial. Its current Vial drawing is schematic; check key widths and stagger against the physical keyboard before publishing that layout.

Check all mappings and production builds without connecting a keyboard:

```sh
./scripts/check.sh --all-layouts --backlight-active-high
```

The production fork records physical layout identity in its storage schema. This draft uses the upstream schema and needs an explicit settings reset when changing physical layout. Check pairing and saved key assignments after a reset or revision change.

An owner of each layout must then check every physical key in Vial, Fn and Shift across halves, simultaneous input, release after disconnect, wake on the first key, wired/Bluetooth/dongle typing, saved remaps after restart, and recovery entry and exit. Record those observations separately from host tests and cross-builds before adding the images to an app package.

## Architecture and source map

```text
Right inputs → board scanner → RMK split BLE → Left keymap
                                                ├→ USB keyboard
                                                ├→ Bluetooth keyboard
                                                └→ RMK dongle → USB keyboard

Production Companion → local USB recovery request → selected part’s factory bootloader
Companion/Vial → left USB, or dongle relay → left configuration
```

RMK owns key actions, debounce, transports, Bluetooth profiles and storage. Board code adapts PCA9555 expanders, battery ADC, PWM, indicator pins, USB control-pipe behavior and startup checks. The upstream draft leaves the switch inputs unused.

| Concern | Source |
| --- | --- |
| Expander mapping and complete input snapshots | `crates/nocfree-input/` |
| Scanner and interrupt idle behavior | `firmware/src/scanner.rs` |
| Keymap | `firmware/src/keymap.rs` |
| Firmware composition | `firmware/src/main.rs` |
| Battery sampling | `firmware/src/battery.rs` |
| Board PWM and brightness | `firmware/src/board_pwm.rs`, `firmware/src/board_backlight.rs` |
| Indicator | `firmware/src/board_status_led.rs` |
| UICR read gate | `firmware/src/startup.rs` |
| nRF52833 USB adapter | `firmware/src/usb_adapter.rs` |
| Keyboard dimensions and sleep settings | `firmware/keyboard.toml` |
| Shared recovery journey | `desktop/src/recovery_journey.rs` |
| Per-part transfer and backup journeys | `desktop/src/peripheral_journey.rs`, `desktop/src/firmware_journey.rs` |
| Whole-keyboard installation | `desktop/src/install_journey.rs` |
| Factory input validation | `desktop/src/factory_source.rs` |
| Bundled firmware validation | `desktop/src/release.rs` |
| Battery/status transport | `desktop/src/battery.rs`, `desktop/src/battery_vial.rs` |

Each Cargo manifest and lockfile fixes the dependency versions. The firmware and desktop use separate Rust toolchains.

Dependabot checks all four Cargo projects and GitHub Actions monthly. Security updates are enabled. Renovate updates both Rust toolchains together each month. All updates need review. Update the license inventory when desktop dependencies change. Review pinned upstream revisions with their firmware and desktop protocol changes.

## Upstream RMK with factory recovery

This draft uses untouched [RMK `434ab4d7`](https://github.com/rmk-rs/rmk/tree/434ab4d7d29d8e9ba689837358c8a44996ba38cc) and published Embassy nRF 0.11.0. It has no fork dependencies or dependency patches. It keeps the factory MBR and bootloader. It contains no bootloader installer or `rmk-boot` image. Companion still packages the approved production firmware.

RMK owns key processing, debounce, USB/BLE routing, profiles, bonds, storage and normal watchdog feeding. Board code supplies the PCA9555 scanner, battery ADC, PWM, indicator and nRF52833 USB control-pipe adapter. Image startup checks reject UICR changes and convert watchdog resets into a request for the existing recovery bootloader.

The physical selector inputs are unused. RMK chooses the ready transport; when USB and BLE are ready together, its saved preference decides. This does not establish the switch's power behavior. Native dongle pairing replaces private peer repair. Battery sampling feeds RMK, but the private Companion battery, mode and pairing reports are absent. The existing switch-driven Companion tests still need adaptation before these candidates can replace the bundled firmware.

| Control | Action |
| --- | --- |
| LEFT Fn+Tab | Select the dongle profile; hold five seconds to clear its bond |
| LEFT Fn+Space | Toggle saved USB/BLE preference |
| Fn+1 through Fn+5 | Select a Bluetooth host profile; hold five seconds to replace its bond |

LEFT Fn+Esc and RIGHT Fn+0 held at CPU startup request local recovery at the scanner seam. No runtime key is bound to recovery. There is no single-tap bond-clear key. RIGHT's runtime keys are processed on LEFT, so a normal bootloader key cannot target RIGHT. Dongle host commands also relay to LEFT; recovery must use the dongle's local USB interface.

Backlight uses RMK User actions, events, user-data storage and split messages. Board PWM supplies 400 Hz output with 16 levels. Existing Vial lighting assignments need remapping to the new actions. Physical layouts compile separately. Changing physical layout requires a settings reset because this draft uses the upstream storage schema.

### Local USB recovery

LEFT and dongle use RMK's standard runtime DFU DETACH interface. It accepts entry only within 30 seconds of CPU startup. Replugging the bus-powered dongle restarts that window; reconnecting LEFT USB while its battery keeps it running does not. LEFT also exposes upstream Vial's bootloader request; that command is not gated by Vial unlock. Upstream Rynk's equivalent request requires unlock. That request must go to LEFT, not be presented as recovery of the dongle relay.

RIGHT uses Embassy's standard runtime DFU class and a callback to RMK's bootloader request. Its local USB identity is distinct from LEFT and dongle. This class has no download partition: the app requests recovery; the factory bootloader handles subsequent transfers. Windows receives the standard WinUSB descriptor.

Companion accepts both its production runtime descriptor and upstream RMK's descriptor, while retaining local role, interface and USB-port checks. Its recovery worker sends only DETACH. It does not send a firmware download to the running application. LEFT's native host request and the startup-window instructions still need integration into the installer. Until then, the approved production package remains the installation input.

### Image boundaries

| Role | Application interval | Settings interval |
| --- | --- | --- |
| LEFT and RIGHT | `0x1000..0x65000` | `0x65000..0x6d000` |
| Dongle | `0x27000..0x65000` | `0x65000..0x6d000` |

The guarded ANSI Mac-keymap builds use 372,052 bytes for LEFT, 231,788 for RIGHT and 248,172 for the dongle. The dongle's slot has 253,952 bytes; its UF2 page padding also fits. The complete layout/keymap build matrix is checked separately.

Compared with main, project source adds 1,158 net lines. Retiring 2,962 lines from the pinned RMK and Embassy forks reduces maintained source by 1,804 lines overall. This count includes board code and tests, and excludes lockfiles, documentation, generated binaries and dependency code.

The lower half images replace the resident SoftDevice while preserving the MBR. The dongle keeps the resident SoftDevice. All images exclude the factory filesystem at `0x6d000..0x74000`, the bootloader above it and UICR. These are the existing guarded project boundaries; they are not inferred from a generic Adafruit example.

The startup reserve at application offset `0x200` stays erased. It does not request recovery on every normal startup. Address, vector, board-family and BIN/UF2 equivalence checks remain mandatory. Passing them proves image structure, not recovery on a device. Factory backups contain the recovery drive's exported user-flash data, not a full chip or UICR dump. Keeping the original bootloader avoids needing a bootloader restoration image.

```sh
./scripts/check.sh --host-only --all-layouts
./scripts/check.sh --build-only --all-layouts --backlight-active-high
```

### Device acceptance

Check local recovery on each part, watchdog entry and long transfers, interrupted application updates, startup failure, and return to the original firmware without opening the case. A healthy watchdog feeder cannot detect every stalled task; the startup hook also cannot recover if its own executable code is corrupt. Neither a USB request acknowledgement nor a successful cross-build establishes these recovery routes.

Then check every key and remap, pairing and reconnection, disconnect releases, simultaneous input, wake latency and wired/Bluetooth/dongle typing. Owners must test ISO, JIS and KR. Keep hardware observations separate from host checks.

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

In production, the physical left switch selects top dongle, middle wired and bottom Bluetooth. The upstream draft ignores its selector inputs and uses the RMK controls listed above.

Tap Fn+1 through Fn+5 to select a Bluetooth host profile. Hold the combination for five seconds without another key to replace that profile’s pairing. In RMK, Fn+0 clears the selected host pairing. Use Companion to enter recovery mode.

## Production power and lighting

A USB-powered half stays awake regardless of the selected connection mode. On battery, the right follows the left’s sleep request. The configured idle period is 1,800 seconds. The board stays powered during sleep.

`async-scanner` waits for the expander interrupt when idle. Active keys, unfinished debounce and read failures continue scanning. The board scanner reads keys through I²C.

Backlights use 400 Hz PWM with 16 brightness levels, saved brightness and split synchronization. Zero brightness stops and disables the PWM generator; wake or nonzero brightness restarts it. The DMA buffer remains valid through cancellation. A battery-powered disconnected right darkens while seeking the left, and disconnect cancels brightness holds.

The left blue indicator blinks while seeking either wireless route. After connection it stays on for 30 seconds, then turns off. Wired mode uses the shared red indicator’s existing behavior. Charging can mix red with blue. The right has no host-mode indicator because it only links to the left.

Battery conversion uses `BatteryProcessor::new(100, 150)`. Calibration work is tracked in [issue #7](https://github.com/sarimabbas/nocfree-and-rmk/issues/7). To adjust the conversion, compare raw ADC counts with cell voltage and discharge measurements.

## Production recovery and backup

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

## Production app state machines

Companion uses Statig state machines. The UI derives the screen and available buttons from the machine state. Parent journeys reuse recovery, transfer, backup and pairing machines. Generation tickets reject stale or duplicate results after cancellation or a change of part.

Opening a page shows the first step. Click Next to move through each step. Next becomes available when the step is complete, and the spinner stops. Completed progress stays visible. A successful typing test enables Next as soon as the connection conditions are met.

RMK installation orders dongle → right → left. Factory restoration orders left → right → dongle. Selected parts remain fixed during a journey, even when a step asks for disconnection. The factory left and dongle can share USB descriptors. Companion asks you to unplug and reconnect a part to identify it.

Each transfer saves a fresh backup, writes the image once, reads it back to check the bytes, and checks normal startup. If a transfer stops, the app checks the original backup and a new readback before another write. This record persists when you reopen the app or switch pages.

Check pairing reads the connection state. Repair clears and replaces the dongle pairing when requested. It keeps the Bluetooth host profiles. A successful check confirms the left-to-dongle and left-to-right connections, their device identities and link encryption.

Whole-keyboard installation ends with wired, Bluetooth and dongle typing checks. The test `qwert HJKL h` exercises left input, right input, cross-half Shift and modifier release.

Companion checks the latest public GitHub release once at startup. A newer stable app version shows an Update available link in the sidebar. The request runs in the background with a ten-second timeout; failed checks leave the app usable.

## Build and test

Install Rust through rustup and use the checked-in toolchains. Firmware also needs Arm GNU bare-metal GCC with newlib headers. Linux CI uses `gcc-arm-none-eabi` and `libnewlib-arm-none-eabi`. The P-256 dependency uses the Arm compiler.

```sh
./scripts/check.sh
```

This runs host checks and firmware cross-builds without accessing a device.

CI runs host tests once and selects firmware or desktop jobs from the changed files. Rust dependencies are cached, and newer commits cancel older runs for the same branch or pull request. Published releases and manual CI runs check everything. The required `CI` check fails if a selected job fails or does not run. Use `./scripts/check.sh --host-only` for local host checks; `--build-only` skips host checks when building a specific firmware configuration.

For the desktop:

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

Each app download includes the firmware for both halves and the dongle. The same app release also contains `companion-firmware.zip` for builds. Use `scripts/unpack_companion_release.py` to extract this package for local app builds. `scripts/package_companion_release.py` creates firmware packages from images and device test records.

Firmware roles are `left`, `right`, `receiver`; select exactly one role and the factory-compatible layout on this branch. Production halves use the lower application layout; the receiver preserves resident S140. Role builds share an output name, so use separate target directories or save each ELF before another build.

The production image tools are `scripts/image_guard.py` for receiver images and `scripts/migration_guard.py` for lower-layout halves. They check addresses, vectors and board family. They also guard the draft’s factory-compatible images. Candidate generation does not add them to the approved release package.

Before transport or scanner changes, run the host harness and cross-build all supported roles. Test disconnect and key release, simultaneous input, wake on the first key, recovery entry and all connection modes on the keyboard. Record build results separately from device tests. Use primary sources for pin, memory and radio claims, and distinguish assumptions from device observations.

For automated work, do not flash, erase, unlock, change bootloaders, write UICR or run `probe-rs recover`. Images must pass the address, vector and family checks and have device-specific bootloader evidence and a tested recovery route. Do not add a generic `cargo run` flash runner.

## Production USB protocol reference

The development runtime identities are `4c4b:4643` (left), `4c4b:4671` (right) and `4c4b:4644` (dongle). The observed factory UF2 identity is `239a:0029`. Discover the DFU interface number from its `fe/01/01` descriptors; do not hard-code it. DFU DETACH targets the local part, while Vial requests through the dongle target the left.

Read-only custom Vial getters use usage page `ff60`, usage `61`, and 32-byte reports. Headers identify battery/status (`NCBT`), selected mode (`NCMO`) and raw ADC (`NCAD`). Dedicated pairing uses `NCPR`. Use the versioned serializers and validators in the source. Mark an unsupported or inconsistent reply as unknown.

Status separates USB power, selected connection policy, active typing route and recovery. Battery values use cached measurements. Connection observations and battery estimates have separate expiry times. Mark a missing USB reply as unknown.

## Diagnostics and bug reports

**Help → Show logs** opens local logs. **Help → Export diagnostic logs** saves a ZIP with fixed-state events and an app-window screenshot when capture is available.

Logs rotate across four files capped at 256 KiB each. They record version, journey state, part and test mode, operation outcomes, USB changes and a 30-second heartbeat. Typed text, firmware bytes, raw error/panic payloads and device identifiers are excluded. An unclean-exit marker is reported on the next successful launch. A gap between heartbeats helps locate a hang.

If the app hangs, force quit it, reopen it and export logs. If it cannot reopen on macOS, logs are under `~/Library/Logs/NocFree RMK Companion/`. Backups are under `~/Library/Application Support/NocFree Companion/`.

Include the app version, OS, affected part, switch positions, USB connections, exact steps and whether recovery still works. An issue needs the exported logs or an explanation of why export failed.

## Contributions

Keep changes focused and include the reason and test results in the pull request. Discuss larger changes in an issue first. Use independent implementation and review passes for substantial changes, with clear file ownership. Commit reviewed changes with concise Conventional Commit messages. Keep factory firmware, recovery executables, credentials, local device identifiers and generated binaries out of Git.

Report security flaws through [GitHub private vulnerability reporting](https://github.com/sarimabbas/nocfree-and-rmk/security/advisories/new), with the affected version and steps to reproduce the problem.

## Releases and licenses

Project code is MIT; RMK uses MIT OR Apache-2.0. Pin information, expander order and the ANSI layout mapping come from the MIT-licensed [NocFreeKB/NocFree-and-zmk](https://github.com/NocFreeKB/NocFree-and-zmk) project. Dependencies, fonts, icons and bundled SDK code keep their own licenses. The app packages include `LICENSE` and third-party licenses generated from `docs/notices/inventory.json` and the source texts in `docs/notices/texts/`.

```sh
python3 scripts/companion_notices.py /tmp/NocFree-Third-Party-Notices.md
python3 scripts/companion_notices.py /tmp/NocFree-Third-Party-Notices.md --strict --check
```

The notice generator checks the recorded source hashes and license texts. Update the inventory when dependencies or bundled firmware change.

Release jobs build the app first. A protected job signs the build with a temporary keychain in the main-branch release environment. Verify the archive checksums, bundled firmware manifest and license notices. On macOS, also check the Developer ID signature, notarization ticket and Gatekeeper result.

When starting the release workflow, select an app release that holds the firmware package and provide its SHA256. For new firmware, put the checked package in a draft of the new app release and select that draft as the source. The workflow checks the package, bundles it in all three apps, and adds the app downloads to the same draft. When reusing firmware, it copies the package from an earlier app release. Firmware and app downloads share one release.

## Windows and Linux

All three app builds use the same keyboard images and state machines.

Linux requires a desktop with Vulkan, X11 or Wayland, the libraries listed in the archive's README.txt, BlueZ for Bluetooth status, and zip for log export. Install the included udev rule to give your desktop session access to the keyboard, then reconnect the devices. Open the recovery drive in the file manager. Run Companion as your desktop user.

Windows uses its Bluetooth connection-status API. Automatic USB recovery can require WinUSB on the DFU interface. Keep the existing HID and mass-storage drivers. If automatic entry is unavailable, use the displayed physical recovery procedure.

Backups and app data use `LOCALAPPDATA/NocFree Companion` on Windows and `XDG_DATA_HOME/NocFree Companion` (or `~/.local/share/NocFree Companion`) on Linux. Logs use `LOCALAPPDATA/NocFree RMK Companion/Logs` on Windows and `XDG_STATE_HOME/nocfree-rmk-companion` (or `~/.local/state/nocfree-rmk-companion`) on Linux. Windows uses the user profile’s file permissions. File writes use write-through handles and explicit flushing. Before another write, the app checks any incomplete transfer against the backup and a new readback.

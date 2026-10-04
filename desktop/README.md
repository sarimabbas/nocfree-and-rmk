# NocFree RMK Companion

A native macOS application using [GPUI Kit](https://gpui-kit.com/). Install RMK, Restore factory, Backup firmware, Enter recovery mode and Check pairing share explicit Statig state machines. Opening a page is passive: selecting connected parts and pressing Next leads to setup; the setup’s Next starts device work.

Choose scope with the Left, Right and Dongle cards. Connected USB parts are selected by default; disconnected parts are disabled. Factory left and dongle can share a USB descriptor. When identity is ambiguous, a short unplug/reconnect guide identifies the selected part rather than guessing from its layout or USB port.

## Firmware journeys

Install RMK uses the fixed, hash-pinned local package. Each selected part goes through shared recovery, a durable backup, one transfer, exact readback and normal startup. An exact match skips the transfer. Whole-keyboard installation additionally runs pairing and guided wired, Bluetooth and dongle typing checks; partial installation does not establish whole-keyboard acceptance.

Restore factory uses private original backups or user-supplied factory UF2s. Official firmware is never bundled or downloaded. An app-only factory UF2 needs a complete original donor containing S140. Backups captured from an identified factory part retain role and integrity evidence independently of layout/version. An arbitrary unknown image does not become an original merely because it was selected in a file picker.

Known factory layouts are detected independently of the ANSI layout used by the current RMK keymap. Detection is not physical acceptance of every hardware revision. USB identity does not prove installed application bytes. Development images sharing the same reported version require recovery readback to determine whether the pinned image is already installed.

Uncertain transfers are journaled and reconciled after restart before another write. Sidebar navigation does not replay transfers or pairing commands. Pairing success reports a completed check; Done returns to setup and an explicit Next retries a failed check.

## Status and recovery

The status bar separates selected connection mode from USB power. Battery readings are cached across mode changes while connected and hidden after disconnection. Percentages are firmware-reported integers; the bundled voltage scaling remains provisional, not a measured capacity calibration.

Compatible RMK parts expose standard USB DFU runtime recovery. Companion addresses the identified part directly; factory firmware uses its guided factory procedure. Both paths compose the same recovery worker used by backup and firmware journeys. Factory bootloaders remain unchanged. Per-part recovery icons are independent of the keyboard firmware label.

Copies are private under `~/Library/Application Support/NocFree Companion/`. Recovery archives exclude MBR, bootloader and UICR and are not full-chip backups. Permission failures are actionable flow states; grant removable-volume access in macOS settings before retrying.

## Build and verify

The desktop pins Rust separately from firmware. Prepare the reviewed firmware package in `dist/companion-firmware/` first. Maintainers can recreate the current package with `python3 scripts/package_companion_release.py` when its private review/readback evidence is available. A clean clone does not include firmware binaries or that evidence. The app builder rejects missing, changed or incomplete packages before replacing the bundle.

```sh
./desktop/build-macos.sh
open 'dist/NocFree RMK Companion.app'
```

```sh
cd desktop
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
```

The built app is locally ad-hoc signed, not a public notarized release. Developer ID distribution, portable release-package delivery and Intel/Windows/Linux acceptance remain release work.

## Owner acceptance before release

Host tests and a macOS build do not establish a physical roundtrip. Test factory → RMK → factory → RMK with the complete current journeys, partial scopes, relaunch during interrupted work, removable-volume denial/regrant, pairing, first-key wake and all three typing modes. Runtime recovery of each shipped image, including dongle crash recovery, is a separate hardware gate. Never substitute a descriptor match, saved backup or cross-build for those observations.

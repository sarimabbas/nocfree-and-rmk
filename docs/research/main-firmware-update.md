# Main firmware candidate, 2026-10-04

Source version `0.1.1` pins RMK main `89fead1de856132911ec685313fa88cda0652bac` and Rust `1.98.1`. The Vial preset branch and RMK backlight conversion branch have been integrated into their respective main branches. The two RMK merge conflicts retain upstream main's checked macro bounds and Bluetooth bond-state mutation, with NocFree's invalid-slot rejection and remembered host profile added. Independent review found no correctness issues.

## Reproduce without devices

With `arm-none-eabi-gcc` (including newlib) on `PATH` and Rust LLVM tools installed:

```sh
python3 scripts/build_firmware_candidates.py --dry-run
python3 scripts/build_firmware_candidates.py
```

Ignored output is `.evidence/main-firmware-update/candidates`: one exact BIN/UF2 pair per role, build logs, section sizes, guard results, and a manifest binding source hashes, dependency revision, compiler and feature sets. Candidates are structurally checked, not approved releases. The script never invokes a device runner or flashes hardware.

| Role | BIN bytes | Guarded flash interval | UF2 SHA-256 |
| --- | ---: | --- | --- |
| Left | 374220 | `0x1000..0x5d000` | `c72333620a85e76323225d0d97eb96f69c3448f7102ccfdade8918d5c148bdfb` |
| Right | 229028 | `0x1000..0x39000` | `ce2de33edfcf8f3c896aba71a0a70c77df268628bbbf1599018d3955233a2bc1` |
| Dongle | 249060 | `0x27000..0x64000` | `3e26410a96d69017bc2ff7c23a45e45a06ad4f93fea6e3c5cd7719dbec7a7c49` |

The dongle retains S140 and has one 4096-byte page left before the protected application boundary. Both halves retain their existing reclaimed-space policy. Address, reset vector, family, page padding and exact BIN/UF2 agreement passed for all three roles. This proves image structure and build fit, not startup, RAM headroom, battery consumption or device operation.

## Validation

- Merged RMK: 632 full-feature host tests and 12 configuration tests passed; strict Clippy and formatting passed.
- Board scanner: 21 unit and 2 integration tests passed.
- Public structural safety: 78 tests passed.
- Companion: 249 unit and 2 integration tests passed, including all six private fixture checks; strict Clippy passed.
- Four Vial preset checks and the official offline importer passed; all 168 values per preset were preserved without HID access.
- All three production role cross-builds passed. Root independently rechecked generated source hashes, UF2/BIN hashes and role guards.

## Hardware gate and promotion

Companion still bundles the exact previously verified `0.1.0-local.2` images. The release packager refuses unverified candidates; new image hashes must not be substituted for installed readback evidence.

Before promoting these candidates, use each proven recovery route, preserve a fresh full backup and settings observation, obtain role-specific review, install through the guarded procedure, and compare exact installed application/padding and untouched protected regions. No device was accessed or written during this update.

Then verify normal startup and independent recovery on each role, split typing in wired/Bluetooth/dongle, simultaneous input and cross-half modifier release, disconnect recovery, first-key idle wake, Bluetooth bond/reconnect, and Vial brightness readback/remapping over left USB and dongle. Import/export each preset, restart to check persistence, and verify a brightness hold remapped to RIGHT stops on release or disconnect. Restore the owner's preferred preset afterward.

Only after those observations should the package allowlist, manifest hash and bundled firmware notices be updated together. Signing/notarization credentials, the twelve recorded source-notice gaps, and signed launch on another Mac remain separate public-release gates.

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

## Hardware observations and promotion

The fixed release is now `0.1.1`, release ID `nocfree-a717808eec6f8dfe`, manifest SHA-256 `e9da15aebe486cb879eb62c65628d85d12b33ca587cbb32564c17a0b824d70af`. Source provenance is commit `fb45eecef29f2a2883f44cd6e28843798238d0a7`; the feature sets and image hashes above are unchanged.

On the owner's keyboard, each role had a fresh full recovery backup, independent hash-bound operator/baseline review, a guarded application-only install, and exact installed application/padding readback. Untouched application gaps and the dongle's S140 prefix matched the original backup. Storage bytes changed after startup; the release records that fact and does not claim settings preservation. Bootloader code is outside the readable backup and was not rewritten.

The root operator reopened each newly installed image's recovery route through Companion. Private recovery observation records identify these as root-reported GUI observations and bind the candidate UF2 and archived installed readback hashes; they are not synthetic raw USB captures. The package independently recomputes exact byte comparisons and refuses unbound observations or baseline reviews.

Owner observations confirmed left and right typing, cross-half Shift release, dongle typing, and Bluetooth pairing/reconnection. The owner reported the other requested brightness, remapping/persistence and idle checks looked correct. These are owner observations, not measured current consumption, calibrated battery readings or wake latency measurements.

A held right Space continued repeating for more than ten seconds after RIGHT was switched OFF with USB unplugged. A host reproduction also found missing release delivery on the shared RMK split-BLE disconnect path. The owner explicitly deferred a local fix in favor of RMK; this is a known release limitation. Do not describe disconnect release as passing or imply all RMK boards were tested.

Signing/notarization, complete bundled source notices, and launch on another Mac are independent distribution gates.

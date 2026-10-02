# Left migration trial candidate

Prepared 2026-10-01. **Not installed; hardware approval and validation pending.** This is a USB recovery diagnostic, not typing firmware or the RMK split central. It reuses the tested recovery probe with a separate explicit `migration-probe` feature. The existing MBR and bootloader remain outside its address range.

## Reproduce and verify

From `firmware/`, build:

```sh
cargo build --locked --release --bin recovery-probe --target thumbv7em-none-eabihf --no-default-features --features left,migration-probe
```

Run `./scripts/check.sh --reclaimed-softdevice --usb-recovery-first` from the repository root. The harness includes this migration probe alongside all production roles and existing factory-layout diagnostics. Ordinary full left production firmware still exceeds the factory application slot; a default-layout failure is not resolved by this milestone.

Package the exact file-backed ELF load segments into a BIN with erased-byte gaps; verify independently against Arm objcopy using `-O binary` (preserve the ELF’s internal zero padding). Use ordinary nRF52833 application-family UF2 blocks, padded with `0xff` through only the final touched 4-KiB page. Call `scripts/migration_guard.py`'s `inspect_migration(uf2_bytes, bin_bytes)` before transfer. The existing `image_guard.py` intentionally rejects this lower layout and remains unchanged. This candidate has no serial DFU ZIP: the first migration needs explicit UF2 target addresses.

The independent restore policy is `inspect_left_factory_restore(uf2_bytes)`: it accepts only this owner's saved original left container, structurally checked and bound to its exact hash. This device-specific trial tool is not a universal installer or role detector. The private package builder and generated artifacts remain ignored under `.evidence/` and `dist/`.

## Candidate identity and bounds

| Property | Value |
|---|---|
| Role/features | `left,migration-probe` |
| USB identity | `4c4b:4653`, `NocFree Recovery Probe Left Migration` |
| CDC greeting | `NocFree recovery probe 0.1.0 left migration` |
| BIN size / end | 14,488 bytes / `0x4898` exclusive |
| UF2 addressed range | `0x1000..0x5000` exclusive; 64 blocks |
| Touched flash pages | `0x1000`, `0x2000`, `0x3000`, `0x4000` |
| Initial SP / reset PC | `0x20020000` / `0x1205` |
| Recovery marker | `0x1200 = 0x87eeb07c` |
| Old S140 detection word | `0x3004 = 0x48880310`; old magic absent |
| BIN SHA-256 | `bca78013b3a45473ee13c77cfd0e00b2d4f6f917479a66bf6edda56ea1c2d10c` |
| UF2 SHA-256 | `c873d1cf09f59a42555d7fe8ed838505025183c4e319aa1ddf01cf6947cf3233` |

No scanner, radio, storage, battery or lighting task runs. MPSL is linked for the critical-section implementation but is not initialized. The greeting and existing 1200-baud update entry are the only application diagnostics. Compile-time guards reject incorrect or simultaneous roles and keep the input probe factory-only.

The restore container covers `0x1000..0x6d000`, including S140 and factory application/settings; SHA-256 `88cb768f452682cb12025f2296ee151442349079405a7a11f454f752153e4100`. It excludes MBR, filesystem, bootloader and UICR. It is not the previously tested app-only restore ZIP.

## Software review

Independent implementation and safety-review agents checked the source, guards, ELF/BIN/UF2 and original private restore artifact. The BIN also matches Arm `objcopy -O binary` exactly. The reset vector points into the ELF executable text section. Ten new guard tests cover protected addresses, malformed/reordered blocks, exact BIN binding, final-page tails, vector boundaries, old S140 magic and exact restore binding. Together with existing tests, 42 Python and seven scanner tests pass. None of these results is hardware validation.

## Device trial gates

Follow [the researched migration sequence](next-migration-trial.md). Before transfer, independently establish the connected left unit and verify its installed diagnostic and bootloader readback. After transfer, require exact candidate bytes, final-page padding and retained readable-byte equality, bootloader identity with absent SoftDevice, USB greeting, physical USB-first recovery, battery-first startup, and full factory restoration/readback equality. Stop on any failed gate before considering the full RMK central.

The lower-layout bootloader path and restoration of overwritten S140 have not been hardware-tested. An interrupted write or vendor bootloader difference could require opening the enclosure or a debug probe. No automatic rollback or interrupted-update guarantee is provided. The right half and receiver are outside this trial.

# Flashing and recovery

**Current gate: closed.** This is an unvalidated development port. No device has been flashed or established an independent recovery route after application failure. The left half has been observed entering factory CDC-only DFU and returning after reconnecting; see [device observations](device-observations.md). The original factory applications and the new Rust radio stack use different internals; matching UF2 addresses is necessary but insufficient to establish a compatible bootloader handoff.

## Prepare without writing

Keep `output_20260911_v2.4.5.zip` outside the repo and check its SHA-256:

```sh
shasum -a 256 /Users/sarimabbas/Downloads/output_20260911_v2.4.5.zip
```

Expected: `c11381390b421f6ee3c41bcc090513d392368d09c85b93db31a1dc242427c38a`.

Use one NocFree device at a time and a data-capable cable. For the left half select USB mode. The inspected vendor recovery application enters DFU by opening and closing its serial port at 1200 baud; that behavior is evidence of a supported factory route, not authorization to touch any arbitrary serial device. First identify the serial port/USB identity for the exact role. Save local observations in `.evidence/`.

The connected factory left half exposes CDC-only DFU after 1200-baud entry, with no UF2 drive. Do not assume copying a UF2 is available. If an independently verified bootloader entry does expose a drive, read `INFO_UF2.TXT`; copying or dragging firmware at this stage writes to the board, so stop after reading. Record bootloader version, board ID, SoftDevice version, family and current application handoff. Preserve `CURRENT.UF2` if offered, but check its coverage before calling it a backup. Consult [hardware research](research/hardware.md) for the evidence and unknowns.

Do not run mass erase, unlock/recover commands, write UICR, or replace SoftDevice/bootloader. The ZIP does not contain a full bootloader restore image. If USB recovery disappears, this project currently has no proven independent recovery method.

## Build and inspect

Run the pinned firmware build and the repeatable [acceptance harness](acceptance.md). Builds must reserve factory low flash, filesystem/bootloader space, and bootloader-owned RAM. An ELF is a build result, not an approved flash image. Do not substitute a generic nRF52840/nice!nano UF2 converter or use an example linker map.

The read-only guard checks 256-byte, family-tagged nRF52833 UF2 payloads in `[0x27000, 0x65000)`, contiguous blocks and vector validity:

```sh
python3 scripts/image_guard.py --image /absolute/path/to/application.uf2
```

The upper limit is a conservative inference from the factory board package, pending your actual bootloader identity. The guard's pass message explicitly leaves device compatibility unverified.

## First physical trial, after the gate opens

1. Confirm ANSI physical layout and exact role, bootloader identity, handoff, memory protection, and a recoverable factory application for that role.
2. Record build commit, role, image hash and guard result. Back up available settings/pairing state separately; application UF2 is not a settings backup.
3. Test the right half first only if its independent bootloader/recovery route is confirmed. Preserve the left half as a factory keyboard until that trial succeeds. Never flash the receiver with a half's firmware.
4. Use the confirmed transport for that bootloader. The observed factory route requires a legacy Adafruit serial DFU application package, not a UF2 copy. Package manifest, initialization data, application range, size and erase semantics must be independently validated before any serial transfer. Wait for actual re-enumeration, then verify physical keys, battery and reset/recovery before continuing.
5. Test the left, then receiver individually, with rollback available at each step. Factory and RMK radio links are incompatible; a mixed pair does not constitute a transport test.
6. Pair and execute all [hardware acceptance tests](acceptance.md) across the requested host platforms. Failed recovery, lost transitions or stuck keys block a working-first-version claim.

These are conditional trial steps. Device-specific entry and copy commands will be supplied after identification and recovery are verified; there is deliberately no automated flashing command in this repository yet.

# Offline image-safety review

Review date: 2026-10-01. This pass inspected repository code and previously saved local evidence only. It did not access a device, open a serial port, change device state or write firmware.

## Finding and regression

The factory UF2 guard accepted an initial stack pointer such as `0x20000008`, inside the low 32 KiB reserved by both linker layouts for bootloader warm-reset state. A structurally valid UF2 could therefore pass despite violating this project's RAM preservation boundary. The root agent tightened the guard to require `0x20008000 < SP <= 0x20020000`, with eight-byte alignment. The lower bound is exclusive because a descending stack at `0x20008000` would push into the reserved region.

The independent regression test rejects three stack addresses within or at that boundary and accepts the first aligned address above it. All 16 image-guard host tests pass after the correction. This validates the guard's structural checks, not firmware execution or recovery on hardware.

## Application and migration boundaries

- The existing guard accepts only generic nRF52833 application-family UF2 blocks, contiguous from `0x27000` through at most `0x65000`. Every target address, size, block count, block index and magic word is checked. The upper boundary is page-aligned, so rounding a permitted application length up to a 4 KiB erase page does not enter settings at `0x65000`.
- `memory-factory.x` uses that same flash range and reserves low RAM. The optional linker recovery fragment places the marker at application base plus `0x200`, moves executable text to base plus `0x204`, and asserts vector-table and marker sizes.
- `memory-sdc.x` intentionally starts at `0x1000`, replacing resident S140 while preserving MBR and the same upper flash/RAM boundaries. Such images are rejected by the current guard. There is no approved migration guard or flashing route; cross-building that layout does not open the migration gate.
- Factory restore images are deliberately rejected by this application guard because they include S140/settings and use the board-specific family. Any future restore guard must check the exact role-specific backup hash, complete range, family and factory handoff metadata separately; widening the application guard would weaken its purpose.

## Saved backup coverage

Re-reading the saved left and right `CURRENT.UF2` files independently confirmed 1,728 contiguous 256-byte payload blocks covering exactly `0x1000..0x6d000`, with family `0x239a0029`. Their SHA-256 hashes matched those already recorded in the device observations and recovery documentation. No device read occurred in this review.

These files exclude MBR `0..0x1000`, filesystem `0x6d000..0x74000`, bootloader/configuration `0x74000..0x80000` and UICR. They are not full-chip backups. There is no saved dongle backup in the reviewed evidence directories.

## Limits that remain

The guard checks that the reset vector is Thumb and points inside loaded flash. It cannot establish that the target contains viable executable instructions, that every interrupt vector is correct, or that the image's role matches the connected board. A role manifest, reviewed build and device-specific evidence remain necessary.

The marker is an existing bootloader convention, not execution validation. The owner-assisted right-half trial demonstrated its USB-first recovery behaviour; equivalent left and receiver behaviour remains unverified. Power loss during an update and interrupted marker-page programming were not tested. Serial DFU erases pages at START before receiving the whole image, so a guarded UF2 does not substitute for validation of the actual serial package and its erase semantics.

Sources: repository [guard](../../scripts/image_guard.py), [tests](../../tests/test_image_guard.py), [factory layout](../../firmware/memory-factory.x), [migration layout](../../firmware/memory-sdc.x), [recovery fragment](../../firmware/bootloader-recovery.x), [migration evidence](migration-safety.md), [serial package analysis](serial-probe-trial.md) and [recorded right trial](../recovery-probe.md).

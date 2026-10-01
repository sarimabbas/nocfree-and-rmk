# Moving the application below S140: safety evidence and gates

Research dated 2026-10-01. No device writes or mode changes were performed by this research agent. Candidate compilation alone does not establish safe recovery.

## What is now established

The root agent read the user's left ANSI bootloader after the owner held factory `Fn+5`. The mounted `NocFree &` volume reports `UF2 Bootloader 0.9.2-39-g0147d71`, board/model `NocFree &`, build date 2025-12-26, and S140 7.3.0. Its `CURRENT.UF2` was copied and verified read-only at `.evidence/factory-left/CURRENT.UF2`:

- SHA-256: `88cb768f452682cb12025f2296ee151442349079405a7a11f454f752153e4100`.
- 884,736 container bytes, 1,728 contiguous 256-byte payload blocks.
- Target range `0x1000..0x6d000` (exclusive end); family `0x239a0029`.
- Root inspection found S140 magic at `0x3004`, size `0x27000` at `0x3008`, FWID `0x0123` at `0x300c`, version 7.3.0.

This is a backup of the resident S140 plus application/settings region. It excludes MBR `0..0x1000`, factory filesystem `0x6d000..0x74000`, bootloader/config/metadata `0x74000..0x80000`, and UICR. It must never be described as a full-chip backup. Excluded regions can remain in place because candidate/restore operations can be constrained not to write them; the bootloader's own application-completion metadata update is an expected exception, not a replacement of its code.

## Exact upstream commit found

GitHub resolves the installed hash prefix to [0147d71e73b9a2c217f56dbc9877d07bb45d6467](https://github.com/adafruit/Adafruit_nRF52_Bootloader/commit/0147d71e73b9a2c217f56dbc9877d07bb45d6467), a 2025-12-19 build-system merge. The vendor build date is seven days later. This identifies the upstream base reported by the build, not the complete vendor binary: the NocFree board definition and any uncommitted vendor modifications are not established by a git-describe string.

At that exact upstream commit:

- [uf2cfg.h](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/usb/uf2/uf2cfg.h) defines user flash from MBR end `0x1000` to bootloader minus reserved app-data pages. The observed readback end independently confirms `0x6d000` for this unit.
- Board application family is `(USB_DESC_VID << 16) | USB_DESC_UF2_PID`; observed `0x239a0029` is consistent with a vendor board-specific application ID. Generic nRF52833 application family is `0x621e937a`.
- [ghostfat.c](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/usb/uf2/ghostfat.c) accepts both board-specific and generic application families. It explicitly treats SoftDevice as part of an application UF2, allowing writes in the full user range and skipping MBR addresses. The backup's custom family is therefore intentionally suitable for restoring both S140 and application under this upstream implementation. Bootloader family `0xd663823c` selects a different path and must be prohibited.
- [dfu_types.h](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/lib/sdk11/components/libraries/bootloader_dfu/dfu_types.h) chooses the application start from S140 metadata when its magic exists, otherwise MBR end `0x1000`. A candidate linked at `0x1000` must overwrite the old S140 metadata and ensure the word at `0x3004` is not S140 magic `0x51b1e5db`; merely moving the linker origin while leaving old magic would boot from the wrong address.
- [msc_uf2.c](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/usb/msc_uf2.c) finalizes an application image when all advertised blocks have arrived and updates application-validity settings. This makes incomplete copies and metadata regeneration part of the failure model.

Consequently `0x1000..0x65000` provides 400 KiB for a conventional RMK central while preserving MBR, filesystem and bootloader code. It intentionally replaces S140, contrary to the original application-only preservation policy. It is a separate migration profile, not a reason to weaken the original profile's bounds.

## Independent recovery remains the open gate

Factory `Fn+5` proves software entry while factory firmware works. It does not survive an unbootable custom application. A Rust 1200-baud recovery interface also depends on that application running. Neither supplies independent recovery.

The manufacturer's [troubleshooting page](https://www.nocfree.com/pages/nocfree-and-troubleshooting) links separate left and right recovery videos. The left link resolves to [W38TJN79V8Q](https://www.youtube.com/watch?v=W38TJN79V8Q); current metadata/download attempts both return **Private video**, so no left recovery pads or physical procedure could be verified. The user-supplied [jGfepV1DYVE](https://www.youtube.com/watch?v=jGfepV1DYVE) is the **right** keyboard tutorial; its module-edge contacts cannot be assumed to describe the left PCB. The root agent's right-frame analysis shows module edge pads, not fine MCU pins. No external reset pinhole was established; the owner corrected that assumption.

The manufacturer page also cautions against opening/bridging internal recovery points without its support team's instruction. That is source advice, not an instruction from the user to stop research. Practically, the unavailable left video leaves the exact safe left reset contacts and procedure unidentified. Request a manufacturer left recovery diagram/video or identify clearly labeled reset and ground contacts on the actual left board before proposing a physical action. Do not infer pads by counting from the right-side video.

## Minimum conditions before recommending the migration flash

1. Validate and retain the role-specific `CURRENT.UF2` and `INFO_UF2.TXT` outside the bootloader drive; preserve hashes and target ranges. Back up each half and receiver individually before migrating that role. A left backup is not a right/dongle backup.
2. Establish a physical left bootloader entry independent of firmware, with exact contacts/control verified for the owner's hardware revision; test that it enters the same MSC bootloader while the factory app is intact. This test is not an application flash.
3. Guard candidate UF2 strictly: correct role manifest, app family, valid/unique/contiguous blocks, start `0x1000`, end at or below `0x65000`, aligned payloads, valid vector SP/Thumb PC, and old S140 magic absent. Prohibit MBR, filesystem, bootloader-family images, UICR and arbitrary unguarded UF2 copying.
4. Guard restore separately: only the exact backed-up role/hash, expected custom family, complete `0x1000..0x6d000` range. Revalidate S140 metadata and factory vectors. This restores overwritten S140 and app/settings bytes; it does not restore unrelated factory files or bootloader code because those must remain untouched.
5. Keep migration an explicit reviewed operation, flash one role first, then check enumeration, recovery entry and readback. Verify every candidate-written address against readback and separately verify retained regions available in that readback. Do not call a successful file-copy request a verified installation.

A complete bootloader dump is useful but not logically mandatory for an application-range migration that never writes bootloader code. A proven independent recovery path, exact role backup and correct guards are mandatory practical safeguards here. Installed-vendor binary equivalence and physical recovery are currently unresolved; the backup materially improves recoverability but does not make an unverified physical recovery path sufficient.

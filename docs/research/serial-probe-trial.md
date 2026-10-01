# Application-only serial DFU for the right USB recovery probe

Research 2026-10-01; no device commands or writes by the research agent. This addresses the explicitly authorized controlled right-side trial, not migration below S140.

## Matching the actual legacy transport

The factory recovery application statically invokes `adafruit-nrfutil dfu serial -pkg <zip> -p <port> -b 115200 --singlebank` after opening the application serial port at 1200 baud. Use this maintained Adafruit legacy transport; do not send guessed raw opcodes or use Nordic secure-DFU formats.

[Adafruit nrfutil source snapshot 7fdfe15](https://github.com/adafruit/Adafruit_nRF52_nrfutil/tree/7fdfe15feee5f304fb7d9b031721dcefa1f72b58) exposes `dfu genpkg`. Application-only package preparation, after verifying local tool help/version, is:

```sh
adafruit-nrfutil dfu genpkg --dev-type 0x0052 --dev-revision 0xffff \
  --application-version 0xffffffff --sd-req 0x123 \
  --application probe.bin trial.zip
```

This command only creates a local artifact. Do not pass `--softdevice`, `--bootloader` or a signing-key option. Match the actual device port and owner-approved operation separately when flashing; device identities can change across bootloader enumeration.

The package must contain exactly an application BIN, application DAT and `manifest.json`, with `dfu_version=0.5`, a single `manifest.application` entry and no SoftDevice/bootloader entries. Its init packet is little-endian `uint16 device_type`, `uint16 device_revision`, `uint32 application_version`, `uint16 softdevice_count`, one `uint16 softdevice_fwid`, then `uint16 firmware_crc16`. Thus one-FWID CRC packet is 14 bytes, matching the official firmware's DAT structure: type `82`, revision `65535`, application version `4294967295`, count `1`, FWID `291` (`0x123`), CRC16 over exact BIN bytes with initial value `0xffff`. [init_packet.py](https://github.com/adafruit/Adafruit_nRF52_nrfutil/blob/7fdfe15feee5f304fb7d9b031721dcefa1f72b58/nordicsemi/dfu/init_packet.py) and [package.py](https://github.com/adafruit/Adafruit_nRF52_nrfutil/blob/7fdfe15feee5f304fb7d9b031721dcefa1f72b58/nordicsemi/dfu/package.py) implement these fields.

The DAT/manifest does not encode absolute application start or keyboard half. A validated right-side role manifest, BIN hash and current device evidence are therefore required outside nrfutil. Matching the chip type alone does not distinguish right from left.

## Exact installed-base flash behavior

At [installed upstream 0147d71](https://github.com/adafruit/Adafruit_nRF52_Bootloader/tree/0147d71e73b9a2c217f56dbc9877d07bb45d6467), `CODE_REGION_1_START` comes from S140 size when S140 magic exists. With the observed S140 7.3.0, that is `0x27000`; it is implicit and cannot be changed by a DAT package.

[Single-bank START handling](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/lib/sdk11/components/libraries/bootloader_dfu/dfu_single_bank.c) accepts application update mode `4` with SoftDevice size `0`, bootloader size `0` and application size equal to the exact BIN length. START immediately erases `ceil(BIN length / 4096)` application pages starting at `0x27000`. DATA writes at that same base plus received offset. A roughly 14 KiB probe therefore erases four pages, `0x27000..0x2b000`, not the entire application slot. Existing application tail bytes remain, but are unreachable from the new image.

For this pure application path, S140/MBR, factory filesystem, bootloader code and UICR are not written. Application-validity/CRC metadata at the bootloader settings page is intentionally updated. The vendor board build can contain undisclosed modifications; exact upstream behavior is corroborated by its version string and factory update-tool use, not by a full binary audit.

[Init validation](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/dfu_init.c) requires Adafruit device type `0x52`, validates installed SoftDevice FWID and checks CRC16. Its special device-revision unlock applies to SoftDevice/bootloader updates, not ordinary application updates. Default revision `0xffff` is consistent with the official application package; do not use the chip-specific unlock revision for a probe.

## Required review before the controlled write

- Right backup hash/role and current right USB evidence must match the authorized trial; left evidence cannot substitute.
- Derive BIN from the guarded right ELF/UF2, not a generic raw ELF converter with unknown offsets. Confirm vector SP/Thumb PC, four-byte aligned length, base `0x27000`, end below `0x65000`, no S140 or UICR payload, and the boot-before-application marker at `0x27200`.
- Parse generated ZIP/DAT/manifest, compare BIN hash byte-for-byte with the guarded probe and recompute CRC16. Confirm application-only mode and exact length.
- Enter the observed factory 1200-baud serial DFU path, then enumerate the newly present bootloader port; do not assume the previous application port identity proves the new endpoint's role.
- After sending an update, distinguish HCI acknowledgements from valid installed behavior. Validate probe USB enumeration, enclosure-closed cold-start recovery and MSC readback before claiming success. Retain factory restore data outside the mounted drive.

START is destructive to the current application pages even before the first firmware DATA packet. An invalid package that fails later may leave an erased or partial application, so all package checks must precede transmission. Neither opening a bootloader port nor generating the package is itself a firmware update.

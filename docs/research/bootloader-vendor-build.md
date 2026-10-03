# NocFree vendor bootloader evidence

Offline inspection, 2026-10-03. No recovery executable was launched and no device,
serial port, firmware transfer, or power control was used.

## Supplied recovery archive

The owner-supplied `output_20260911_v2.4.5.zip` contains nine role/layout UF2
applications, a macOS Firmware Care bundle, and a Windows Firmware Care executable.
Static ZIP and PyInstaller CArchive inspection found the same nine Nordic DFU
application packages in the recovery tools. Each nested package contains an
application BIN, a 14-byte DAT, and a manifest whose only firmware component is
`application`; no `bootloader`, `softdevice`, or combined component is declared.
The manifests require SoftDevice ID 291 (`0x123`) and device type 82.

All nine outer UF2 images use family `0x621e937a`, start at `0x27000`, and end
between `0x38f00` and `0x4dc00`. None addresses the declared upper bootloader region.
The nine application BINs contain no CF2 magic pair `0x1e9e10f1, 0x20227a79`.
That absence does not establish the installed bootloader's CF2 values: these are
application images, not bootloader readbacks.

The recovery program's statically extracted strings explicitly describe
application-only DFU ZIP validation and blocking bootloader ZIPs. Its updater is
not evidence of a vendor-supported bootloader self-update package.

Private inventory, parsed manifests, address summaries, PyInstaller tables, and
extracted original resources remain under `.evidence/bootloader-vendor-inspection/`.
Factory applications, original executables, and generated binaries are not public
repository content.

## Public source search

At inspection time, the public NocFreeKB account listed one repository,
[`NocFree-and-zmk`](https://github.com/NocFreeKB/NocFree-and-zmk), with no published
release assets. Its recursive tree at commit
`8bc5f6fe4531cadc62dc39aa92750fba90e009c4` contains ZMK board definitions and
documentation, but no Adafruit bootloader board definition or original
bootloader HEX/BIN/UF2. Searches for NocFree bootloader sources did not identify
another published vendor source or bootloader artifact. This is a bounded search
result, not proof that no private artifact exists.

The community [recovery document](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/docs/recovery.md)
declares `0x74000..0x7ffff` bootloader space read-only and relies on application
CDC entry or unverified accessible reset controls. It does not furnish the
bootloader build configuration or an independently proven physical reset route.

The vendor [pin table](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/README.md)
describes firmware-derived GPIO/expander assignments and asks developers to verify
them against the hardware revision. Those assignments support constructing a
bounded chord adapter; they do not establish the original bootloader's LED,
button, power-init, or USB descriptor configuration.

## What is known and what is missing

Previously saved `INFO_UF2.TXT` files for both halves report
`UF2 Bootloader 0.9.2-39-g0147d71`, model/board ID `NocFree &`, build date
December 26, 2025, and S140 7.3.0. A reported git description identifies an
upstream base, not the complete vendor source, board configuration, or exact
installed binary.

The pinned upstream [CF2 definition](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/boards/pca10100/pinconfig.c)
shows flash size, RAM size, board ID (VID/PID), family, and port size fields.
Its [self-update parser](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/usb/uf2/ghostfat.c)
checks bootloader/UICR addresses and CF2 board identity. A matching compiled
prototype is not evidence that the vendor build exposes that same update path.

Before installation, the remaining device-specific facts are:

- Exact existing bootloader bytes or a vendor-provided original bootloader and
  complete build configuration, including CF2 identity and button/LED pins.
- The installed image's self-update parser behavior, UICR address expectations,
  and MBR copy compatibility with the reclaimed application layout.
- A rollback image that restores the original bootloader, rather than only its
  application and SoftDevice. Existing CURRENT.UF2 application backups do not
  provide that bootloader backup.
- Physical confirmation that startup chord sampling and peripheral teardown work
  independently on each half, and that restarting actually resets the MCU.

User authorization accepts the trial risk; it does not turn these missing facts
into verified device observations. This research supports offline builds and a
precise install review, not a claim that installation or rollback is proven.

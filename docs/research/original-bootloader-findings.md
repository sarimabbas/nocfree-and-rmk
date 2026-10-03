# Original left bootloader findings

The left half's original bootloader, MBR and UICR were read twice using the temporary inspection application; both observations matched. Private captures and disassembly remain outside Git. This is software readback evidence, not a successful bootloader update or held-key recovery test. The bootloader settings page now describes the inspection application.

## Board configuration

The captured CF2 reports 512 KiB flash, 128 KiB RAM, 32 pins per port, application family `0x621e937a`, and bootloader identity `0x239a0029`. Its location is `0x7d800`. UICR points the MBR to bootloader `0x74000` and parameters page `0x7e000`; the MBR's own override words are erased.

Binary-to-source reconstruction identifies the same GPIO configuration as the pinned [Feather nRF52833 Express board definition](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/boards/feather_nrf52833_express/board.h):

| Function | Configuration |
| --- | --- |
| Physical DFU input | P1.02, pull-up, active low |
| Physical factory-reset input | P0.10, pull-up, active low |
| Primary status LED | P1.15, active high |
| Secondary status LED | P1.10, active high |
| Addressable RGB indicator | P0.16, one pixel, brightness mask `0x040404` |
| Low-frequency clock | Internal RC |
| USB firmware drive | VID `0x239a`, PID `0x0029` |
| USB serial-only mode | PID `0x002a` |

These are configurations encoded in the installed executable. They do not establish external reset accessibility or the electrical wiring of every pin. The status LED polarity is separate from the keyboard's backlight polarity.

UICR reset selection is P0.18 in both entries, NFC protection is disabled, and stored APPROTECT requests protection. REGOUT0 retains its default field; actual supply wiring and voltage cannot be inferred from that alone. The original board initialization contains neither a DCDC enable write nor REGOUT0 programming. A replacement should preserve these choices and must not silently alter UICR.

## Self-update path

The installed executable contains the pinned upstream [UF2 bootloader update branch](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/usb/uf2/ghostfat.c) and [MBR activation path](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/usb/msc_uf2.c). Its branch checks bootloader family `0xd663823c`, fixed address metadata, and matching CF2 board identity. It stages `0x74000–0x7e000` at `0x63000–0x6d000`, then requests MBR COPY_BL for the full 40 KiB region. UICR records are metadata checks, not UICR programming.

The staging region overlaps RMK settings. A migration must preserve their backup and explicitly account for restoration; successful application backup alone does not establish that bonds survive self-update. The staged image must completely specify the fixed-length copy region, including erased padding and CF2, without carrying the device's current settings, MBR, SoftDevice or unique data.

The source and installed binary establish that the self-update route exists. They do not prove it survives power failure or that a new bootloader works on hardware. The right half needs its own capture before its configuration can be called verified. The current left diagnostic remains installed while a board-specific candidate is prepared.

## Left self-update observation

The guarded LEFT-only self-update was performed. The retained inspection
application enumerated afterward, and two complete bootloader reads matched the
reviewed candidate exactly over `0x74000–0x7e000`. MBR, UICR, the parameters page
and bootloader settings matched their original readbacks. This establishes the
observed update and exact resulting bytes on this left half; it does not establish
power-failure recovery or right-half compatibility.

The staged image overwrote the reserved application/settings region as predicted.
The fresh working RMK application and its settings remain backed up. The next
acceptance step removes only the diagnostic's four-byte recovery marker, then
checks normal USB startup and held-key recovery independently of RMK.

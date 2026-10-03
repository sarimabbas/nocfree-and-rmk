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

## Held-key trial failure and factory bootloader restoration (2026-10-03)

The replacement bootloader matched its intended bytes, but two owner-assisted
held-key startup trials did not enter recovery. The marker-free inspection
application continued to enumerate; it had no software reset command. Removing
the established recovery marker before proving the new chord on hardware left
the keyboard unable to type and without a demonstrated closed-case recovery
route. Successful builds, host tests and byte readbacks did not establish that
route.

With the left enclosure open, an owner-operated current-limited contact test
opened the UF2 drive. This is a device observation, not a published pad map:
voltage measurements and a right-half video do not prove left contact identities.
The exact saved original factory bootloader was then restored through its
self-update container. Two matching inspection reads equalled the original
entire upper region, MBR and UICR. The original working RMK application, protected
gap and saved settings were restored separately; the complete readable payload
matched the fresh pre-inspection backup. The owner subsequently confirmed that the left keyboard types again. The right half and receiver were not changed.

Further recovery work uses an opt-in application startup shim. It requests the
factory bootloader through the existing GPREGRET convention; it does not modify
the bootloader or UICR. Its own entry and vector table remain application data
and can be corrupted by a bad update. Independent source review and linked-image
checks are required, followed by real startup/chord/typing tests; none substitutes
for that hardware acceptance.

## Application shim acceptance on the left (2026-10-03)

The first application shim typed normally and matched its guarded readback, but
held-key startup did not enter recovery. Temporary startup instrumentation then
reported `boot:noUSB`: the early VBUS condition skipped key scanning. This
observation does not establish whether USB detection was still settling or the
application had already started during the unplug interval.

The corrected shim checks the local chord on every startup, without a VBUS
precondition. The owner held left Fn+Escape before unplugging USB, kept both held
through a five-second disconnect and reconnection, and confirmed that the factory
UF2 drive appeared **before either key was released**. RMK's ordinary bootloader
action runs on release, so this distinguishes the observed startup entry from
that runtime shortcut. The drive identity and entire application readback matched;
all untouched readable gap and fresh settings bytes remained exact. A subsequent
middle-WIRED USB restart without held keys typed normally without recovery.

These checks establish the observed closed-case startup/recovery procedure on
this left half. The original factory bootloader remains installed. The right
half and receiver were unchanged; right startup recovery, interrupted holds,
and corruption of the application entry are not covered by these observations.
The retained temporary manufacturer diagnostic does not change the product name.

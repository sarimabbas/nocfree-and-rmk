# Tested right-half update foundation

Hardware trial completed on the owner's right NocFree AND, 2026-10-01. Source checkpoint `840adb9d65de44370c0b37aa5d737fd4aa717850`. The right now runs the **USB-only recovery probe**, not keyboard firmware. The left and receiver remain factory firmware. No bootloader replacement, MBR/UICR write or SoftDevice migration was performed.

## Observed results

| Check | Result |
|---|---|
| First installation | Passed through factory 1200-baud entry and application-only Adafruit serial DFU |
| Application identity | `4c4b:4650`, `NocFree Recovery Probe Right` |
| Version greeting | `NocFree recovery probe 0.1.0 right factory` |
| Software update entry | 1200 baud with DTR low enters MSC bootloader `239a:0029` |
| Battery-first startup | OFF + USB absent five seconds; ON without USB five seconds; connect USB → probe runs |
| Recovery without application command | From running probe: OFF + USB absent five seconds; reconnect USB with switch OFF → MSC bootloader |
| Probe readback | Exact BIN match; resident S140 and user flash above `0x2b000` unchanged |
| Factory rollback | Saved right `CURRENT.UF2` copied through MSC; original `239a:80d8` factory USB identity returned |
| Probe reinstallation | Passed through serial DFU; final full user-flash readback matches first probe installation |
| Final state | Battery-first restart; probe identity and greeting verified |

This validates the tested right-half update/recovery workflow on macOS. It does not validate keyboard input, radio, battery measurement, Windows/Linux behavior, automatic rollback or arbitrary interrupted writes. The factory restore was verified by its returning normal USB identity; a complete readback immediately after that restore was not obtained. Final probe readback confirmed the expected entire readable user-flash region after reinstallation.

The right switch OFF alone does not remove USB power. Full startup sequences therefore include unplugging USB. The marker's startup window holds the bootloader when USB enumerates; unplugging afterward does not start the application. Start on battery first when you want to run the probe. Cold USB recovery was demonstrated without opening the enclosure or sending a command from the application.

## Preserved data and bounds

- Probe: 14,448 bytes across ELF file-backed flash, beginning `0x27000`; BIN ends `0x2a870`. Serial DFU erases four pages through `0x2b000`. Packaged UF2 payload ends `0x2a900` after 256-byte padding.
- Existing Adafruit recovery marker: `0x87eeb07c` at `0x27200`; initial SP `0x20020000`, reset vector `0x27205`.
- UF2 SHA-256: `f8b4e58c4a0b3d412ea51a4fd708c381ca3afce36b6d029b2203802f5029cdb1`.
- Full probe user-flash readback SHA-256: `ba2d103cbe13816d85f184de6583123f9c3fb4525646eb59cefcc8e5bed4da00`.
- Original right backup SHA-256: `b74ba6c686b15a74f837ba26eb67efd241a6744c48d64d6fed7eb72974f3f043`, stored privately in `.evidence/factory-right/CURRENT.UF2`.

Private factory firmware and readbacks remain excluded from Git. The backup contains S140 and user flash `0x1000..0x6d000`; it is not a bootloader, filesystem or full-chip backup. Bootloader application-validity metadata legitimately changes during updates. See [serial package review](research/serial-probe-trial.md) and [foundation architecture](research/update-foundation.md).

## Future incremental work

Keep the existing bootloader and reserve the recovery marker in each development image. Build complete, versioned images and retain the previous working image on the Mac. Guard each exact role/layout before installation, test its version and readback, and preserve a host-held restore image. This is manual rollback, not two on-board firmware slots.

The next stage adds input scanning and USB HID while retaining the proven recovery behavior. BLE split, host Bluetooth, receiver transport and battery follow after their own acceptance checks. The approximately 340 KiB complete central needs the separate S140-replacement layout; that migration has not been tried. The left half still needs its own usable reset/recovery workflow. The USB-only receiver cannot use the half's battery-first startup sequence, so its marker build is not a production solution.

The version greeting is triggered by DTR. For reliable reads after re-enumeration, set 115200 baud, pulse DTR low for at least 100 ms, clear stale input, then assert DTR and read a line. The first post-reinstall read timed out without that explicit pulse; the pulse returned the expected greeting. Choosing 1200 baud with DTR low deliberately requests update mode.

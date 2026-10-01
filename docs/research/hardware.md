# Hardware and factory-image evidence

Inspected 2026-09-30. No bundled recovery application or factory firmware was executed; no device was flashed. Hardware has not been connected or measured. These are published mappings and static artifact evidence, not a tested board support package.

## Sources and confidence

1. [NocFree vendor-published hardware guide](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/README.md): pin aliases, MCU, radio architecture, expander ports, battery scale. The guide says these were derived from factory Arduino definitions, and requests verification against the relevant revision.
2. [Community architecture](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/docs/architecture.md), [ANSI spec](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/tests/ansi_spec.py), and [recovery](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/docs/recovery.md): implementation choices and explicit unverified assumptions. Do not promote these to schematic evidence.
3. User-supplied `output_20260911_v2.4.5.zip`: SHA-256 `c11381390b421f6ee3c41bcc090513d392368d09c85b93db31a1dc242427c38a`. Machine-readable UF2 observations are in [factory-images.json](factory-images.json). The bundle contains nine application UF2s and Mac/Windows recovery packages; it contains no full flash backup, bootloader image, or SoftDevice image.
4. [UF2 family registry](https://github.com/microsoft/uf2/blob/master/utils/uf2families.json): family `0x621e937a` is nRF52833. Every bundled image, including the dongle, declares that family.

The vendor firmware is proprietary. Its images are retained in the user's Downloads, not redistributed here. The community's MIT license does not extend to those binaries or hardware designs.

## MCU and scan interface

Both halves use nRF52833; the official dongle UF2 declares nRF52833 too. Vendor image strings include `NocFree nRF52833 Left`; the dongle shares that board descriptor. None of this identifies the physical silicon revision, fitted oscillators, inductors, or factory bootloader version on the user's units.

Keys are individual PCA9555 I²C inputs, not a GPIO diode matrix. Both halves use SDA `P0.11`, SCL `P1.09`; factory speed is published as 400 kHz. Three expanders use addresses `0x20`, `0x22`, `0x24`. Six logical rows read their input ports in this order:

| Logical row | Address | Port | Input-register address |
|---|---|---|---|
| 0 | 0x20 | 0 | 0x00 |
| 1 | 0x20 | 1 | 0x01 |
| 2 | 0x22 | 0 | 0x00 |
| 3 | 0x22 | 1 | 0x01 |
| 4 | 0x24 | 0 | 0x00 |
| 5 | 0x24 | 1 | 0x01 |

KR adds `0x21/P0`. The factory sets input-polarity inversion to `0xff`; the community scanner instead explicitly writes `0x00` to both polarity registers, writes `0xff` to both direction registers, verifies them, and interprets physical low as pressed. A warm-reset port must initialize inversion rather than inherit factory state. Read two input bytes per expander and preserve prior input state on bus failure; a failed read is not a release.

PCA9555 interrupt is active low, pull-up: left `P0.31`, right `P0.05`. Use it for idle wake only after checking electrical behavior; periodic scans during active debounce and a periodic idle reconciliation avoid relying exclusively on an interrupt edge.

## Other pins

| Function | Left | Right | Published behavior |
|---|---|---|---|
| Battery ADC | P0.04 | P0.04 | 12-bit factory sampling |
| Divider enable | P0.05 | P0.31 | High while sampling, low afterwards |
| Red charge/low-battery indicator | P0.09 | P0.17 | Shared charge status; factory emulates open drain and releases on USB power |
| Backlight | P0.20 | P0.20 | Endpoint brightness 255 drives low; physical polarity unverified |
| Blue status LED | P0.10 | Not published | Left active low |
| BLE-position switch | P0.15 | Not published | Left input pull-up, low selects BLE |
| Dongle-position switch | P0.17 | Not published | Left input pull-up, low selects 2.4 GHz |

Do not drive charge-indicator pins push-pull. Do not enable DC/DC or choose an external low-frequency crystal until fitted components are confirmed. Dongle GPIOs, status LEDs, switches, oscillator configuration, and accessible reset/SWD are not documented by the cited sources.

Battery conversion uses a published scale of `130/100`, i.e. battery voltage = measured ADC pin voltage × 1.3. This is a conversion ratio, not evidence of exact resistor values. ADC reference/gain, settling time, battery chemistry/curve, and charge detection must be calibrated on the actual revision. Ensure cancellation/error paths disable the divider.

## Split and dongle radio architecture

Factory right internal nRF52833 RADIO uses nRF24-compatible ESB to the left's external nRF24L01. Left internal RADIO supplies BLE HID or sends ESB to the USB dongle on a separate channel. The external nRF24 is the split receiver, not the dongle link.

Left external nRF24 pins: SCK `P0.28`, MOSI `P0.29`, MISO `P0.30`, CE `P0.03`, CSN `P0.02`. No IRQ or power-enable pin is published. Right has no external SPI radio in the documented path.

A new BLE split protocol can avoid the external radio and reuse RMK's split support. A reflashed USB receiver can use BLE to receive both halves, avoiding proprietary ESB compatibility work. Whether native RMK supports the exact left-central/right-peripheral versus dongle-central topology with this MCU and memory budget must be verified separately. The factory receiver protocol/pairing bytes are not documented; compatibility with an unchanged factory dongle cannot be claimed.

There is no published wired inter-half data connection. USB to the left with a wireless right is a wired host mode; fully wired split operation requires separately confirmed hardware or two host USB devices. Host support follows ordinary USB/BLE HID, but macOS/Windows/Linux acceptance and reconnect behavior remain hardware-test requirements.

## Layout: community assumptions needing physical confirmation

The community ANSI model uses 84 keys split 37 left / 47 right; row population is left `[7,7,6,6,6,5]`, right `[8,8,8,8,8,7]`, taking consecutive low bits of each port. Its own rationale derives these counts from ANSI geometry split between T and Y. The vendor guide publishes port order, not a per-switch PCB net map. Community tests validate consistency with that assumed mapping, not physical wiring.

The ANSI default visual rows are:

- `Esc F1 F2 F3 F4 F5 F6 | F7 F8 F9 F10 F11 F12 PrintScreen Home`
- `` ` 1 2 3 4 5 6 | 7 8 9 0 - = Backspace PageUp ``
- `Tab Q W E R T | Y U I O P [ ] Backslash`
- `Caps A S D F G | H J K L ; ' Enter Delete`
- `LShift Z X C V B | N M , . / RShift Up PageDown`
- `Fn LCtrl LAlt LGui Space | Space RGui Fn RAlt Left Down Right`

Static binary inspection partially corroborates the visual legends: the official ANSI left image contains a little-endian 32-bit action table at flash address `0x4d086` with `Esc, F1..F12, PrintScreen, Home` followed by empty/numpad slots. A row at `0x4d326` contains `Tab, Q, W, E, R, T, Y, U, I, O, P, [, ], Backslash`, then empty/numpad slots; rows have a 21-slot stride. This establishes matching legend order in factory data, not the expander-bit wiring or a complete decoded action format.

User layout and scan-bit mapping must be identified before firmware is labeled ready to flash. ISO/JP/KR cannot be obtained safely by copying ANSI geometry.

## Flash evidence and preservation

All official UF2s start at `0x27000`, carry 256-byte payload blocks, and have contiguous target ranges. ANSI left ends exclusively at `0x4d900`, ANSI right at `0x3b500`, dongle at `0x38f00`. These are occupied image ranges, not the maximum application partition.

Factory strings identify Adafruit nRF52 Arduino 1.7.0. [Its nRF52833 S140 v7 linker](https://github.com/adafruit/Adafruit_nRF52_Arduino/blob/1.7.0/cores/nRF5/linker/nrf52833_s140_v7.ld) places application flash at `0x27000..0x6cfff` and RAM at `0x20006000..0x2001ffff`. [The upstream bootloader linker](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/master/linker/nrf52833.ld) places bootloader at `0x74000`, config `0x7d800`, MBR parameters `0x7e000`, settings `0x7f000`, with reserved warm-reset RAM at `0x20007f7c..0x20007fff`. Preserve these RAM bytes if relying on that bootloader's warm-reset handoff; confirm compatibility before choosing a Rust RAM layout.

The community partitions application `0x27000..0x64fff` and settings `0x65000..0x6cfff`, preserving factory filesystem `0x6d000..0x73fff`, MBR/SoftDevice below `0x27000`, and bootloader `0x74000..0x7ffff`. These upper boundaries remain linker-based inference until device bootloader metadata/backup confirms them. A Rust BLE stack does not need to replace the resident SoftDevice merely because it does not use it. Never write UICR or mass erase as part of application flashing.

## Recovery evidence and gates

The Mac recovery bundle is a PyInstaller application. Static extraction of its embedded `nocfree_recovery` bytecode constants, without executing it, shows `enter_dfu` opens serial at 1200 baud then closes. Its preparation strings say to connect one device at a time, set the left to USB mode, and use a data-capable cable. This corroborates factory serial DFU entry; it does not prove the user's unit exposes that port or that serial DFU also produces an MSC UF2 volume.

Ordered top-level constants and names indicate left/dongle USB ID `0x2886:0x8029`, right `0x239a:0x80d8`, bootloader `0x239a:0x002a`, expected S140 FWID `0x0123`. Treat descriptor matches as identification clues, not exclusive role proof: left and dongle share identity. The recovery tool accepts application DFU ZIPs and rejects bootloader ZIPs. Nested dongle manifest explicitly requests SoftDevice `291` (`0x123`).

Before a first application flash, collect the connected role, layout, USB descriptors, `INFO_UF2.TXT` if MSC entry is available, and a readable current firmware backup if supported. Prove factory recovery entry and rollback on the actual unit. If firmware cannot boot, 1200-baud software entry may no longer work; an independent reset/bootloader-entry path must be identified. Community documentation says accessible double-tap reset is unverified. No independent physical recovery sequence was established from this bundle.

The v2.4.5 catalog reports stale-event and I²C-fault fixes, 13 ms right debounce, dongle rollback to v2.4.3, and remaining interference effects. Zero missed inputs and zero latency are not promises software can substantiate without a bounded hardware acceptance test, radio-interference testing, and reconnect/fault injection. The deliverable must separate compilation from those results.

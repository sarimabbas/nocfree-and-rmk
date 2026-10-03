# Original left-half configuration

This directory is only for the measured left half. It does not establish the
right half's pins or configuration.

The private original bootloader readback identifies nRF52833, flash start
`0x74000`, CF2 at `0x7d800`, MBR parameters at `0x7e000`, and settings at
`0x7f000`. The pinned upstream nRF52833 linker already uses those addresses.

The original `board_init`, `button_pressed`, `led_tick`, `led_state`, and
`usb_desc_init` were disassembled independently. They establish:

| Setting | Measured value |
| --- | --- |
| Status LEDs | P1.15 and P1.10, active high |
| Physical DFU / factory-reset buttons | P1.02 / P0.10, pull-up, active low |
| NeoPixel | P0.16, one pixel, brightness mask `0x040404` |
| USB VID / UF2 PID / CDC PID | `0x239a` / `0x0029` / `0x002a` |
| USB manufacturer / product | `NocFree` / `NocFree &` |
| Volume label / board ID | `NocFree &` |
| LF clock | Internal RC, matching upstream `board_init` |
| Board-specific initialization | No-op; no DC/DC or REGOUT0 writes |

These hardware constants match upstream `feather_nrf52833_express`; they are
not Nordic DK pin guesses. The held-key adapter uses the separately verified
PCA9555 scanner wiring. Its behavior has host tests; the replacement bootloader
has not been validated on hardware.

`build_board.py` requires a private, two-observation original LEFT capture,
checks its hashes and UICR/CF2 values, and generates `pinconfig.c` privately
from the exact original 2 KiB CF2 region. No captured binaries are committed.
It builds only an ELF and a flat `0x74000..0x7e000` binary, excluding UICR
metadata sections. It never packages, uploads, resets, or flashes a device.
The `SystemInit` object also undefines upstream reset/NFC UICR programming
features after global compiler flags. This retains the existing pin settings
without including startup reprogramming paths.

```sh
python3 experiments/held-key-recovery/build_board.py \
  --evidence PRIVATE_ORIGINAL_LEFT_READBACK \
  --toolchain ARM_TOOLCHAIN_BIN \
  --output NEW_PRIVATE_BUILD_DIRECTORY
```

The actual patched startup function runs through the repeatable host harness
before every build. Successful compilation and byte checks are not hardware
or self-update validation.

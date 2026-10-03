# Temporary bootloader inspection application

Prepared 2026-10-03; not hardware validated or installed by this preparation.
This is an application replacement for collecting existing bootloader evidence,
not a bootloader replacement. The original vendor tools were inspected without
executing them; they do not supply the original bootloader.

`firmware/src/bin/bootloader-inspect.rs` runs the previously used Embassy USB/HFXO
initialization path. It starts no keyboard scanner, I2C, BLE, storage, or radio
task. Its only USB interface is CDC, with distinct products
`NocFree Boot Inspect Left` and `NocFree Boot Inspect Right`, development VID/PID
`0x4c4b:0x4670` (left) / `0x4c4b:0x4671` (right), and a static role-specific serial
`nocfree-boot-inspect-left-v1` / `nocfree-boot-inspect-right-v1`, not a device-derived
identifier. It requires exactly one half
role, `bootloader-inspect`, `reclaimed-softdevice`, and `usb-recovery-first`.

## Explicit read protocol

The host must send exactly `READ_BOOT_V1\n`. A bounded 32-byte command buffer
handles requests split across USB packets. Unsupported or overlong commands
return `ERR REQUEST`; no requested address or arbitrary memory read is accepted.
No data dump occurs before the explicit request. Disconnects abandon the response;
the host must discard incomplete captures and request again.

Each complete response consists of:

1. `BOOT_READ_V1 LEFT` or `BOOT_READ_V1 RIGHT` followed by a newline.
2. Fixed-order `AAAAAAAA:WWWWWWWW\n` uppercase hexadecimal address/word records.
3. `END BOOT_READ_V1\n`.

The words are volatile aligned 32-bit reads from these fixed ranges, with exclusive
ends:

| Region | Start | End | Words |
| --- | --- | --- | --- |
| MBR | `0x00000000` | `0x00001000` | 1,024 |
| Bootloader/configuration/settings | `0x00074000` | `0x00080000` | 12,288 |
| UICR | `0x10001000` | `0x10001308` | 194 |

The last UICR word is REGOUT0 at offset `0x304`, as declared in the pinned
`nrf-pac 0.4.0` nRF52833 UICR implementation. Thus each complete response contains
13,506 records. A host verifier must enforce every address and the closing frame,
not infer completeness from a serial timeout. Record bytes describe integer words;
reconstruct little-endian target bytes when producing private binary evidence.

Reading UICR does not program it. The inspection feature enables the pinned HAL's
opt-in `preserve-uicr` feature: requested UICR changes return `Failed`, with NVMC
programming code excluded. Debug configuration is explicitly `NotConfigured`.
This corrects the initial prototype's default HAL initialization, which could
program UICR despite a read-only inspection loop. No NVMC command, flash
erase/write, UICR write, reset, unlock, bootloader transfer, or serial-baud reset
handler belongs in this corrected application. See the independent
[inspection review](bootloader-inspection-review.md).

FICR hardware identifiers are not read. UICR customer words and bootloader data can
still contain private configuration, so captures and reconstructed images must
remain in ignored private evidence, never public commits or release artifacts.

## Installation and restoration boundaries

The application retains the tested recovery-first marker. USB-first recovery is
therefore available before this application executes; the application adds no
new recovery assumption or bootloader-level gesture. Normal startup observation
still needs the established battery-first procedure.

Root must cross-build both roles, run image guards, review the output, save the
current working application/settings, and arrange a device-specific trial before
installation. The inspection code itself cannot install or restore anything.
Companion/host tooling must not confuse this CDC interface with the factory or
normal RMK keyboard identity. Recovery and restoring the saved application remain
separate, explicitly reviewed operations.

The collector is `scripts/bootloader_readback.py`, using pyserial in a private
tool environment. It requires the role-specific VID/PID and protocol header,
accepts only the exact ordered fixed ranges, and saves evidence only after two
complete readbacks agree. It writes no firmware or reset request. Its private
output is original code/configuration evidence, not a ready-to-flash rollback
package. Bootloader settings in the upper range reflect installation of the
inspection application; do not blindly restore that entire range as the previous
working keyboard's metadata.

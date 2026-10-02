# Left serial-mode stage candidates

Prepared 2026-10-01 at source checkpoint `766bce4`. **Not installed and not approved for transfer.** The left remains factory firmware; the right remains its working Mac USB diagnostic. A read-only host inventory found both normal identities and no recovery bootloader or mounted readback.

The two candidates implement the [serial-stage observation plan](serial-stage-observation.md), grounded in [USB startup boundary research](usb-startup-boundaries.md). The first requests the existing serial-only bootloader after unchanged HAL initialization; the second requests it after the existing USB driver/CDC device has been built. Neither polls USB or tests key input/radio. Existing runtime/HAL MSC-return stages retain their request and behavior.

## Exact artifacts

Each private local directory is `dist/migration-<stage>-probe-left/`, containing `candidate.elf`, `application.bin`, `migration-<stage>-probe-left.uf2` and its manifest. Generated images and factory bytes are excluded from Git.

| Stage | BIN bytes | Exact BIN end | UF2 SHA-256 |
| --- | --- | --- | --- |
| `hal-serial` | `8328` | `0x3088` | `d12c01d0474922294b7e494853f45033c2b4c6f2f6b00e99e7f6416564d7e646` |
| `usb-build-serial` | `8488` | `0x3128` | `4cd140292a561724fec469a8ba19ca485b60b9f18d8be94ab07fc468820dbdc1` |

Both UF2 containers have 48 blocks, standard nRF52833 family `0x621e937a`, initial stack `0x20020000`, Thumb reset `0x1205`, recovery marker `0x87eeb07c` at `0x1200`, and exactly three touched pages: `0x1000`, `0x2000`, `0x3000`, ending at `0x4000`. The linked erased-word section replaces the old S140 detection word at `0x3004`; ELF-derived bytes are padded with FF only through the final touched page. This is a lower-layout migration diagnostic, not a factory-preserving application update.

The independently verified original left restore remains SHA-256 `88cb768f452682cb12025f2296ee151442349079405a7a11f454f752153e4100`, its original full readable container including S140. That archive is not a full-chip backup. Recheck its exact policy and connected-device identity before any approved trial.

## Software verification

- `./scripts/check.sh --reclaimed-softdevice --usb-recovery-first` passed 42 Python safety tests, five Rust scanner tests and two documentation tests, all six role/keymap production builds, and every USB/input/migration diagnostic including this pair.
- `./scripts/check.sh` completed with only the known full-left protected-layout failures: the ordinary and Mac builds overflow by 86,048 and 86,080 bytes respectively. Other roles and diagnostics built. This limitation remains unresolved.
- All ten combinations of two startup stages and eight invalid role selections for the new stages failed compilation for the intended selection/role guard. Formatting and shell syntax checks passed.
- Independent review reconstructed every ELF load byte and checked BIN, page-padded UF2, vector/marker/family/address guards and the exact factory restore. Reset initializes the normal data/BSS ranges; disassembly retains HAL initialization, POWER/USBD IRQ setup and CDC descriptor/device construction before the USB stage's `0x4e` GPREGRET write, DSB and system reset. A diagnostic-only black-box reference prevents the built device from disappearing under optimization.

These are host/build/linked-image checks, not hardware execution, cold recovery, USB enumeration, or typing validation.

## Proposed owner-assisted trial

After approval of this exact pair and the original restore, correlate the physical left in normal mode with its same-port MSC bootloader and require its original exact factory readback before transfer. Keep the right and receiver unchanged.

Install the HAL serial control only. Recheck exact bytes, FF tail and retained readable bytes at/above `0x4000`, and repeat its independent WIRED/USB recovery before battery-first startup. Require observed USB disappearance before the owner switches to Bluetooth while unplugged, waits ten seconds, then attaches USB. Observe without opening a serial port or issuing any bootloader command.

A stable same-port CDC-only `239a:002a`, with no MSC interface, is the calibration result to seek. Absence of a Finder drive alone is insufficient. Silence, ordinary MSC, unexpected identity, loss of proven MSC recovery or any ambiguous sequence stops the pair; restore through the demonstrated physical MSC route. Do not interpret failure as a HAL diagnosis.

Only after successful calibration and fresh physically recovered exact HAL readback may the USB construction image replace it. Repeat the same recovery, readback and startup observations. Restore the complete original left factory container at the end or on a failed gate; confirm its exact readable bytes including S140, then normal factory enumeration. No automatic transfer retries, serial DFU writes, bootloader replacement, UICR writes, watchdog, GPIO indicator or flash logging are included.

The previous factory serial-only observation establishes that the mode exists, while upstream source supplies the proposed `0x4e` mapping. Vendor behavior without S140 still needs calibration. A matched negative-control image would be needed before treating mode changes as independent proof of exact hook execution; this pair prepares no such image and makes no root-cause claim. The companion remains a read-only copy tool and does not run this trial or install firmware.

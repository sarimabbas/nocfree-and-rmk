# Initial bring-up results

These are software checks and binary inspections, not a physical working firmware release. Source is the conventional three-role port based on RMK commit `9607aedf343b17dd6b27307583ae80c4f728fbbd`. No application has been installed on a keyboard or receiver.

## Repeatable local harness

`./scripts/check.sh` with Rust 1.93.1 and Arm GNU 15.2.rel1 completes all roles and returns failure if any fails. On the current source:

| Check | Result |
|---|---|
| Python image guard | 13 tests pass |
| Rust expander driver | 7 tests pass |
| Rust formatting | Pass |
| Left release build | Fails protected FLASH limit (latest regression overflow 86,016 B; prior build 85,984 B) |
| Right release build | Pass: 218,052 B text+data; 41,796 B data+BSS |
| Receiver release build | Pass: 249,772 B text+data; 54,188 B data+BSS |

Each MCU has its own flash; code cannot execute remotely from another half's unused memory. The protected code slot is 253,952 B (248 KiB). Linker alignment makes occupied address span slightly different from `size` text+data: the receiver ends at `0x63fb4`, leaving 4,172 B below `0x65000`.

`--nmagic` removes an ELF header LOAD segment which otherwise appears below the protected application base. The successful role ELFs were inspected with `arm-none-eabi-readelf -l`: all file-backed LOAD addresses lie inside `0x27000..0x65000`; zero-file RAM LOAD addresses lie in `0x20008000..0x20020000`. This is structural evidence only. No ELF or UF2 has been approved for device flashing.

GitHub Actions independently reproduced the left linker overflow with Ubuntu's Arm toolchain. The workflow must remain failing until the conventional full firmware fits; do not mask this as a successful build.

## Measured experiments

Disposable diagnostic binaries used fictional larger flash maps only to measure size. They were neither committed as deployable images nor flashed.

- Removing unused combo/fork/Morse/macro capacity and log output reduced the initial left overflow by roughly 78 KiB, but did not reach the protected budget.
- Final conventional `opt-level="z"` is the smallest measured successful profile, about 340 KB of flash content.
- `opt-level=2`: 419,176 B; `opt-level=3`: 444,824 B. Both are larger.
- `opt-level="s"` hit a compiler `Undefined temporary symbol` error on Rust 1.93.1 and 1.98.1; disabling debug info did not resolve it.
- Rust 1.98.1 with `z` reduced a diagnostic image by only 1,572 B; this is insufficient. The repository retains its reproducible tested toolchain pending a justified upgrade.
- Erasing top-level future types made the diagnostic left larger by 1,728 B and increased RAM by 25,224 B. Rejected.
- Erasing upstream BLE's nested future types overflowed the conservative RAM region by about 27 KiB. Rejected.
- Removing Keyboard/keymap altogether still used 310,728 B before adding an inter-half report bridge. Thus that offload alone does not close the gap. No unconventional protocol was adopted.

Published independent image comparisons are in [size options](research/size-options.md) and [community/ZMK evidence](research/community-size.md). They show larger MCUs or reclaimed low flash as the common current RMK route, while the reduced-function ZMK port fits with a different BLE stack.

## Explicit migration candidate

`./scripts/check.sh --reclaimed-softdevice` uses a nondefault application range `0x1000..0x65000`, replacing S140 with the SDC/MPSL stack linked by current RMK. It retains the same standard transport roles and high 96 KiB RAM. Each MCU receives its own role image.

| Role | Flash text+data | RAM data+BSS | Highest flash end (exclusive) |
|---|---:|---:|---:|
| Left | 339,936 B | 64,556 B | `0x53fe4` |
| Right | 218,052 B | 41,796 B | `0x363c4` |
| Receiver | 249,772 B | 54,188 B | `0x3dfb4` |

All three migration roles cross-build with the pinned toolchain. Independent ELF inspection confirms the left vector table at `0x1000`, stack pointer `0x20020000`, reset vector `0x1101`, and file-backed LOAD segments below `0x65000`. RAM allocations end at `0x20017c30`. These bounds leave the MBR, RMK storage, factory filesystem and bootloader regions outside the application image.

The default factory-preserving layout remains unchanged and still fails for the left. CI checks both layouts independently. Link geometry does not prove installed bootloader compatibility, erase behavior, recovery, or functioning USB/radio hardware. The default UF2 guard has not been weakened, no migration UF2 has been generated, and no device write has occurred. Physical recovery and restoration verification remain prerequisites to a migration trial.

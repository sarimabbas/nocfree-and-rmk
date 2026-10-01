# Community RMK firmware and the NocFree flash limit

Inspected 2026-09-30. Downloaded public firmware artifacts were parsed as data, never executed or flashed. Measurements below are UF2 payload/span bytes, not UF2 container bytes or compressed archive sizes. UF2 files here use 256 payload bytes inside each 512-byte block.

The preserved NocFree code slot is `0x27000..0x65000`, exactly 253,952 bytes (248 KiB). The project's [build report](../build-results.md) records the conventional RMK central overflow and smaller successful right/receiver builds. I found no community source/artifact demonstrating a current RMK BLE split central below this exact 248 KiB budget.

## Real community projects

| Project/source snapshot | Firmware version | Actual downloaded artifact | Application target span | How it has room |
|---|---|---|---|---|
| [Cornix](https://github.com/numachang/cornix-rmk-custom/tree/d5e7498bcd7f9c4861a26e587ad2d4e7842035be) | RMK development rev `00a42c3`; package identifies itself 0.8.2 | [v0.2.0](https://github.com/numachang/cornix-rmk-custom/releases/tag/v0.2.0), left central | 455,424 B / 444.75 KiB, `0x1000..0x70300` | nRF52840; starts after MBR, not after S140 |
| Same | Same | right peripheral | 290,048 B / 283.25 KiB, `0x1000..0x47d00` | Same |
| [LaLaPad Gen2](https://github.com/e-sp9/lalapad-gen2-rmk/tree/6adf4c4550e67f3761b5e4ea9d9d4cb2b04234ab) | Vendored RMK 0.8.2 | [v0.2.223](https://github.com/e-sp9/lalapad-gen2-rmk/releases/tag/v0.2.223), right central | 460,544 B / 449.75 KiB, `0x1000..0x71700` | nRF52840; starts after MBR, not after S140 |
| Same | Same | left peripheral | 305,664 B / 298.50 KiB, `0x1000..0x4ba00` | Same |
| [Corne RMK](https://github.com/faboulaws/corne-rmk/tree/7208b7d3fb69602a2e15918d2e00252415464ee3) | Vendored RMK identifies itself 0.9.0 | No release asset; CI artifact measured in [size-options.md](size-options.md) | 372,992 B central / 233,984 B peripheral | nRF52840; source linker starts `0x1000` |
| [RMK BLE GH60](https://github.com/HaoboGu/rmk-ble-keyboard/tree/0f4a28efacc096bf650326fb3d0552c2919ebefd) | RMK 0.5 dependency | Checked-in ELF exists; not a modern comparison | Not measured here | nRF52840; source permits 868 KiB starting `0x27000` |

All four downloaded UF2 images declare nRF52840 family `0xada52840`, not nRF52833. None fits NocFree's 248 KiB slot, even if linked at the NocFree base. The nRF52840's physical 1 MiB flash and larger RAM permit these projects to succeed without demonstrating a code-size solution for a 512 KiB nRF52833 with resident S140 preserved.

[Cornix memory.x](https://github.com/numachang/cornix-rmk-custom/blob/d5e7498bcd7f9c4861a26e587ad2d4e7842035be/memory.x) explicitly grants 636 KiB from `0x1000`, reserves storage above `0xa0000`, and locates bootloader at `0xf4000`. [LaLaPad memory.x](https://github.com/e-sp9/lalapad-gen2-rmk/blob/6adf4c4550e67f3761b5e4ea9d9d4cb2b04234ab/memory.x) grants 892 KiB from `0x1000`. [Corne memory.x](https://github.com/faboulaws/corne-rmk/blob/7208b7d3fb69602a2e15918d2e00252415464ee3/memory.x) grants 1020 KiB from `0x1000`; this overly broad link region is not a recovery-preservation pattern to copy.

The RMK 0.9 Corne source disables Cargo default features, selecting storage/Vial/split/BLE explicitly. Its vendored patch strips the extra Vial BLE HID service to improve macOS binding. This is a documented compatibility change, not measured proof of a major flash reduction. Cornix and LaLaPad keep default features. All inspected release profiles already use `opt-level="z"`, `lto="fat"`, and one codegen unit. Thus copying that profile is sensible baseline practice, but it does not explain away the measured oversize.

Searches through GitHub code/repository queries for `nrf52833 rmk` and `nrf52820 rmk`, plus primary web searches, did not identify an independent community firmware/artifact for either chip. This is a scoped search result, not proof none exists. [RMK's hardware table](https://rmk.rs/docs/getting_started/supported_hardwares) lists nRF52833 support but marks nRF52820 untested; an upstream support declaration is not an under-budget community build.

## Direct comparison: NocFree-and-zmk

The [NocFree ZMK port](https://github.com/NocFreeKB/NocFree-and-zmk/tree/8bc5f6fe4531cadc62dc39aa92750fba90e009c4) uses exactly the same 248 KiB application slot and retains S140, filesystem, and bootloader. This is a real published build, not only configuration scaffolding.

I downloaded the `firmware` artifact from [GitHub Actions run 32804587753](https://github.com/NocFreeKB/NocFree-and-zmk/actions/runs/32804587753), main commit `8bc5f6fe4531cadc62dc39aa92750fba90e009c4`, artifact ID `9547719883`:

| Role | Linker-reported flash | Linker-reported RAM | UF2 occupied span | UF2 SHA-256 |
|---|---|---|---|---|
| Left central | 226 KiB = 231,424 B (91.13%) | 59,288 B | `0x27000..0x5f800`, 231,424 B | `4e3853b71d61c92633d3e8f153b144bc8977aa3fd198a297741f1066f7ba261b` |
| Right peripheral | 191,104 B (75.25%) | 44,264 B | `0x27000..0x55b00`, 191,232 B | `9a5d319c7c3cab4ba23daba30ca3141f92b066b3c244bfdb5d5fc4efbce1dd1f` |

UF2 padding accounts for the right payload exceeding its linker-used bytes by 128 bytes. Both family IDs are nRF52833 `0x621e937a`. Build logs configure `CONFIG_BT_LL_SW_SPLIT=y`: Zephyr's native software BLE controller is part of the application. It does not call the resident S140 to save application code; S140 remains on flash to preserve factory recovery compatibility. Modern RMK uses Trouble host plus a separately linked controller (Nordic SDC/MPSL for the relevant examples); neither that controller nor Zephyr's native controller is the resident full S140 image. Similar Nordic naming must not be confused with reuse of the factory stack.

[NocFree limitations](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/docs/limitations.md) explicitly omit receiver/dongle, battery reporting, lighting, status/charge indicators, mode switch, Studio, deep sleep, and gaming modes. Its left provides USB/BLE HID; right is BLE split only, with USB CDC recovery. It has ordinary layers, but no runtime Studio editor.

The same document reports one ANSI unit tested on macOS, all 37 left keys individually checked, and USB/BLE typing observed. It also records residual radio issues at approximately 80 cm obstructed separation, no measured latency, incomplete 84-key sweep, and no Windows/Linux validation. Treat this as useful hardware evidence for a community unit; the user's own hardware revision and recovery mode still need confirmation.

Useful ideas to retain are: one read per expander port pair; explicit polarity/direction initialization and verification; state preservation after I²C failure; dedicated scan work; bounded partitions; 1M PHY; and deeper split queues. The ZMK scanner polls at 100 kHz I²C, 3 ms active/10 ms idle with 5 ms debounce. Those values deliberately trade latency/power for conservative hardware assumptions; they are not suitable evidence for the user's zero-lag requirement.

Its successful fit is evidence that this hardware can run an independent BLE host/controller in the preserved slot for a reduced feature set. Different C/Rust compiler output, BLE stacks, keyboard engines, configurators, and feature sets prevent attributing the entire approximately 110 KiB difference to one option. There is no source-level proof here that using its queue settings or keymap layout will shrink RMK enough.

## Available routes and their limits

1. **Actual code reduction within `0x27000..0x65000`:** keep current RMK, measure symbol/link map sizes, strip unused configurators/logging/USB classes first, compare size profiles, and isolate BLE host/controller/security costs. Save exact binary sizes after each change. These are experiments, not demonstrated community recipes. BLE security/storage and the central's host+split roles can be substantial and cannot be removed casually while claiming equivalent functionality.
2. **Reallocate settings space:** the full inferred application region to filesystem is 280 KiB. Replacing the 32 KiB RMK settings allocation buys at most 32 KiB, insufficient for a roughly 340 KiB central. BLE bonding still needs persistent storage. It also violates the current agreed `0x65000` code ceiling unless explicitly redesigned.
3. **Reclaim resident S140:** starting at `0x1000` frees 152 KiB (`0x27000-0x1000`) and resembles the newer community projects. It is not a code reduction and violates factory S140 preservation. The installed bootloader determines application start from SoftDevice metadata, so changing linker origin alone is not enough. A complete recovery/bootloader migration is a distinct, riskier project.
4. **Replace bootloader or add external flash:** [RMK bootloader docs](https://rmk.rs/main/docs/configuration/bootloader) offer no-swap or external-DFU variants with larger ACTIVE slots. These need a bootloader change and, for external DFU, confirmed SPI NOR hardware. Neither is an application-only solution for this keyboard. No external NOR is documented; the external nRF24 is a radio, not flash.
5. **Alternative BLE controller:** Zephyr's successful size is a reason to investigate a native smaller controller integration, not evidence that RMK can already switch to it unchanged. Requires implementing RMK's `bt-hci` controller boundary and testing security, concurrency, timing, and recovery. Benchmark before committing to the architecture.

Under the present safeguards, the conventional RMK left-central build must be reduced to fit, or remain unflashable. The community comparisons give no justification for weakening the address guard or presenting a bigger nRF52840 image as ready for NocFree.

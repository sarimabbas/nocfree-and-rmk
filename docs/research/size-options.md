# How other current RMK builds fit

Investigated 2026-09-30. Scope: current RMK and conventional central/peripheral roles. An older stack and moving the key engine to the peripheral are **not recommended** for this project. The owner's latest requirement is to retain current RMK and ordinary split behavior.

## NocFree's measured constraint

Current LEFT candidate measured by the implementation harness: **339,924 application bytes**, exceeding protected application flash `0x27000..0x65000` (**253,952 bytes / 248 KiB**) by **85,972 bytes**. The diagnostic experiment removing `Keyboard` and keymap still measured **310,728 bytes**, so moving key processing to the other half cannot resolve the measured lower bound. These are local diagnostic builds, not hardware measurements.

Do not widen into settings/filesystem or overwrite the resident SoftDevice to hide that failure. A device-confirmed memory migration would be a separate project.

## Independent actual build artifacts

Downloaded the named GitHub Actions artifacts read-only, parsed every UF2 header, and counted payload bytes and address extent. UF2 payload sizes include final block padding; they are not ZIP sizes or ELF debug-file sizes. These are published build results, not our hardware tests.

| Repository / exact build | MCU and layout | Central payload | Peripheral payload | What makes it fit |
| --- | --- | ---: | ---: | --- |
| [faboulaws/corne-rmk](https://github.com/faboulaws/corne-rmk/tree/7208b7d3fb69602a2e15918d2e00252415464ee3), [CI run 32305365092](https://github.com/faboulaws/corne-rmk/actions/runs/32305365092), artifact `9384519175` | nRF52840; application starts `0x1000` | **372,992 B** | **233,984 B** | 1 MiB flash and an application origin that does not preserve S140's low-flash region. |
| [t-ogura/rmk-keyboard-aerogu34](https://github.com/t-ogura/rmk-keyboard-aerogu34/tree/869f9d854b5651ce6b34b519019e15d334d924c2), [CI run 34845502439](https://github.com/t-ogura/rmk-keyboard-aerogu34/actions/runs/34845502439), artifact `10347522885`, Vial flavor | nRF52840 Xiao; application starts `0x27000`, S140 retained unused | **473,856 B** (right) | **299,008 B** (left) | 1 MiB flash; custom PAW3222 pointing driver and larger feature set have room. |
| Same Aerogu34 build, Rynk flavor | Same protected low-flash origin | **496,128 B** (right) | **299,520 B** (left) | Same larger MCU. Protocol choice changes measured footprint but does not bring either central near 248 KiB. |

Corne's central ends at `0x5c100`, peripheral at `0x3a200`. Aerogu34 Vial central ends at `0x9ab00`, peripheral at `0x70000`; Rynk central ends at `0xa0200`, peripheral at `0x70200`. All inspected UF2s use 256-byte payload blocks.

Both repositories' RMK package metadata says **0.9.0**, with source pins/forks rather than evidence that the crates.io release is byte-identical. Corne vendors upstream `f8da274` plus its USB-only Vial patch. Aerogu34 pins its current RMK fork to `9b59208e6f411beaf6c425c1b4116628897f5508`. Their feature sets differ from NocFree's, so the figures are comparative evidence rather than a controlled one-variable benchmark.

## Source choices behind those artifacts

[Corne Cargo](https://github.com/faboulaws/corne-rmk/blob/7208b7d3fb69602a2e15918d2e00252415464ee3/Cargo.toml) disables RMK defaults, enables storage/Vial/host lock/split/async matrix/Adafruit entry/1M host PHY, and leaves watchdog off. It uses one codegen unit, `opt-level = "z"`, fat LTO, and [DEFMT_LOG=off](https://github.com/faboulaws/corne-rmk/blob/7208b7d3fb69602a2e15918d2e00252415464ee3/.cargo/config.toml). It still produces a 364 KiB central.

[Aerogu34 Cargo](https://github.com/t-ogura/rmk-keyboard-aerogu34/blob/869f9d854b5651ce6b34b519019e15d334d924c2/Cargo.toml) also disables defaults and uses `z`, fat LTO and one codegen unit; it includes watchdog, split, async matrix and Adafruit entry. Its default is Vial plus defmt. Its [memory map](https://github.com/t-ogura/rmk-keyboard-aerogu34/blob/869f9d854b5651ce6b34b519019e15d334d924c2/memory.x) explicitly preserves the resident SoftDevice, while [packaging](https://github.com/t-ogura/rmk-keyboard-aerogu34/blob/869f9d854b5651ce6b34b519019e15d334d924c2/package.sh) protects its storage. Thus preserving S140 is compatible with current RMK on a 1 MiB chip; that does not imply it fits the NocFree's 512 KiB chip and 248 KiB application partition.

## nRF52833 and nRF52820 examples

The current upstream [tri-mode nRF52833 central](https://github.com/rmk-rs/rmk/tree/9607aedf343b17dd6b27307583ae80c4f728fbbd/examples/use_rust/nrf_dongle/central) uses USB/BLE, split, dongle, storage and Rynk. Its [memory.x](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/examples/use_rust/nrf_dongle/central/memory.x) starts at **0 and allows all 512 KiB**; it is a probe-flashed example, not a preserved-S140 build. The [current nRF52833 template](https://github.com/rmk-rs/rmk-template/blob/be716a19b286454b2173470df125337a37cf7a7e/nrf52833/memory.x) likewise allows all 512 KiB. Neither establishes a successful 248 KiB tri-mode image, and no independent conventional nRF52833 build retaining S140 within that application budget was found in the inspected code search results.

nRF52820 has **256 KiB flash, 32 KiB RAM and USB**, per [Nordic](https://www.nordicsemi.com/Products/nRF52820). RMK's [support table](https://rmk.rs/main/docs/getting_started/supported_hardwares) explicitly marks it **untested**, with no SAADC/battery ADC. The [feature definition](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/Cargo.toml) adds `_no_saadc`, not `_no_usb`. A listed chip feature is not a measured successful USB/BLE/split firmware. No independent published nRF52820 build was found; do not use its feature as proof that every current RMK configuration fits 256 KiB.

## Conclusions and remaining current-stack work

The examined repositories predominantly get around the constraint by using **more flash**, allowing the application to use low flash instead of retaining S140, or supporting smaller roles. They do not demonstrate a hidden compiler option recovering NocFree's missing 84 KiB. Optional feature trimming and log removal remain useful, but those improvements are already represented in the comparative artifacts and local attempts.

Current RMK's `_nrf_ble` selects Trouble's Cortex-M4 P-256 security backend; Trouble security is enabled by RMK's dependency declaration. Disabling required security/pairing code without specifying interoperable host behavior would sacrifice requirements, not establish an equivalent smaller build. Buffer/link/profile counts mostly change RAM and should not be presented as a cure for an 84 KiB flash deficit without measurements. [Upstream features and dependency configuration](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/Cargo.toml).

A continued latest-RMK effort should use isolated, measured builds to find optional upstream code that can be mechanically removed without changing USB/BLE/split/dongle/battery semantics. If that cannot cross the fixed partition bound, keep LEFT's build failing and report the hardware-memory constraint honestly. No reviewed independent example currently closes it.

Historical explanation only: RMK0.6.1 used resident `nrf-softdevice` S1407.x, and 0.7 migrated to Trouble/SDC. That older strategy can save an embedded BLE stack, but lacks the current relay-dongle implementation and automatic dual battery services. It is not recommended here, given the explicit latest-RMK requirement. [Migration evidence](https://github.com/rmk-rs/rmk/blob/a281dcce02d4b56ed99c6dee2eb1a14b021de507/docs/docs/main/docs/migration/v06_v07.md), [0.6.1 dependencies](https://github.com/rmk-rs/rmk/blob/ec516ca3b2906b1bf8cf8c5e80413e9c3ff00f9e/rmk/Cargo.toml).

The firmware uses RMK at commit `9607aedf343b17dd6b27307583ae80c4f728fbbd` (unreleased changes after 0.9.0). `main.rs` selects exactly one hardware role. RMK owns key behavior, HID, Bluetooth profiles, split transport, receiver transport, and persistent bonds. `scanner.rs` adapts the independent PCA9555 inputs to RMK events; `battery.rs` switches the divider around each measurement. The shared, host-tested electrical driver lives in `../crates/nocfree-input`.

Build from this directory so Cargo reads `.cargo/config.toml` and RMK reads `keyboard.toml`:

```sh
cargo build --locked --release --features left
cargo build --locked --release --features right
cargo build --locked --release --features receiver
```

Install the Rust target and an Arm GNU toolchain including newlib. The pinned root toolchain installs the Rust target. `arm-none-eabi-gcc` must be in `PATH`; upstream's Cortex-M4 P-256 implementation requires it.

The linker deliberately limits firmware to `0x27000..0x65000`, reserves `0x65000..0x6d000` for RMK storage, and preserves the low 32 KiB of RAM, including the factory bootloader marker. `cortex-m-rt` sets VTOR to the application vector table. `--nmagic` prevents a loadable ELF header segment below the application boundary. These boundaries are safeguards, not proof of bootloader compatibility. Never install an oversized diagnostic build.

See [build results](../docs/build-results.md) for the exact harness result. Current validation: right and receiver link inside these bounds; left typechecks but exceeds the application region by about 84 KiB. The left cannot be flashed. Removing unused macro/combo/fork/morse capacity and diagnostic logging saved roughly 78 KiB; top-level erased futures made the left larger and were rejected. Internal BLE future erasure exceeded the reserved RAM. Optimization levels 2 and 3 also increased size; level s failed with an undefined temporary symbol on Rust 1.93.1 (with and without debug metadata) and 1.98.1. Rust 1.98.1 with level z saved only about 1.5 KiB, so the pinned toolchain was retained. Preserving the factory SoftDevice and bootloader is a remaining size constraint.

The ANSI map follows the community port's 37 left plus 47 right electrical positions. Fn+1..5 selects Bluetooth profiles; Fn+0 clears the current bond; Fn+B toggles USB/Bluetooth preference; Fn+U selects the RMK receiver profile (hold for pairing). Consumer brightness and media/volume keys are on Fn+F1/F2 and Fn+F7..F12. The 5 ms debounce and continuous input scans prioritize responsiveness; latency, radio behavior, power, battery calibration and OS interoperability still need physical validation.

Battery level uses the factory 130/100 voltage conversion, RMK's voltage-derived capacity estimate, and separate left/right BLE battery characteristics. Charging state is unavailable because the factory charging indicator shares an actively controlled pin. Backlighting and factory proprietary 2.4 GHz compatibility are not implemented.

RMK's current split event forwarding does not guarantee delivery across link failure. Mock scanner tests cannot establish zero lost keystrokes or bounded end-to-end latency. Resolve this and run the physical acceptance harness before calling the port ready.

An explicit, nondefault `reclaimed-softdevice` feature builds a migration candidate with application flash at `0x1000..0x65000`. It replaces the resident S140 SoftDevice with RMK's linked SoftDevice Controller and MPSL. It preserves the MBR, factory filesystem, bootloader and metadata, and retains the same high 96 KiB RAM and storage reservation. Default builds still use `memory-factory.x` and preserve S140. This migration requires independently verified physical bootloader entry, matching bootloader behavior, and restoration of the backed-up SoftDevice/application range; a successful build alone does not authorize flashing.

```sh
cargo build --locked --release --features left,reclaimed-softdevice
cargo build --locked --release --features right,reclaimed-softdevice
cargo build --locked --release --features receiver,reclaimed-softdevice
```

All three migration roles compile with the pinned latest RMK revision and Rust toolchain. Flash content is 339,936 bytes for left, 218,052 bytes for right and 249,772 bytes for receiver; RAM content is 64,556, 41,796 and 54,188 bytes respectively. ELF inspection confirms that all file-backed load segments lie in the selected application flash interval and all zero-fill RAM segments lie in the high 96 KiB RAM interval. These checks establish build fit and region confinement; hardware operation and recovery still require separate validation.

The factory source is named `memory-factory.x` so cortex-m-rt's `INCLUDE memory.x` resolves the feature-selected file generated in `OUT_DIR`. Keeping a source `memory.x` beside Cargo.toml would shadow that generated file and silently ignore the layout selection. The renamed factory file is byte-identical to the original.

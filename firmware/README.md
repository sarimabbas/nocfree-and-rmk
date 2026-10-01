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

All three migration roles compile with the pinned latest RMK revision and Rust toolchain. Flash content is 339,988 bytes for left, 218,052 bytes for right and 249,772 bytes for receiver; RAM content is 64,556, 41,796 and 54,188 bytes respectively. ELF inspection confirms that all file-backed load segments lie in the selected application flash interval and all zero-fill RAM segments lie in the high 96 KiB RAM interval. These checks establish build fit and region confinement; hardware operation and recovery still require separate validation.

The factory source is named `memory-factory.x` so cortex-m-rt's `INCLUDE memory.x` resolves the feature-selected file generated in `OUT_DIR`. Keeping a source `memory.x` beside Cargo.toml would shadow that generated file and silently ignore the layout selection. The renamed factory file is byte-identical to the original.

The left role enables RMK's upstream `adafruit_bl` behavior. Hold the left Fn key, press and release the left Escape key, then release Fn to request the Adafruit bootloader. RMK processes this action on Escape release, writes `0x57` to `GPREGRET`, and requests a system reset. Fn+5 still selects Bluetooth profile 4. This provides a software update-entry request for an operational left application; it has not yet been physically validated on this board.

This entry requires the application, scanner and key processing to be running. It cannot recover a failed boot or a hung scanner. The right half and receiver still need their own independently verified entry and recovery paths. No custom bootloader or OTA updater is included, and there is no health-based rollback guarantee. Flash and restoration remain governed by the separate physical-entry and backup checks.

An additional explicit `usb-recovery-first` feature inserts the existing Adafruit `0x87eeb07c` application marker at application base + `0x200`. The linker reserves the marker before executable text and asserts that it cannot overlap vectors. This feature is off by default for every role. It requests the installed upstream bootloader's recovery-first convention; it does not replace the bootloader or implement rollback.

The upstream revision enters DFU on startup when this marker applies. If USB enumerates within the three-second window, DFU stays active; disconnecting USB after that does not make this installed revision boot the application. A half can instead start on battery without USB, wait for the timeout, and then attach USB after its application starts. Actual vendor behavior and a usable power-cycle procedure must be verified on hardware. An always USB-powered receiver would enter DFU on normal cold plugs, so this option is unsuitable for its normal firmware and is provided only as an explicit analysis candidate.

```sh
cargo build --locked --release --features right,usb-recovery-first
cargo build --locked --release --features left,reclaimed-softdevice,usb-recovery-first
```

The marker can offer an application-independent startup window when the verified power/start conditions apply. It is not a watchdog, A/B update system, health check, or automatic restoration guarantee. Keep the independent host backup and restoration path.

The separate `recovery-probe` binary is a stage-zero, USB-only right-half diagnostic. It initializes USB CDC and the Embassy executor/time driver, sends a version/role greeting when the serial port opens with DTR asserted, and requests the existing Adafruit bootloader when the host selects 1200 baud with DTR low. It does not initialize the radio, key scanner, battery ADC, keyboard processing or storage. The normal keyboard binary remains the default Cargo run target; ordinary role builds do not include this diagnostic.

```sh
cargo build --locked --release --bin recovery-probe --features right,recovery-probe,usb-recovery-first
```

The probe requires the factory `0x27000` layout and the explicit recovery-first marker; incompatible migration or role features fail compilation. Its diagnostic USB identity is VID `0x4c4b`, PID `0x4650`, product `NocFree Recovery Probe Right`. This development identity is not a registered production allocation. The CDC descriptors follow Embassy's composite convention for Windows; enumeration and serial behavior on each host OS still need physical tests.

The approved right-only trial verified USB-first recovery, battery-first application startup, the version greeting, 1200-baud update entry and host restoration. Keep the factory backup and tested startup path: a running CDC application is only the software entry path. No radio or keyboard functionality is expected from this diagnostic. The complete keyboard and other roles still require their own hardware acceptance checks.

The locked probe build passes with 14,192 bytes of code/data and 2,952 bytes of RAM content. File-backed load segments occupy 14,448 bytes including the reserved marker gap, from `0x27000` to `0x2a870`. Independent ELF inspection confirms the initial stack at `0x20020000`, reset handler at `0x27205`, recovery marker at `0x27200`, and preservation of the resident SoftDevice region. USB identity, update entry, cold USB recovery, factory restore and reinstallation now pass on the owner's right half under macOS. See [tested foundation](../docs/recovery-probe.md) for startup sequences, readback evidence and the DTR pulse used for reliable greeting reads.

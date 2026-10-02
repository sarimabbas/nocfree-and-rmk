# Left USB input diagnostic

Status: source reviewed and cross-built, not installed or authorized for a first write. The left remains factory firmware. See [left reset evidence](research/left-reset-evidence.md) for the separate recovery gate.

The existing input diagnostic now selects exactly one half. The left uses the same PCA9555 scanner seam, explicit external crystal startup, RMK debounce, shared keymap and USB transport as the tested right. Its 37 positions are the first slice of the shared ANSI map; the right remains the last 47. No new debounce, HID processing or split protocol was introduced.

From `firmware/`, build without writing to a device:

```sh
cargo build --locked --release --bin input-probe --target thumbv7em-none-eabihf \
  --no-default-features --features left,input-probe,usb-recovery-first
```

The diagnostic requires the factory-preserving application layout and existing recovery marker. Compilation rejects missing/multiple roles, the receiver role, reclaimed SoftDevice memory and omission of the marker feature. Packaging must separately pass the UF2 and serial-package guards; a cross-build does not establish safe device recovery.

Expected development identity is `4c4b:4652`, product `NocFree Input Probe Left`. Expected CDC greeting is `NocFree input probe 0.1.0 left factory\r\n`. These are development identifiers, not registered production identifiers. The left feature also enables RMK's vendor and reset-only DFU-detach interfaces; no application DFU writer or radio task is started. The public USB builder supplies room for the diagnostic CDC interface.

The diagnostic does not initialize battery ADC, backlight, mode selection, BLE or storage. Fn host-profile actions are transparent. The shared left Fn+Escape action requests the existing bootloader on Escape release; unlike factory Fn+5, it has no five-second hold. CDC update entry uses the same 1200-baud/DTR request as the right. Both depend on working application code and cannot replace the independent recovery gate.

Expected base positions, scanner columns `0..36`:

```text
Escape F1 F2 F3 F4 F5 F6
Grave 1 2 3 4 5 6
Tab Q W E R T
CapsLock A S D F G
LeftShift Z X C V B
Fn LeftCtrl LeftAlt LeftGUI Space
```

The repeatable harness now cross-builds both diagnostics alongside all production roles. The recovery-first migration configuration passed 39 host tests and formatting; all three production roles and both factory-preserving input diagnostics cross-built. Invalid diagnostic feature selections were separately checked for compile-time rejection. These are software results, not left hardware validation. The default factory-preserving full left central remains oversized.

Before a first trial, record exact candidate hashes, flash/erase bounds and guarded package results; verify this left's backup and bootloader identity; establish an application-independent recovery route and a plausible application-start sequence; then obtain explicit authorization for the concrete trial. Retain the factory image for rollback. Initial hardware checks must cover identity/readback, startup, independent recovery, keys and releases before proceeding to the disconnect, held-modifier and wake tests in [acceptance](acceptance.md).

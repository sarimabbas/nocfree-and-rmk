# NocFree AND + RMK

An experimental Rust port for the **ANSI NocFree AND**, using current RMK and a conventional left-central/right-peripheral split. **Not ready to flash:** the left firmware still exceeds the protected application partition. No replacement firmware has been installed on a board.

The target is USB or Bluetooth HID from the left half, Bluetooth split communication from the right, separate battery reporting, and a reflashed RMK Bluetooth-to-USB receiver. The factory receiver protocol is proprietary; an unchanged receiver is incompatible with this design. Host behavior still needs macOS, Windows and Linux testing.

The nRF52833 boards read switches through PCA9555 I²C expanders. One small scanner hides that wiring; RMK owns debounce, key behavior, persistent bonds and transports. Three role-specific builds avoid compiling receiver behavior into the right half. We have not adopted an unconventional cross-half processing protocol.

## Current evidence

- ANSI left-half USB identity confirmed on the owner's board.
- Factory 1200-baud bootloader entry and return after reconnect verified without writing firmware. That entry exposed CDC serial, not a UF2 drive.
- Seven Rust input-driver tests and thirteen Python image-guard tests pass.
- Right and receiver firmware cross-build within the preserved flash/RAM ranges.
- Left fails the protected linker limit; the whole build harness correctly fails until this is resolved.

“No missed keystrokes or input lag” remains an acceptance requirement. RMK currently has six-key ordinary rollover, and split disconnect/reconnect delivery needs deliberate testing. Source compilation and mock tests do not establish physical latency or loss-free operation.

Backlighting, indicators, physical mode-switch handling, factory web configuration and optional numpad support are pending. Both battery estimates are implemented but uncalibrated. This is not factory feature parity.

## Build, research and recovery

- [Repeatable build harness](docs/building.md)
- [Build results and rejected size experiments](docs/build-results.md)
- [Architecture](docs/architecture.md)
- [Hardware and factory image evidence](docs/research/hardware.md)
- [RMK compatibility, battery and transport research](docs/research/rmk.md)
- [Measured RMK size comparisons](docs/research/size-options.md)
- [Community and NocFree ZMK comparison](docs/research/community-size.md)
- [Guarded flashing and recovery plan](docs/flashing.md)
- [Observed device behavior](docs/device-observations.md)
- [Physical acceptance requirements](docs/acceptance.md)

Factory firmware and recovery executables stay outside this public repository. Preserve the original recovery ZIP locally. Never flash an oversized diagnostic build, overwrite factory low flash, or replace the bootloader without a verified backup and independent recovery plan.

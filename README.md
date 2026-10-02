# NocFree AND + RMK

An experimental Rust port for the **ANSI NocFree AND**, using current RMK and a conventional left-central/right-peripheral split. **Both halves have passed USB diagnostic recovery and factory restoration trials.** The complete keyboard port is not hardware-ready: all three roles cross-build in an explicit migration layout, but complete split/radio operation, receiver recovery and lower-layout application startup remain unverified.

Both halves previously passed the opt-in [Mac USB diagnostics](docs/mac-mode.md). The right still runs its Mac diagnostic; the left is restored to factory firmware after a [lower-layout migration trial](docs/research/left-migration-trial.md). Independent recovery and exact full factory restoration passed, but the migration probe did not enumerate after battery-first startup. Keyboard backlight is still unimplemented.

The target is USB or Bluetooth HID from the left half, Bluetooth split communication from the right, separate battery reporting, and a reflashed RMK Bluetooth-to-USB receiver. The factory receiver protocol is proprietary; an unchanged receiver is incompatible with this design. Host behavior still needs macOS, Windows and Linux testing.

The nRF52833 boards read switches through PCA9555 I²C expanders. One small scanner hides that wiring; RMK owns debounce, key behavior, persistent bonds and transports. Three role-specific builds avoid compiling receiver behavior into the right half. We have not adopted an unconventional cross-half processing protocol.

## Current evidence

- A later [entry-only probe trial](docs/research/migration-entry-trial.md) returned to bootloader after controlled battery-first startup, consistent with early entry, though hook execution itself remains unproven and HAL/USB runtime remains unvalidated. Exact complete factory restoration passed again. Current hardware: left factory firmware, right Mac USB diagnostic, receiver untouched.

- The left USB diagnostic passed identity/greeting, exact readback, physical USB-first recovery, battery-first startup, exact factory restoration and diagnostic reinstallation. Final startup and owner-reported typing, Shift, Space and Tab passed; the complete acceptance sweep remains pending; see [left trial](docs/left-input-probe.md).
- Factory 1200-baud bootloader entry and return after reconnect verified without writing firmware. That entry exposed CDC serial, not a UF2 drive.
- Factory Fn+5 entry exposes a UF2 drive; its SoftDevice/application readback is saved locally and hash-verified. It does not back up the bootloader or filesystem.
- Seven Rust input-driver tests and 42 Python safety tests (factory updates, migration images and exact factory restoration) pass.
- The right-only USB typing diagnostic is installed. Its version, keyboard interface, software update entry, exact application readback and initial cold recovery passed on macOS. A USB-clock initialization correction resolved the observed battery-first enumeration failure; its subsequent startup, greeting and owner-reported basic typing passed. The complete key sweep and wake/disconnect tests remain pending.
- Earlier right-half recovery-probe installation, USB identity, software update entry, cold USB recovery, factory restore and reinstallation passed on the owner's hardware. The right now runs the newer clock-corrected input diagnostic; the receiver remains factory firmware.
- Right and receiver firmware cross-build within the preserved flash/RAM ranges.
- Left fails the protected linker limit; the whole build harness correctly fails until this is resolved.
- Separately selected `reclaimed-softdevice` builds fit all three roles by replacing S140 with current RMK's radio stack. They preserve the MBR, filesystem and bootloader address regions. The factory-preserving image guard still rejects this layout. A separate migration guard was used only for the explicitly approved left USB trial; full RMK migration images remain unapproved.

“No missed keystrokes or input lag” remains an acceptance requirement. RMK currently has six-key ordinary rollover, and split disconnect/reconnect delivery needs deliberate testing. Source compilation and mock tests do not establish physical latency or loss-free operation.

Backlighting, indicators, physical mode-switch handling, factory web configuration and optional numpad support are pending. Both battery estimates are implemented but uncalibrated. This is not factory feature parity.

## Build, research and recovery

- [Left migration recovery, failed startup and exact factory restoration](docs/research/left-migration-trial.md)
- [Repeatable build harness](docs/building.md)
- [Tested update foundation and current device state](docs/recovery-probe.md)
- [Right-only USB typing candidate and trial checklist](docs/input-probe.md)
- [Build results and rejected size experiments](docs/build-results.md)
- [Architecture](docs/architecture.md)
- [Staged implementation and owner-assisted bring-up plan](docs/implementation-plan.md)
- [Pinned RMK transport reliability review](docs/research/transport-reliability.md)
- [Hardware and factory image evidence](docs/research/hardware.md)
- [RMK compatibility, battery and transport research](docs/research/rmk.md)
- [Measured RMK size comparisons](docs/research/size-options.md)
- [Community and NocFree ZMK comparison](docs/research/community-size.md)
- [Guarded flashing and recovery plan](docs/flashing.md)
- [Offline image-safety review](docs/research/overnight-safety-review.md)
- [Explicit migration safety evidence](docs/research/migration-safety.md)
- [Recovery-first foundation and rollback choices](docs/research/update-foundation.md)
- [Observed device behavior](docs/device-observations.md)
- [Physical acceptance requirements](docs/acceptance.md)

Factory firmware and recovery executables stay outside this public repository. Preserve the original recovery ZIP locally. Never flash an oversized diagnostic build, overwrite factory low flash, or replace the bootloader without a verified backup and independent recovery plan.

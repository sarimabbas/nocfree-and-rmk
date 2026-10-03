# NocFree AND + RMK

An experimental Rust port for the **ANSI NocFree AND**, using current RMK and a conventional left-central/right-peripheral split. **Basic split typing now works through USB, direct Bluetooth and the original receiver reflashed with RMK on macOS.** Cross-half Shift and owner-assisted Bluetooth/receiver reconnection checks passed. These are functional observations; disconnect/release recovery, simultaneous input, measured wake latency and the full feature set remain unverified.

The earlier [Mac USB diagnostics](docs/mac-mode.md) and factory backups remain available privately. The full half images replace S140 with the current radio stack while preserving the existing bootloader; the receiver preserves its resident S140. See the [left trial](docs/research/full-left-trial-candidate.md), [right trial](docs/research/full-right-trial-candidate.md) and [receiver observations](docs/research/receiver-rmk-plan.md). The optional [framework backlight extension](docs/research/backlight-implementation.md) passed a left-only owner trial: off control, visible dimming, tap-and-hold ramping, stop on release and typing during a hold. Both-half lighting and persistence acceptance remain pending.

The target is USB or Bluetooth HID from the left half, Bluetooth split communication from the right, separate battery reporting, and a reflashed RMK Bluetooth-to-USB receiver. The factory receiver protocol is proprietary; an unchanged receiver is incompatible with this design. Host behavior still needs macOS, Windows and Linux testing.

The nRF52833 boards read switches through PCA9555 I²C expanders. One small scanner hides that wiring; RMK owns debounce, key behavior, persistent bonds and transports. Three role-specific builds avoid compiling receiver behavior into the right half. We have not adopted an unconventional cross-half processing protocol.

## Current evidence

- The full left RMK image passed stable expected USB enumeration, owner-confirmed basic left-key typing, independent recovery and exact application/tail readback. The protected gap is unchanged; the recorded initialization affects only approved settings storage.
- The full right RMK image passed independent recovery and exact application/tail readback. The protected gap is unchanged. Independent review decoded valid schema and peer-address records in the approved settings region.
- With the right USB cable unplugged, the owner typed `hjkl` using only the right half. Read-only inventory confirmed the full left USB device was present and the right USB device absent. The owner also held left Shift while typing right-side `HJKL`, confirming that cross-half modifier path. These establish basic split input and one modifier combination; they are not latency, loss-free, exhaustive mapping or authenticated-bonding acceptance.
- Installed firmware roles: full RMK left USB/Bluetooth coordinator, right BLE split peripheral, and RMK receiver. The right intentionally has no runtime USB keyboard or CDC interface. Both halves share the left's keymap. The left runs the owner-tested 400 Hz active-high lighting image with tap-and-hold controls; the right has not received a lighting image.
- The pinned build harness passes host safety/scanner checks and all six role/keymap cross-builds in the explicit `reclaimed-softdevice` layout. Cross-builds do not establish hardware behavior. The separate factory-preserving left build still exceeds its protected linker budget.
- The original factory backups and previous diagnostic recovery/restoration evidence remain private. Readable UF2 coverage excludes the MBR, bootloader and UICR; it is not a complete-chip backup. Full migration images require separate device-specific approval and guards.
- Direct Bluetooth pairing, saved reconnection and receiver-dependent typing passed owner-assisted macOS checks. Receiver application readback and working-app update entry passed; independent recovery from a broken receiver application is not proven. Exhaustive profile switching, battery calibration, both-half lighting and persistence, physical mode switches and Windows/Linux behavior remain pending. See [split architecture](docs/research/split-architecture.md), [receiver results](docs/research/receiver-rmk-plan.md) and [transport reliability](docs/research/transport-reliability.md).

“No missed keystrokes or input lag” remains an acceptance requirement. RMK currently has six-key ordinary rollover, and split disconnect/reconnect delivery needs deliberate testing. Source compilation and mock tests do not establish physical latency or loss-free operation.

Both-half backlight synchronization and persistence, indicators, physical mode-switch handling, factory web configuration and optional numpad support are pending. Both battery estimates are implemented but uncalibrated. This is not factory feature parity.

## Build, research and recovery

- [Read-only macOS companion prototype](desktop/README.md)
- [Guided GPUI/gpuikit installer proposal](docs/research/guided-installer-app.md)
- [Runtime/HAL observations and exact factory restoration](docs/research/runtime-hal-trial.md)
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

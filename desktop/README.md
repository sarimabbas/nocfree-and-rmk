# NocFree Companion

A native macOS application using [GPUI Kit](https://gpui-kit.com/) 0.7.0. The sidebar follows the owner’s flow sketches: contextual firmware tasks, Backup firmware and Recovery mode, with keyboard observations and firmware in a persistent status bar. The same workspace presents each physical step through GPUI Kit Stepper and Spinner components. Component selection uses illustrated cards. System appearance chooses the light or dark theme automatically. Recovery mode is a first-class utility; development trial flows stay out of navigation.

## What works

- Read-only discovery of supported normal-mode keyboard identities and recovery drives.
- Guided firmware copies with automatic saving after fresh identity and readback checks.
- A shared `Journey` state machine over `Session`’s device, archive and return checks. Observed state advances the guide; pause, resume and retry retain saved archives and replace stale connection/timing evidence.
- Separate producer-owned battery and split-connection status, refreshed through read-only Vial custom HID queries or legacy Rynk getters. Charging metadata stays intact; unknown charge state stays unknown. Disconnected, failed, replaced or expired observations are not presented as current readings.

The guide does not write firmware. Install, update and factory restoration require the reviewed transfer service and compatible release/restore metadata that have not yet been integrated. The app never claims that an unavailable release is current. Factory firmware is not bundled.

Backup plans include only supported, identified components. New right-half firmware exposes a local USB management interface without a second keyboard HID device. A Bluetooth connection is not evidence of USB backup support; the full role identity and runtime DFU descriptor are required for automatic entry.

Vial battery reporting uses one versioned, read-only VIA custom-value query through the native raw-HID collection (usage FF60/61). Companion prefers a unique direct left USB connection, otherwise a unique receiver; keeping both connected requires no extra step. The same existing receiver relay returns the left and right snapshots. Old firmware without this getter, malformed replies and changed connections remain Unknown with an explanation behind Details; invalid Vial replies never trigger a Rynk fallback. Legacy Rynk firmware retains its read-only getters. Bluetooth battery reporting is provided by firmware to the host OS independently.

HIDAPI 2.6.7 uses the native HID backend and shared access on macOS, leaving keyboard HID independent. A native synchronous report write itself has no application-controlled timeout. Companion stops waiting after five seconds, clears unavailable readings, continues device discovery, and accepts no late result; a single-flight guard prevents accumulating workers if the OS call stalls. Reply reads use a one-second native timeout, and a delayed query is checked before submission. Polling is every 30 seconds. Neither host observation time nor a successful getter is a producer sample timestamp. macOS/Windows/Linux HID hardware acceptance remains separate from host tests and the macOS build.

Battery percentages are uncalibrated voltage estimates. Retaining charging metadata and rejecting stale host results fixes host reporting errors, not the unresolved low percentage or hardware charging behavior. See [battery diagnosis](../docs/research/battery-reporting.md).

## Run and verify

```sh
./desktop/build-macos.sh
open 'dist/NocFree Companion.app'
```

```sh
cd desktop
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
```

The desktop pins Rust separately from firmware. Embedded board drawings show orientation, not device status. The independently drawn ampersand icon avoids using the manufacturer’s logo. The current bundle is locally signed development software; notarization, portable distribution and Windows/Linux device acceptance remain outstanding.

## Copies and interrupted work

Copies are saved privately under `~/Library/Application Support/NocFree Companion/`. Archives include exposed CURRENT.UF2 bytes, metadata and a SHA-256/coverage manifest. Directories are 0700 and files 0600; saves are flushed and finalized atomically. An uncertain save stops automatic work. Retry retains completed copies while requiring fresh device observations.

Pausing stops the guide; it does not erase a copy or change the keyboard’s firmware. Returning to Backup firmware offers Resume. A saved archive is not a full-chip backup or automatically a restore-compatible image: exposed readback excludes MBR, filesystem, bootloader and UICR. Image guards and durable update-intent journals remain separate from the copy flow; interrupted transfers must be reconciled, never replayed automatically.

## Recovery mode

Additional utilities → Recovery mode uses a shared, illustrated state machine: choose a component, identify its supported firmware, follow the appropriate entry procedure, and wait for the correlated recovery drive. Cancel invalidates the attempt; retry starts a new explicit attempt. The physical step has no reading-time deadline. USB dispatch and subsequent drive observation retain bounded deadlines and never retry automatically.

Recognized factory halves retain their reconnect/correlation check and factory key-hold instructions. The receiver guide requires a paired factory left and the documented Dongle DFU key mapping. Compatible normal RMK firmware needs only a USB connection: after the user selects a component, Companion sends one local standard DFU_DETACH to that component’s observed FE/01/01 interface. It never sends a forwarded Vial or Rynk bootloader command. The selected DeviceId, physical USB location, role-specific product identity and unique WILL_DETACH functional descriptor are rechecked before dispatch. Left uses `4c4b:4643` / `NocFree RMK`, right uses `4c4b:4671` / `NocFree RMK Right`, and receiver uses `4c4b:4644` / `NocFree RMK Receiver`. Legacy identities without a compatible DFU interface receive manual guidance rather than inferred support. Older firmware can still impose its own runtime deadline; the new firmware removes that deadline.

All routes require the recovery drive to follow the selected component on the same USB connection and expose reviewed factory bootloader metadata. Backup composes this recovery worker and illustrated state machine, then saves after a fresh readback check. Firmware installation services are not yet integrated. Factory route changes and the new normal-runtime contract are covered by host fixtures; hardware acceptance of the new normal-runtime images remains outstanding. Historical startup-stage tests do not establish acceptance of this replacement. Factory bootloaders remain unchanged.

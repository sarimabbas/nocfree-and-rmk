# NocFree Companion

A native macOS GPUI application using GPUKit. The sidebar follows the owner’s flow sketches: contextual firmware tasks, Backup firmware, Keyboard status, and the observed firmware state. The same workspace presents each physical step. Recovery mode is a first-class utility; development trial flows stay out of navigation.

## What works

- Read-only discovery of supported normal-mode keyboard identities and recovery drives.
- Guided firmware copies with automatic saving after fresh identity and readback checks.
- A shared `Journey` state machine over `Session`’s device, archive and return checks. Observed state advances the guide; pause, resume and retry retain saved archives and replace stale connection/timing evidence.
- Separate producer-owned battery and split-connection status, refreshed through bounded GET-only Rynk USB requests. Charging metadata stays intact; unknown charge state stays unknown. Disconnected, failed, replaced or expired observations are not presented as current readings.

The guide does not write firmware. Install, update and factory restoration require the reviewed transfer service and compatible release/restore metadata that have not yet been integrated. The app never claims that an unavailable release is current. Factory firmware is not bundled.

Backup plans include only supported, identified components. The full RMK right peripheral does not have the earlier diagnostic’s normal USB identity; a Bluetooth connection is not evidence that the USB backup route supports it. RMK’s product name alone does not identify its recovery procedure.

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

Recognized factory halves receive their factory key-hold instructions. The receiver guide requires a paired factory left and the documented Dongle DFU key mapping. Compatible RMK startup interfaces receive one scoped app-recovery request; recognized older RMK halves retain a manual recovery procedure rather than guessing support from the product name. The existing RMK receiver’s normal identity remains unsupported by this entry guide; its prepared startup-stage update is separate work. All routes require the recovery drive to follow the selected component on the same USB connection. Backup shares the guide presentation and verified procedure; firmware installation services are not yet integrated. Factory route changes are covered by host fixtures, not a new factory hardware trial.

The right no-chord startup image has passed owner-device app recovery, exact readback and reported normal typing. The corresponding left and receiver candidates remain uninstalled. Factory bootloaders remain unchanged. Host fixtures, cross-builds, native UI inspection and hardware acceptance are separate results.

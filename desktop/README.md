# NocFree Companion

A native macOS GPUI application using GPUKit. The sidebar follows the owner’s flow sketches: contextual firmware tasks, Backup firmware, Keyboard status, and the observed firmware state. The same workspace presents each physical step. Development tools are hidden unless their experiment is enabled.

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

## Development recovery

`NOCFREE_USB_RESCUE_EXPERIMENT=1` exposes Developer tools → Recover a device. Select a component and follow its power-cycle guide. The app sends one scoped DFU request to the matching experimental startup USB interface and reports success only after observing the recovery drive at the same USB location. This requests a reset, never copies firmware, and requires a supporting installed image.

The separate private startup trial remains a developer tool, not a newcomer installer. It has no heartbeat dependency. Host fixtures, cross-builds, native UI inspection, physical recovery and firmware hardware acceptance are separate results.

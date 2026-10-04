# Companion recovery protocol research

Research only, 2026-10-03. No firmware or device changes. Sources inspected against our pinned RMK `b41bd7caa7de67a47f6e7380519b730630f16152`, whose upstream base is `9607aedf`, and current upstream documentation.

## Conventional options

| Route | Actual behavior | Suitability |
| --- | --- | --- |
| Rynk `BootloaderJump`, command `0x0004` | Host service calls `rmk::boot::jump_to_bootloader()`. Requires an unlocked physical-presence session. | Existing LEFT host protocol, but current default `LockConfig` has no unlock keys and is locked. Receiver forwards host commands to LEFT. Does not give RIGHT local USB management. Rynk is documented experimental. |
| VIA/Vial `BootloaderJump`, command `0x0b` | 32-byte HID host report reaches the VIA service, which calls the same boot function. | Conventional supported configurator route, but our LEFT selects Rynk, not Vial. Switching protocols solely for recovery is unnecessary. Pinned VIA bootloader arm itself has no unlock test; do not conflate it with Rynk's gate. |
| USB DFU **runtime DETACH** | USB class request tells the running application to reset into its existing bootloader. RMK already has this specifically for dongles. | Best common Companion primitive: no configuration-protocol switch and no firmware-download implementation in RMK. Existing implementation accepts DETACH only during its first 30 seconds of uptime. |
| `dfu_nrf` + `rmk-boot` | Receives firmware downloads and uses an embassy-boot partition scheme. | Separate full update architecture; not needed to open the existing factory UF2 drive. Would change the bootloader/flash layout, so exclude from this plan. |

All first three routes converge on `rmk/src/boot.rs`: with `adafruit_bl`, write `POWER.GPREGRET = 0x57`, then software reset. This is the same entry request already used successfully with these factory bootloaders; bootloader compatibility is board evidence, not a generic guarantee for every Nordic board.

These command handlers and the dongle runtime DETACH are upstream behavior: local `git diff 9607aedf..b41bd7c` for `boot.rs`, `usb/dfu_detach.rs`, `host/rynk`, and `host/via` is empty. Our recovery startup window, pre-init hook, role integration and Companion watcher are board-specific work.

## Current role wiring

`firmware/Cargo.toml` enables `split,dongle,adafruit_bl,rynk` for LEFT; `split` only for RIGHT; `dongle,adafruit_bl` for receiver. `firmware/src/main.rs` constructs normal USB only under `cfg(not(feature = "right"))`.

- LEFT serves Rynk locally and also exposes the dongle-feature runtime DFU interface. Its Rynk bootloader command is currently permanently locked by default configuration. Its DFU DETACH expires 30 seconds after MCU startup, not 30 seconds after a battery-powered USB reconnect.
- Receiver normal USB uses `with_dongle_router`: Rynk/Vial commands target the keyboard through the radio. The separate runtime DETACH targets the receiver itself, locally.
- RIGHT normal operation is a BLE split peripheral and has no running USB management task. Its successful Companion recovery test used the separate startup USB stage. Removing that stage requires a small normal-runtime, management-only USB path for RIGHT; it should not advertise independent keyboard HID or duplicate key behavior.

## Recommended agreed design

The owner chose **Vial compatibility** and authorized implementation. Replace the LEFT's Rynk selection with RMK's Vial service; keep recovery independent of either configurator protocol so the receiver is addressed locally. Builds and hardware acceptance for this change are pending.

One user procedure: open Companion's Recovery page and connect the selected part. Companion uses **local USB DFU runtime DETACH** for LEFT, RIGHT and receiver. Extend/reuse RMK's existing handler at the transport seam, with deliberate availability beyond the existing 30-second uptime gate. Do not invent a new firmware-download protocol or use a forwarded host command that accidentally resets the wrong part.

For a stalled firmware, enable the hardware watchdog early; use RMK's existing watchdog runner to feed it during normal execution. A minimal pre-init hook converts a watchdog reset into the existing factory recovery request before RMK initializes. It does not scan keys, enumerate USB, or wait for Companion. The factory UF2 bootloader then supplies USB recovery. This replaces the separate startup USB stage, not the factory bootloader.

Correction: pinned/upstream RMK lists `watchdog` among its default features. Our dependency uses `default-features = false`, so our ordinary build did not enable it automatically. Existing RMK support supplies the watchdog abstraction/runner; early arming and watchdog-reset-to-factory-recovery are our board integration.

The deliberate-hang watchdog proof is narrower than integrated acceptance: each part still needs ordinary Companion DETACH, induced crash recovery, normal startup, settings preservation, disconnect/release recovery, simultaneous split input and wake/typing tests. The hook cannot rescue corrupted vectors, corrupted hook code, or a fault before watchdog arming. A task that remains alive and feeds despite another task failing is not automatically detected.

## Primary sources

- [RMK Rynk documentation](https://rmk.rs/main/docs/features/rynk)
- [Rynk command and lock protocol](https://rmk.rs/main/docs/development/rynk_protocol)
- [Upstream runtime DFU DETACH implementation](https://github.com/rmk-rs/rmk/blob/main/rmk/src/usb/dfu_detach.rs)
- [Upstream bootloader entry](https://github.com/rmk-rs/rmk/blob/main/rmk/src/boot.rs)
- [Upstream VIA bootloader handler](https://github.com/rmk-rs/rmk/blob/main/rmk/src/host/via/mod.rs)
- [Full DFU / rmk-boot architecture](https://github.com/rmk-rs/rmk-boot)
- [RMK v0.9 migration and watchdog defaults](https://rmk.rs/main/docs/migration/v08_v09)

Exact local source references under the pinned checkout: `rmk/src/host/rynk/handlers/system.rs:90`, `rmk/src/host/rynk/mod.rs:68`, `rmk/src/host/via/mod.rs:145`, `rmk/src/config/lock.rs:1`, `rmk/src/usb/dfu_detach.rs:16`, `rmk/src/usb/mod.rs:379`, `rmk/src/usb/mod.rs:437`, `rmk/src/boot.rs:1`, `rmk/Cargo.toml:114`, `rynk/src/api.rs:92`, `rmk-types/src/protocol/vial.rs:25`.

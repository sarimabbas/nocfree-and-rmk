# Transport reliability review

Reviewed 2026-10-01 against pinned RMK `9607aedf343b17dd6b27307583ae80c4f728fbbd` and the current board scanner. This is source inspection, not a hardware result. No transport or scanner behavior was changed, and no device commands were sent.

## Findings

| Boundary | Existing behavior | Consequence / remaining gate |
| --- | --- | --- |
| PCA9555 → scanner | All three reads must succeed before returning a snapshot. An error leaves debounced state unchanged. | Avoids fabricated releases from a partial scan. A sustained bus failure can leave a legitimately held key stale; release is recovered only after successful sampling. |
| Scanner → RMK | Each debounced transition uses awaited `publish_event_async`. State changes before publication; scanning waits while the event channel is full. | Backpressure avoids intentionally dropping the published transition. It can delay later scans; a tap entirely between samples is inherently unobservable. This is not a hardware no-loss proof. |
| Right → left | The peripheral consumes a key event, then calls `split_driver.write(&e).await.ok()`. BLE notification errors become `BleError(1)`. | A failed write consumes that transition without retry or an explicit recovery action. A failed release can leave central state stale. |
| Split reconnect | Central records connectivity; protocol carries individual key transitions, not a held-key snapshot. No keyboard handler for `PeripheralConnectedEvent` was found. | A key released while disconnected has no demonstrated reconciliation route. Reconnecting the radio alone does not release it at the central. |
| Left → host BLE / receiver BLE | BLE writer consumes a queued HID report, logs write failure and reads the next report. | The failed report is not retried by this task. A later full keyboard report may reconcile state, but no later input is guaranteed. |
| Receiver → USB | Receiver explicitly sends empty keyboard, mouse, media and system reports when its relay connection ends. | There is already a deliberate host release path for this boundary. Delivery still depends on the USB writer and host state; do not duplicate it in board code. |
| USB suspend | USB writer consumes a pending report, signals remote wakeup when suspended, then `continue`s. A disabled-endpoint error takes a separate path with one retry after 500 ms. | The proactive suspend path discards its wake-triggering report. A short tap can be missed; a release may remain stale until another report. Disabling split sleep does not address host USB suspend. |
| Host / profile change | Report routing drops reports when no transport is selected, and stops pending enqueue when transport changes. Old transport queues are cleared with an all-up keyboard report. | Outage keystrokes are not promised replay. Explicitly test old-host release and new-host held-state behavior. |

Primary sources at the exact pin:

- [Peripheral event consumption and write](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/split/peripheral.rs#L107-L129), [ignored write result](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/split/peripheral.rs#L265-L277).
- [BLE peripheral notification error handling](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/split/ble/peripheral.rs#L161-L174).
- [Split protocol](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/split/mod.rs#L38-L85), [central connectivity lifecycle](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/split/ble/central.rs#L216-L232), [incoming key publication](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/split/driver.rs#L281-L301).
- [Awaited event publication](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/event/mod.rs#L141-L160), [HID queue routing](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/channel.rs#L49-L82).
- [BLE HID writer](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/ble/mod.rs#L874-L882), [USB HID writer](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/usb/mod.rs#L74-L102).
- [Receiver disconnect cleanup](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/dongle/mod.rs#L326-L347), [empty reports](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/dongle/mod.rs#L777-L791).
- Local board seams: `crates/nocfree-input/src/lib.rs` and `firmware/src/scanner.rs`.

Successful notification submission is not application-level acknowledgment. These findings establish explicit error/reconnect gaps in the reviewed code; they do not establish the frequency of failures on this board or that every BLE radio packet loss loses input. BLE handles ordinary link-layer retransmission below these tasks.

## Minimal fixes belong upstream

Keep the established topology and RMK ownership. Do not add a parallel NocFree transport, independent keymap engine, or indefinite replay queue.

1. **Preserve the USB wake report.** Keep the consumed report pending while requesting wake; send it after the endpoint resumes, or explicitly end the session if the host disconnects. Preserve press/release order, yield while waiting, and cancel pending reports on transport changes. Replace the unconditional suspend `continue`; do not spin or hold the entire scanner hostage forever.
2. **Make split disconnect release explicit.** RMK should track accepted physical key positions per peripheral. When that session ends, publish releases for those positions through the normal keyboard path, keeping central-local held keys intact. Lifecycle cleanup must run even when `select3` cancels the manager. Reset associated layer/modifier/tap-hold state via normal release processing; sending an empty HID report alone leaves internal key state inconsistent.
3. **Define split rejoin semantics.** Prefer a framework-owned current debounced-state snapshot at session start so keys still held can be re-established and keys released during the outage remain up. A protocol extension affects both halves and requires compatible versions. A simpler initial policy is release everything from the disconnected half and require fresh physical presses, but document that held keys do not resume automatically. Do not replay completed outage taps later into a different application.
4. **Handle failed split/HID writes.** Distinguish retryable resource pressure from a dead session. Retain ordered transitions while the live session can recover, with bounded waiting and disconnect cancellation. If delivery becomes uncertain, close the session and reconcile key state instead of logging and continuing. A blind retry after uncertain submission can duplicate a transition; tests must cover that ambiguity.

Items 2–4 are proposals, not implemented fixes or claims of upstream agreement. Land regression tests in an isolated upstream patch before adopting a new commit. Recheck code size for all three roles. A snapshot repairs held state; it cannot recover a tap that began and ended entirely during a disconnected interval.

## Repeatable software gate

Existing host checks exercise electrical snapshots and image guards, not the runtime split tasks. Run `scripts/check.sh --reclaimed-softdevice` before and after transport/scanner changes; it runs host tests, formatting and cross-builds left/right/receiver plus the recovery probe. The default factory-preserving left flash budget is a separate known failing gate, not a transport regression.

An upstream regression harness should drive the actual writer/manager futures with fake connection, endpoint and clock boundaries. Assert ordered reports/events, session termination and final held state; a separate toy model that mirrors intended behavior does not verify RMK.

| Injected sequence | Required assertion |
| --- | --- |
| USB suspended → press → release → resume | Both reports delivered once, in order; first key is not consumed only to wake. |
| USB suspended → transport change or unplug | No stale report replay to the next host; no infinite pending task. |
| Right modifier press → disconnect → release offline → reconnect | Central modifier becomes up; central-local modifier remains held. |
| Right key remains down across disconnect/reconnect | Matches the explicitly selected rejoin policy; no duplicate action or permanent stale state. |
| Split write fails before enqueue, then succeeds | Transition preserved once, ordering maintained. |
| Split failure after potentially accepted send | Session reconciliation avoids permanent stuck state and does not blindly duplicate actions. |
| BLE HID release write fails with no subsequent input | Session terminates/reconciles or retry succeeds; failure is not silently treated as delivery. |
| Report/event queue saturation with simultaneous halves | Bounded progress, correct ordering, no fabricated release; measure scan suspension rather than calling backpressure lossless sampling. |
| Receiver link ends while keyboard/media held | Existing empty reports emitted; validate their USB delivery and reconnect state. |
| I2C partial-read error during release, then bus recovers | No partial state publication; genuine release eventually debounces normally. |

Use fixed schedules/seeds and record the dependency pin. Run upstream tests on a host with stubbed platform services if needed, then cross-build the exact proposed dependency for all roles. No such new runtime harness was executed in this source review.

## Hardware gate remains required

Follow `docs/acceptance.md`: independently measure USB, host Bluetooth and receiver paths with a shared-clock input actuator and host report capture; at least 100,000 transitions per mode, modifier/layer disconnects, simultaneous halves, congestion, low battery and host sleep/wake. Record raw traces, count errors, and report median/p95/p99/max latency. The numerical latency budget still needs agreement.

Test USB suspend separately from split sleep. Repeat the exact first short tap after host wake on all three desktop OSes. Test receiver reconnection with an already-held right modifier as well as keys pressed on the central. Safe hardware trials still require each role's image guard, bootloader evidence and proven recovery route; this review authorizes no flashing.

The safest next hardware milestone is a right-only USB input probe retaining the tested updater and recovery route. It can establish the electrical map, debouncing and USB wake behavior without depending on split reconnect semantics. It remains a diagnostic milestone; full USB-central/Bluetooth/receiver modes cannot be called reliable until the source gaps and their regression/hardware gates are resolved.

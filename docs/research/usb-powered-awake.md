# USB power and split sleep

Each half uses its own USB VBUS presence to override RMK sleep. Mode selection,
USB report routing, and USB suspend do not determine whether a half is powered.
A USB-powered half remains awake and retries advertising after a timeout. On
battery, the right follows the latest parent sleep request; unplugging restores
that request. The left idle timeout remains disabled in `keyboard.toml`.

## Source evidence

RMK `usb/mod.rs` receives Embassy USB power detection/removal through
`Handler::enabled(bool)`. The opt-in `usb_power_awake` policy keeps this state
separate from `ConnectionStatus`, which peripheral advertising resets. USB bus
suspend retains VBUS and standard USB remote-wake behavior.

RMK `ble/mod.rs` and `split/ble/peripheral.rs` previously parked advertising
after its timeout until a key event. Powered retries now use a 500 ms backoff
without holding undrained input subscribers. `split/peripheral.rs` applies the
same local-power override to parent sleep messages and backlight snapshots.
Key scanning, debounce, and report handling remain in their existing tasks.

## Validation boundary

Host tests cover power attachment while sleeping, powered sleep suppression,
unplug restoration, advertising-wait release on attachment, and charge-only /
suspended USB state. They do not establish board current or hardware wake latency.
Cross-build and image guards must pass for left, right, and dongle before an
application-only half update. The dongle does not enable this half sleep policy.

Hardware acceptance separately checks idle first-key delivery while each half
is USB powered, battery operation after unplug, simultaneous cross-half input,
modifier release, reconnect, and Companion recovery. No new battery idle timeout
or connection-parameter policy is introduced by this change.

Software checkpoint (2026-10-04): RMK `c1480a3026735b01387ed8ff8e40e11d7faf6c1a`;
293 host tests with the policy enabled and 282 with it disabled passed under
nextest. The board input harness and 78 Python safety tests passed. All three
roles cross-built and passed their image guards. Hardware acceptance is pending.

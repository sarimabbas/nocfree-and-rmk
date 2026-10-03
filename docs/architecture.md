# Architecture

The project has two real seams: the board's expander inputs into RMK, and firmware files into the image validator. RMK remains the deep module for scanning event consumption, debounce, key actions, persistent configuration, USB HID, Bluetooth HID, split communication and receiver transport. We avoid wrapping each framework concept in a new interface.

The board scanner owns I²C addresses, initialization, polarity, populated-key masks, event ordering and handling of read failures. A failed sample must not become a fabricated all-released state. Host tests exercise the scanner with a mock bus at that same interface. Physical layout belongs with the scanner/keymap mapping, rather than scattered through transport tasks.

Three firmware roles share board knowledge: left central, right peripheral, and USB receiver. The left owns the complete keymap and host selection; the right forwards physical events, and the receiver relays HID. Standard BLE links are the first transport because RMK already implements them. Recreating proprietary ESB would add a second scheduler, radio driver and reconnect protocol before we have validated the existing ones.

Firmware dependencies are pinned, the lockfile is committed, and every role must cross-build in the same harness. Image validation independently rejects writes outside the application interval, wrong MCU family, malformed blocks and invalid vectors. It proves structural safety only; device-specific handoff and recovery remain separate hardware gates.

The first version chooses information hiding and few moving parts over a promise of feature parity. Any missing factory behavior is recorded explicitly. Zero input loss and bounded latency require measured acceptance results, not an architectural assertion.

Optional backlighting extends RMK itself: the central resolves light actions,
owns brightness and delayed persistence, and sends absolute snapshots over its
existing split link. The board main supplies only the vendor-published pin,
PWM configuration and an explicit polarity. A latest-value watch separates
key processing from output/storage/link work. There is no board-local lighting
key interpreter, second radio protocol or animation system. See
[implementation](research/backlight-implementation.md).

# Split communication architecture

Researched 2026-10-02. Recommendation for the requested direct USB, direct Bluetooth and USB-dongle modes: **keep the left half as the keyboard central, the right as its BLE split peripheral, and use RMK's dongle as a HID relay**. This matches the repository's existing architecture and uses upstream transport behavior. It is a design recommendation, not hardware acceptance; full RMK left USB startup, basic typing and independent recovery have passed; the split/dongle links have not yet been validated on these devices.

## Roles and paths

```text
Right switches → RMK split peripheral ── BLE ──→ Left RMK keyboard central
                                                    ↑ left switches
                                                    │ one keymap/state
                                                    ├─ USB → computer
                                                    ├─ BLE HID → computer
                                                    └─ BLE HID → RMK dongle ── USB → computer
```

The right sends physical key transitions. The left combines both halves and owns modifiers, layers, shortcuts, media behavior and host selection. For example, left Fn + right F8 reaches one keymap and becomes play/pause. The dongle forwards the central's resulting HID reports, rather than running a competing keyboard keymap. RMK documents this split topology and its complete three-board dongle example. [RMK split documentation](https://rmk.rs/main/docs/features/split_keyboard), [RMK dongle documentation](https://rmk.rs/main/docs/features/dongle), [pinned three-board example](https://github.com/rmk-rs/rmk/tree/9607aedf343b17dd6b27307583ae80c4f728fbbd/examples/use_rust/nrf_dongle).

“Keyboard central” names the role owning keyboard state. It should not be confused with every BLE link's radio role: the left is central toward the right, while the host/dongle connects to the left's HID service. The dongle is a BLE central for that host-facing connection while remaining a relay in the keyboard architecture. [Pinned BLE transport](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/ble/mod.rs), [pinned dongle](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/dongle/mod.rs).

Local configuration already selects one peripheral, `connection = "ble"`, left/right coordinate offsets and a disabled central sleep timeout. Cargo enables left `split` + `dongle`, right `split`, receiver `dongle`, with RMK pinned to `9607aedf343b17dd6b27307583ae80c4f728fbbd` and `nrf52833_ble`. These are configured intent, not evidence that all modes run. [keyboard.toml](../../firmware/keyboard.toml), [Cargo.toml](../../firmware/Cargo.toml), [existing architecture](../architecture.md).

## Alternatives and tradeoffs

| Design | Advantage | Cost for this project |
| --- | --- | --- |
| Fixed left central, BLE right, relay dongle | One keymap and the same half roles across all three output modes; follows RMK's example | Right-to-dongle input crosses two BLE links; left carries central power/work |
| Two independent USB keyboards | Each half can type over its own cable | Firmware layers/Fn/combinations have no shared state; host modifier aggregation is insufficient for shared firmware behavior. Requires a separate merge system or reduced functionality |
| Dongle is keyboard central, both halves peripherals | One BLE hop from either half to the dongle; central work/power moves off the halves | The conventional ZMK topology requires the dongle. Its documented undongling path changes role builds/bonds; it does not provide this project's requested direct-USB/direct-BLE modes with the same half roles |
| Wired serial between halves | Can eliminate inter-half radio scheduling when hardware supports it | NocFree wiring/pin access for a safe serial link is unverified; a USB socket does not establish UART capability or an existing inter-half cable path |

The independent-USB limitation is an architectural inference: a firmware action on one controller cannot access the other controller's private layer state without a communication mechanism. The existing local typing diagnostics are useful component tests; they are not a finished split keyboard.

ZMK's official dongle guide makes the dongle a keyless split central and changes both keyboard parts to peripherals. It describes battery advantages and the inability of those peripherals to operate without the dongle. This is a reasonable choice for a dongle-only product, but introduces a role/topology change for the requested three-mode product. Do not copy ZMK's quoted latency numbers as measurements of NocFree or RMK. [ZMK dongle guide](https://zmk.dev/docs/hardware-integration/dongle), [ZMK split roles](https://zmk.dev/docs/features/split-keyboards).

RMK supports serial split through asynchronous read/write implementations, but no verified NocFree inter-half serial wiring is currently established. Do not connect the two USB device ports together or infer serial pins from the MCU's theoretical capabilities. [RMK serial split documentation](https://rmk.rs/main/docs/features/split_keyboard#communication).

## Latency and reliability gates

| Host output | Left key's BLE links | Right key's BLE links |
| --- | ---: | ---: |
| Left USB cable | 0 | 1: right → left |
| Direct host Bluetooth | 1: left → host | 2: right → left → host |
| USB dongle | 1: left → dongle | 2: right → left → dongle |

Hop counts describe transport scheduling, not measured latency. The pinned source requests 7.5 ms intervals for awake split and dongle links, with split peripheral latency 30 and dongle latency 0. Idle-event skipping is not a fixed extra delay on every active key. Radio retries, interference, scanning/debounce, connection scheduling, USB and host processing still contribute. Zero delay and losslessness under arbitrary disconnection cannot be guaranteed. [Split parameters](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/split/ble/central.rs#L233-L245), [dongle parameters](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/dongle/mod.rs#L250-L261).

Pinned RMK awaits central publication of key events, preserving backpressure there. However, the peripheral consumes a transition and ignores a subsequent write error; this source does not establish replay of that transition or complete held-key resynchronization after reconnect. That is a reliability caveat requiring testing, not permission to recreate the transport in board code. [Peripheral send](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/split/peripheral.rs#L269-L277), [central publication](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/split/driver.rs#L282-L301).

First establish full left USB operation, then validate right-to-left input with that stable USB output, followed by direct host BLE, then the relay dongle. In each mode require simultaneous cross-half input, cross-half modifiers/layers, disconnect while holding keys, reconnect with held keys, release recovery, output switching, host suspend/resume and measured wake/typing latency. Keep subrating off and the configured central sleep timeout at zero until wake acceptance passes. Battery percentages from both halves should be checked independently against measurements and host discovery; native OS UI display of both batteries is a separate question. Receiver hardware/recovery/flash-fit must be verified before claiming the original dongle supports RMK.

No transport, scanner, firmware, device or app state was changed by this research.

# NocFree AND + RMK

An experimental Rust firmware port for the NocFree AND split keyboard. This project is under development; no firmware has yet been validated on a physical board.

The target is left-half USB or Bluetooth HID, right-to-left Bluetooth split communication, battery reporting, and a separately flashed RMK Bluetooth-to-USB receiver. The factory receiver uses a proprietary protocol and cannot be presumed compatible. Standard HID is intended to work across macOS, Windows, and Linux; each host still needs acceptance testing.

The board uses nRF52833 controllers and PCA9555 I²C key expanders. The custom scanner hides that wiring; RMK owns key processing and transports. See [hardware research](docs/research/hardware.md), [RMK research](docs/research/rmk.md), and the forthcoming build and recovery instructions.

“No missed keystrokes or input lag” is a test requirement, not a claim. Radio interference, disconnects, sleep, and host scheduling prevent an absolute guarantee. The first release must be measured under a documented hardware workload before daily use.

Factory firmware and recovery executables are intentionally kept outside this public repository. Preserve your original recovery archive locally.

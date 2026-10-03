# Simple controls: source notes and procedure proposal

Researched 2026-10-03. Documentation/design only; no firmware or device writes. The procedures below describe a proposed end state, not instructions that the installed development firmware already supports.

## Official sources checked

- [NocFree AND manual](https://www.nocfree.com/pages/nocfree-and-manual): the left selector chooses Bluetooth or receiver mode; the right must be on. Bluetooth supports three channels, with short channel selection and long Fn+1/2/3 pairing. Receiver mode reconnects automatically after the receiver is plugged in and its switch position selected. Wired use connects the left by USB. The manual lists update order as receiver, left, right.
- [Windows Firmware Care guide, v2.4.5 Beta](https://cdn.shopify.com/s/files/1/0668/2709/2210/files/NocFree_Windows_Firmware_Guide_EN_v2.pdf?v=1789517485): retrieved with `curl` after the web reader failed, then extracted with `pdftotext`. The guide specifies left, right, receiver order (page 1), one USB device at a time, automatic detection/update, and disconnecting each completed device before proceeding. It requires an explicit completion message rather than interpreting progress output (page 15). Interrupted updates may expose a UF2 drive; its emergency procedure selects exactly the file matching the device and keyboard layout (pages 13–15).

These official documents disagree about update order. Treat the PDF as its stated version-specific workflow, not a universal order or evidence that our RMK roles can use the same package. The PDF contains no recovery-key shortcut specification. Font warnings occurred during extraction; text was legible, but screenshots were not visually audited. Nothing in either source establishes this unit's bootloader behavior or recovery path.

## Proposed everyday procedures

The physical selector is the only typing-mode control. Right bottom means on; right top means off. Left top means receiver, middle means wired, bottom means Bluetooth. A USB charging cable does not change the wireless destination. No USB fallback while a wireless position is selected. If a destination is unavailable, report that state instead of silently selecting another destination.

1. Wired: turn the right on, put the left in the middle, connect the left USB cable. With no left cable, the middle position is off subject to actual hardware power behavior.
2. Receiver: turn the right on, plug the saved RMK receiver into the computer, put the left at the top. This reconnects the existing pairing; it does not start pairing. A factory receiver is not assumed compatible with RMK.
3. Bluetooth: turn the right on, put the left at the bottom. It reconnects the last selected slot. Tap Fn+1, Fn+2, or Fn+3 to select another saved computer.
4. First Bluetooth pairing: while the left is at the bottom, hold Fn plus the desired slot number for five seconds, release, then select the keyboard in the computer's Bluetooth settings. A short tap never clears a saved pairing. A cancelled hold performs no pairing action. This hold duration is our proposal; the manual says long press without defining a duration in its text.
5. New receiver pairing: use a guided Companion pairing action that names the receiver and explains any replacement of an existing pairing. Ordinary receiver selection needs no key combination. The UI and RMK capability still need implementation/verification; this is not a present-day usable instruction.

Provide explicit connecting, connected, and pairing feedback where the hardware can support it. Preserve slot pairing records during ordinary switching. Mode changes must release held keys on the old destination. These are acceptance requirements, not verified observations.

## Make recovery intentional

Remove immediate Fn+Escape from the everyday keymap only after a replacement entry and independent recovery route are proven. The preferred update procedure is a deliberate Companion Update action, identification of the exact connected component, a reviewable update step, then explicit user initiation. Normal app startup, USB reconnects, charging, and selector changes must never initiate an update or recovery. The vendor PDF is useful inspiration for sequencing and plain completion feedback; its automatic flashing on detection should not become our ordinary Companion connection behavior.

Avoid assigning another ordinary Fn key as the default recovery shortcut merely to resemble factory firmware. A five-second hold with a connected cable could be a maintenance fallback if the application still runs, but it cannot rescue a broken application. No particular fallback chord or cable/reset sequence is justified here without verified board-specific bootloader evidence. Recovery must work independently of the application, and the two halves need independently verified routes. An app-only recovery button cannot meet that requirement by itself.

The existing `usb-recovery-first` development marker operates before RMK starts and conflicts with cold wired startup. Removing Fn+Escape, reading the mode selector, or rewriting instructions cannot cure that branch. Resolve it while preserving an independently proven recovery route before presenting the proposed everyday procedures as available. The user's successful exit from recovery establishes that one escape worked now; it does not establish a replacement entry route for a broken application.

## Limits and next implementation gate

Local `firmware/src/keymap.rs` currently maps Fn+Escape to the RMK bootloader action, Fn+B to connection preference, Fn+U to receiver selection/pairing, and Fn+0 to clearing the selected BLE bond. This is different from the proposal. The existing `mode-controls-contract.md` records the development marker and right-switch sensing uncertainty. Hardware support for right-off while USB supplies power remains unresolved.

Before publishing this as the user guide: verify selector sensing and right switch behavior, framework routing/profile/hold support, both recovery routes, then hardware-test mode changes, disconnected destinations, held-key release, simultaneous input, pairing, wake latency, and cold USB startup. Keep RMK responsible for key behavior and transport; no scanner-side duplicate pairing/key engine. Builds and host tests alone do not make these procedures safe or available on the device.

# Everyday mode and shortcut contract

Owner intent recorded 2026-10-03. This is the target behavior, not a claim about the installed firmware. No firmware changes or device writes accompanied this audit.

## Target

The left selector is authoritative. Top selects the RMK receiver, middle selects USB typing (no cable means no typing and battery power off where hardware provides it), bottom selects direct Bluetooth. USB must not override top/bottom or provide a silent fallback. A cable in wireless positions may supply charging and maintenance; it does not choose the typing destination. The right top position must mean off, bottom on, regardless of a charging cable, subject to whether hardware exposes a switch state to firmware. Both active halves continue using the existing split BLE link; the right does not become a second independent USB keyboard.

Preserve the factory user vocabulary: three Bluetooth hosts, short Fn+1/2/3 to select and long hold to pair; hold left Fn+4 for five seconds to pair the receiver; hold left Fn+5 for five seconds to enter that half's recovery; hold right Fn+0 for five seconds to enter the right's recovery. Ordinary selection should not clear bonds, pair, or enter recovery. Remove bring-up mode shortcuts Fn+B/Fn+U and immediate Fn+Escape after replacement recovery entry has been tested. Pairing is explicit and separate from switching to the saved receiver.

Sources: [factory manual](https://www.nocfree.com/pages/nocfree-and-manual), [factory troubleshooting](https://www.nocfree.com/pages/nocfree-and-troubleshooting), [factory update guide](https://www.nocfree.com/blogs/news/nocfree-firmware-update-guide). The factory radio protocol differs from RMK's receiver protocol; matching gestures does not establish stock receiver compatibility.

## Confirmed current differences

- `firmware/src/main.rs` starts USB and BLE on the left and does not read selector pins. `firmware/src/keymap.rs` assigns Fn+U to RMK receiver selection, Fn+B to preference toggling and Fn+Escape to the upstream bootloader action. RMK provides the actions; this port chose their physical key assignments.
- `firmware/keyboard.toml` currently enables five BLE profiles, with five selection keys. Pairing/clearing and receiver action IDs depend on the configured profile count; reducing it requires updating every related mapping, not merely changing the count.
- The pinned RMK connection decision permits USB fallback even with BLE preference. A strict selector contract therefore needs framework routing policy, not just setting preferred transport.
- Full RMK right has no normal USB runtime or local keyboard action engine. Its Fn+0 reaches the left combined keymap, currently clearing the selected host bond. Independent right recovery requires a deliberate framework-level peripheral entry path; do not accidentally boot the left instead.
- The optional `usb-recovery-first` marker was deliberately enabled for the guarded half trials. The installed bootloader recognizes it and stays in recovery on a cold USB-first launch. Battery-first startup avoids that branch. This safeguard conflicts with ordinary cold wired startup; changing shortcuts or reading the switch alone cannot fix it. A replacement independent recovery route must be established before removing this existing route.

## Hardware constraints and implementation boundary

The [vendor-published pin map](https://github.com/NocFreeKB/NocFree-and-zmk#4-pins-required-for-zmk-porting) identifies left P0.15 as BLE position and P0.17 as receiver position, each active low with pull-up. Treat these as source evidence; verify physical position readings on this unit before acceptance. The right pin table lists no on/off sense input. Previous USB-powered observations show a light can stay on with the switch off; they do not establish a broken switch, and no schematic/current measurement proves full power isolation. Firmware cannot honor an unreadable physical switch or disconnect USB power electrically.

Keep routing, profile/pairing actions and long-hold timing in RMK. Board code supplies switch inputs through a framework seam. Do not implement a parallel keyboard engine or insert transport control into the key scanner. Run the host harness and cross-build supported roles before transport changes. Hardware acceptance must cover selector transitions with held keys/modifiers, no stuck releases on old hosts, three profile reconnections, receiver pairing/reconnection, cold wired startup and recovery on each half. Host tests cannot certify those behaviors.

## Work order

1. Establish the selector readings and the right switch's actual hardware capabilities. Resolve cold USB recovery separately from everyday mode routing.
2. Implement strict left output routing and three host slots in RMK, with explicit pairing actions and factory gestures. Include safe release behavior when switching hosts.
3. Add half-specific five-second recovery entry, including a right-local path that works without the left radio link. Retain the existing safeguards until the replacement route is verified.
4. Only after those controls are reliable, update Companion instructions and publish the install/update journey. An installer must not teach the development battery-start dance as normal operation.

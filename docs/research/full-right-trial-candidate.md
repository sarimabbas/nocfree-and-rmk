# Full right RMK split trial candidate

Prepared 2026-10-02. The owner approved the scoped trial and the one-shot host transfer completed after fresh right identity and exact baseline checks. Independent recovery/readback, basic split typing and cross-half Shift have passed. The full hardware acceptance remains pending. The left runs the verified full RMK USB candidate. The saved working right Mac diagnostic and factory backup remain available privately; current right application execution is not yet established.

## Exact candidate

Clean packaging source `ea07fc59a43da41681f39ec6094155e372679890`; features `right,reclaimed-softdevice,usb-recovery-first`. Pinned Embassy patch `9640cd7af9d2e5f9fe9860aae7108e4a04f96c8c`. ELF SHA-256 `2c8e484499e5985b7199cce81e474e86398da8240851fc7edf5444e6de318a55`; BIN `f13ceb55d5c2420f50a7398d5e0522dabd57a3e6148a1f4873dc3da311c09b8a`; UF2 `ba81202482b2d68eea470872f1af2a6907924af122a60d5a9a5a4d6eacf51e76`.

Independent offline reconstruction confirms 218,316 application bytes through `0x364cc`, with 2,868 erased tail bytes through `0x37000`; 864 contiguous UF2 blocks, family `0x621e937a`, stack `0x20020000`, reset `0x1205`, recovery marker at `0x1200`. RAM segments remain within `0x20008000`–`0x20020000`. The shared migration guard validates structural coverage but labels its policy left; it does not establish right-specific approval.

## Fresh baseline and proposed scope

The owner entered the right diagnostic's existing bootloader with Fn+0. A fresh unique MSC device was correlated with saved right metadata; complete exposed readback matches the known working Mac diagnostic exactly. The original factory backup remains saved privately. This is exposed application/S140/storage coverage, not a complete-chip backup. Device identifiers and binary files stay outside Git.

The proposed transfer replaces S140 and writes `0x1000`–`0x37000` exclusive. Runtime RMK storage may erase/write `0x65000`–`0x6d000`. Both recovery gates must require exact application and erased tail, unchanged protected gap `0x37000`–`0x65000`, and preserve/classify any permitted storage changes. Initial storage writes are allowed explicitly because the left trial showed initialization before the planned battery-first observation; do not call changed storage original or imply measured launch timing. No MBR, bootloader or UICR writes, factory restore, dock operation, serial-port open or automatic transfer/retry is proposed.

## Hardware trial

Existing right cold recovery was observed with OFF plus USB disconnected first, then USB reconnected while OFF. The full lower-layout right candidate must prove its own independent recovery and exact readback after installation; the left's result is supporting evidence, not substitution.

The right peripheral intentionally has no runtime USB HID/CDC driver. Absence from USB after launch is expected. After independent recovery passes, start it battery-first near the unchanged running left. RMK discovers split peripheral ID 0 automatically and persists peer addresses; no host Bluetooth pairing is required for this link. Source inspection establishes peer-address persistence, not authenticated bonding.

First acceptance is owner-confirmed right keys through the left USB keyboard, then cross-half modifiers and layers, simultaneous input, reconnect/release recovery and wake latency. Split/radio/battery/latency acceptance is pending. Right-only USB typing disappears with this role; the left owns the combined keymap. Full source topology and primary references are in [split architecture](split-architecture.md).

## Approved transfer checkpoint

The owner explicitly approved the exact image and application/runtime-storage ranges above. Thirty independent offline helper checks passed, including wrong role, image, source, range, identity, pending approval, stale review, repeated transfer and corrupted readback rejection. These are host checks, not hardware acceptance. The candidate-bound one-shot transfer completed after another fresh same-device identity, exact metadata and full baseline readback. No automatic retry occurred. The owner-operated right OFF/USB-absent/reconnect-OFF recovery cycle is the next gate; no successful new-image recovery or split input is assumed. Companion remains paused because its existing physical startup text is left-specific.

## Independent recovery observation

The owner performed the requested OFF/USB-absent-five-seconds/reconnect-while-OFF sequence. Fresh right identity and bootloader metadata passed. Readback matches the exact candidate application and erased tail; the entire protected gap remains unchanged. Only 39 bytes differ in the approved storage interval, on page `0x65000`; both storage copies and the complete readback are saved privately. Independent offline review decoded two complete records with valid length/data checksums: the right-specific RMK schema and a valid saved peer address. This supports initialization and peer-address persistence; that record alone does not identify the peer as the left or establish authenticated bonding. This proves the candidate's right recovery/readback gate, not split input. The unchanged left full RMK device remains connected. The next step was a battery-first right launch, followed by right-key input through the left USB keyboard.

## First basic split typing pass

After owner-operated battery-first launch, the right remained USB-unplugged while the unchanged left stayed connected. The owner typed `hjkl` using only the right and confirmed it works. A read-only USB inventory verified the full left device present and the right device absent. This establishes the first basic right-to-left RMK BLE split input through left USB on macOS. Cross-half modifiers/layers, simultaneous input, held-key disconnect/rejoin, wake latency, exhaustive mapping and loss-free acceptance remain pending. No additional transfer or factory restoration was needed.

The owner subsequently held Shift on the left while typing `HJKL` on the USB-unplugged right and confirmed it works. This verifies that cross-half Shift combination. Other modifiers/layers and the remaining acceptance conditions above are still pending.

## Final retained-image readback

After the split typing and Shift observations, the owner returned the right to recovery with OFF plus USB absent for five seconds, then reconnected USB while OFF. Fresh same-device identity and exact application/tail/protected-gap gates passed again. Independent offline review confirms the entire exposed readback is byte-identical to the first accepted full-right recovery snapshot. No additional storage changes occurred beyond the previously decoded schema and peer-address records. The owner then performed the final battery-first restart, left right USB unplugged, and typed `hjkl` using the right again. Read-only inventory confirmed the left full RMK USB device present and the right USB absent. Both halves are left operating in the tested split configuration. This completes the bounded right migration/split typing trial; no further transfer or restore occurred. Full reliability, direct host BLE, receiver and remaining feature acceptance are still pending.

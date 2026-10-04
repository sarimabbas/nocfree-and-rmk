# Battery reporting while retaining Vial

Research only, 2026-10-03. No firmware changes, device commands or hardware validation. Source inspection uses the currently pinned RMK fork, `f1c620c6c089cab385d92868b2e1060d03bddb04`. The seven battery/VIA/relay source files cited below were byte-identical to the previous `b41bd7c` pin; the Rynk/Vial feature exclusion remains present.

## Recommendation

Keep Vial. For Companion battery reporting over both wired USB and the USB receiver, add a small **read-only VIA custom-value getter in RMK**, returning the framework’s existing central battery and peripheral-status snapshots. This uses Vial’s existing HID request/reply channel and receiver relay. It does not need another USB interface, Rynk, a replacement battery processor, or changes to keyboard scanning. This is a conventional extension mechanism, **not a getter already implemented by stock Vial or this pinned RMK**. [VIA custom-value extension documentation](https://github.com/the-via/website/blob/master/docs/custom_ui.mdx), [pinned RMK unsupported custom-get handler](https://github.com/sarimabbas/rmk-nocfree/blob/f1c620c6c089cab385d92868b2e1060d03bddb04/rmk/src/host/via/mod.rs).

Use one documented custom channel/value namespace with a protocol version and an explicit unsupported/error response. A compact response should distinguish available/unknown percentage, unknown charging state, and right-link connection; disconnected cached right levels must not be displayed as current. Read the same producer accessors used by the existing Rynk getters. Keep set/save commands unsupported and do not piggyback on recovery commands. Poll no faster than the board’s 30-second measurement interval. A timestamp would require an actual producer-owned timestamp; request time cannot establish sample freshness. [RMK central accessor and Rynk getters](https://github.com/sarimabbas/rmk-nocfree/blob/f1c620c6c089cab385d92868b2e1060d03bddb04/rmk/src/host/rynk/handlers/status.rs), [peripheral status cache](https://github.com/sarimabbas/rmk-nocfree/blob/f1c620c6c089cab385d92868b2e1060d03bddb04/rmk/src/split/driver.rs), [board sampling seam](../../firmware/src/battery.rs).

This recommendation is an inference from existing interfaces, not a tested implementation. Keep the extension at RMK’s host-protocol seam and propose it upstream; pin the version and maintain a small getter contract test rather than carrying a second host stack.

## Existing conventional paths

| Path | What is already present | Limit |
| --- | --- | --- |
| Direct Bluetooth Battery Service | RMK exposes standard Battery Level characteristics with read/notify and separate main/peripheral service instances, percentage presentation descriptors and user descriptions. | Companion needs native GATT integration and direct access to the keyboard’s selected Bluetooth host connection. OS UI presentation of both levels is unverified; this does not cover a USB dongle session. |
| VIA custom-value getter | The standard `CustomGetValue` command exists, but the pinned RMK handler currently logs unsupported. | Requires the small RMK extension and Companion HID client above; Vial’s stock UI does not automatically acquire a battery panel. |
| Keep Rynk alongside Vial | Existing Rynk getters return the needed typed snapshots. | The pinned RMK explicitly makes `rynk` and `vial` mutually exclusive. Adding both is a larger framework/transport change, not the smallest integration. |

Sources: [RMK Battery Service](https://github.com/sarimabbas/rmk-nocfree/blob/f1c620c6c089cab385d92868b2e1060d03bddb04/rmk/src/ble/battery_service.rs), [Bluetooth SIG Battery Service multiple-instance specification](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/BAS_v1.1/out/en/index-en.html), [VIA command definitions](https://github.com/sarimabbas/rmk-nocfree/blob/f1c620c6c089cab385d92868b2e1060d03bddb04/rmk-types/src/protocol/vial.rs), [RMK feature exclusion](https://github.com/sarimabbas/rmk-nocfree/blob/f1c620c6c089cab385d92868b2e1060d03bddb04/rmk/src/lib.rs).

The receiver’s Vial router forwards whole 32-byte reports and their replies to the keyboard. Therefore a getter implemented on the left can return **both halves’ levels through the receiver**, without a separate battery cache or custom radio packet on the dongle. The disconnected relay explicitly stops pending forwarding. This source-level feasibility still needs receiver hardware acceptance and bounded host error handling. It is distinct from runtime DFU: recovery must continue targeting the locally connected component, not the forwarded keyboard. [Pinned receiver Vial router](https://github.com/sarimabbas/rmk-nocfree/blob/f1c620c6c089cab385d92868b2e1060d03bddb04/rmk/src/dongle/vial_router.rs).

## Transport does not fix the low percentages

The owner initially recorded **4.17 V** as battery-terminal voltage alongside a **12%** getter result, but subsequently corrected those contacts to a **switched power rail**: 4.17 V with USB, 4.17 V in Bluetooth with USB unplugged, and 0 V in WIRED with USB unplugged. The original private record’s battery-terminal label is superseded. This is evidence of that rail’s switch-dependent power state, not an established measurement of the cell or charging current.

If that rail accurately represents the cell voltage, 4.17 V substantially disagrees with the configured estimate: 12% implies roughly 3.64 V. That inference is conditional; the raw ADC count and its sample timestamp were unavailable, and the rail’s relationship to the battery has not been established. Divider ratio, acquisition, ADC input/reference, stale producer state and capacity estimation remain distinguishable hypotheses. Obtain a confirmed cell-voltage measurement paired with the same interval’s raw ADC count before treating a producer conversion defect as proven. [Existing public measurement analysis](battery-reporting.md), [RMK battery conversion](https://github.com/sarimabbas/rmk-nocfree/blob/f1c620c6c089cab385d92868b2e1060d03bddb04/rmk/src/input_device/battery.rs).

Vial, Rynk and BLE BAS all export the same producer estimate; changing transport cannot turn that estimate into calibrated capacity. Do not smooth or remap 12% to hide the disagreement. The next measurement discriminator remains one same-interval raw ADC observation plus meter voltage and exact ADC configuration; then correct the measured board conversion/acquisition issue at the board seam if warranted. USB presence is not evidence of charging; no charge-state reader is configured. Keep Unknown truthful. [Board sampling](../../firmware/src/battery.rs), [prior battery investigation](battery-reporting.md).

Minimum acceptance for a later getter implementation: independent host protocol tests for unsupported/version/invalid/disconnected replies; all-role builds and existing harness; owner-device read-only reads through left USB and receiver USB; matching direct BLE BAS observations; and separate battery-voltage/raw-count calibration. Host tests or builds establish none of those hardware results.

## Implemented query contract

The opt-in RMK `vial_battery` feature handles one VIA `CustomGetValue` request. The board enables it on the left; the receiver uses its existing Vial relay. No set, save, unlock, reset or recovery command is involved.

All reports are 32 bytes. A request begins `08 7e 01 01 4e 43 42 54` (custom getter, channel, value, version, `NCBT`), followed by zeroes. Replies retain that header. Byte 8 is status: 0 success, 1 unsupported requested version, 2 malformed request, 3 unavailable feature. Bytes 9–11 describe the left; byte 12 describes the right link; bytes 13–15 describe the right battery. Bytes 16–31 are reserved zeroes.

Each battery entry contains availability (0 unavailable, 1 available, 2 invalid), percentage (0–100 or `ff` unknown), and charging state (0 unknown, 1 charging, 2 discharging). Unavailable/invalid entries use `ff` percentage and unknown charging. Right-link values are 0 disconnected, 1 connected, 2 unconfigured. A disconnected or unconfigured right must return an unavailable entry, even if RMK retains a previous measurement. The reply carries no sample timestamp.

Companion selects a unique directly connected left first, otherwise a unique receiver, and reads only its native USB raw-HID collection (usage page `ff60`, usage `61`). It checks the connection again around the query. Native HID writes are synchronous OS calls; the UI bounds how long it waits and permits only one outstanding worker rather than claiming those calls are cancellable.

### LEFT hardware observation, 2026-10-03

The normal LEFT build at repository checkpoint `558164f` returned a successful native Vial query before and after a normal USB restart. Both reads reported 13%, unknown charging state and a disconnected right with an unavailable battery entry. The rebuilt Companion visibly displayed a left estimate (11% at its later poll) and “Right: Not connected.” This establishes direct LEFT USB transport and UI display, not calibrated battery capacity, current sample freshness, or right-half battery acceptance. Receiver relay, direct BLE comparison and raw ADC/cell-voltage calibration remain pending.

After the matching RIGHT update and owner confirmation of both halves typing, the same native read returned LEFT 13%, RIGHT 11%, right-link connected, and unknown charging state for both. This establishes both-half cached reporting through LEFT USB. It does not validate the low estimates or establish charging state. Receiver relay and ADC calibration remain pending.

### Receiver runtime recovery observation, 2026-10-03

The receiver's matching normal build was installed from a fresh device-specific backup with an independently reviewed application-only operator. Exact application and page padding matched on readback, and the S140 prefix plus untouched application gap matched the fresh baseline. Runtime storage differed, as expected for the changed RMK schema. The factory bootloader was not modified; its executable region is not included in the readable UF2 backup.

The actual Companion **Recovery mode → USB receiver** flow reopened the receiver's factory recovery drive without a key chord or physical restart. This establishes normal runtime recovery on this receiver, not deliberate-crash recovery or calibrated battery reporting. Normal receiver restart, isolated dongle typing and Vial battery relay are the next hardware checks. The owner authorized this application update after the receiver-specific crash-proof limitation was disclosed.

After ordinary receiver unplug/replug it enumerated as **NocFree RMK Receiver**. With only the receiver connected by USB and direct host Bluetooth disconnected, the owner confirmed both-half typing in response to the shared-modifier check. A read-only native Vial request through the receiver returned LEFT 12%, RIGHT 9%, right-link connected and unknown charging state. This establishes receiver relay of both producer caches; it does not calibrate those percentages or prove sample freshness. Before selecting the receiver route, the disconnected relay returned an unsupported response; that observation was archived separately. Battery ADC/cell-voltage calibration, physical-switch transport selection and deliberate receiver crash recovery remain separate work.

### Raw measurement getter

Fork `acd4689a1284f27decd4d0755a1e316fddcbb8ff` adds a separate read-only diagnostic getter on value 2, leaving the percentage getter unchanged. Request `08 7e 02 01 4e 43 41 44` (`NCAD`) uses a zero tail. The board records the actual signed sample and SAADC register values at the existing sampling seam; it neither samples on demand nor changes conversion.

The 32-byte reply keeps that header. Byte 8 is status (0 available, 1 unsupported version, 2 malformed request, 3 unavailable). Bytes 9–10 hold signed little-endian ADC count; 11–14 hold saturating little-endian sample age in milliseconds. Byte 15 holds sampled switch levels (bit 0 present, bit 1 Bluetooth input high, bit 2 receiver input high). Bytes 16–31 hold four little-endian registers: resolution, oversample, channel configuration and positive input selection. Errors contain no measurement payload. Switch levels belong to the cached observation, not a live position query.

The normal LEFT candidate retains Mac keys, backlight, split, Vial, runtime recovery and watchdog without an extra USB logger. A paired raw sample and confirmed cell-voltage measurement are still needed before adjusting scale or estimating real capacity.

### Current selected mode getter

The mode-reporting update adds a separate read-only getter, preserving both battery and ADC replies. Request `08 7e 03 01 4e 43 4d 4f` (`NCMO`) has a zero tail. The 32-byte response retains that header: byte 8 is status (0 supported, 1 unsupported version, 2 malformed request), byte 9 is selected mode (0 unknown/off/automatic, 1 wired, 2 Bluetooth, 3 dongle), and bytes 10–31 are zero.

The getter reads RMK's current output selection, which the left board's debounced physical-switch reader supplies. It does not read GPIO in the host handler, take another ADC sample, select a transport, or change storage. Host connection and selected mode are separate facts: USB can remain connected while the switch selects Bluetooth. Companion reads this getter with the existing USB status snapshot every three seconds; the battery measurement interval remains thirty seconds. Unsupported or malformed mode replies display mode unavailable. Physical mode through a wireless-only host connection is not yet observed by this USB getter.

Focused framework and Companion protocol tests pass; all three roles cross-build and pass their image guards. Hardware acceptance is recorded separately from these checks.

### Status snapshot version two

The battery getter also accepts request version 2 (byte 3); version 1 remains
unchanged. Successful v2 replies retain the existing battery/link bytes and add:

| Byte | Meaning |
| --- | --- |
| 16 | Flags: bit 0 left USB power, bit 1 right USB power, bit 2 active Wired route, bit 3 active direct Bluetooth route, bit 4 active Dongle route |
| 17 | Selected switch policy: 0 unknown/off/automatic, 1 Wired, 2 Bluetooth, 3 Dongle |
| 18 | Known mask: bit 0 left USB power known, bit 1 right USB power known, bit 2 active route known, bit 3 switch policy known |
| 19–31 | Reserved zeroes |

The route uses RMK's existing output-selection decision, not cable presence.
Local power uses RMK's VBUS snapshot. Right power is an appended split message;
an older right leaves that fact unknown. Disconnect clears the right-power
snapshot. Existing split message tags remain unchanged. The dongle transparently
relays this getter to LEFT, so successful queries are live communication, not
local receiver-cache reads. This corrects the audit's earlier implication that
the reply itself could prove no live link.

Companion requests v2, falling back to v1 only after a valid unsupported reply.
It retains percentages but expires connectivity facts after 45 seconds without
a successful query, and clears them when the producer changes. USB power,
selected mode, active typing route and per-part recovery render separately.
No battery sample timestamp or active charging assertion is added.

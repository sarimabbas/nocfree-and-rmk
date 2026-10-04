# Physical mode switch: evidence and framework boundary

Researched 2026-10-03. This is source research only: no device operations, GPIO changes, electrical measurement or transport implementation. The production firmware remains responsible for key processing and transport through RMK.

## Left switch can be sensed

The [vendor-published hardware guide](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/README.md#41-left-controller) explicitly assigns two **nRF GPIO inputs**, separate from the PCA9555 key ports:

| Signal | Input configuration | Published asserted state |
|---|---|---|
| BLE position | P0.15, pull-up | Low in Bluetooth position |
| Receiver position | P0.17, pull-up | Low in 2.4 GHz position |

These are published pin assignments, not measurements of this owner's PCB. The documented selector therefore has independent position sensing; treating both radio positions solely as indistinguishable battery power would miss that interface. The guide does not supply the complete power schematic or the middle-position truth table. The community [limitations](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/docs/limitations.md) explicitly leaves mode-switch support unimplemented and its electrical role unverified; its compiled ZMK behavior does not establish our truth table.

Expected readings to **measure**, while USB supplies power:

| Owner's physical position | P0.15 | P0.17 | Status |
|---|---|---|---|
| Top / receiver | High | Low | Asserted receiver pin published; other pin inferred |
| Middle / wired | High | High | Inferred, not electrically verified |
| Bottom / Bluetooth | Low | High | Asserted BLE pin published; other pin inferred |
| Transition / invalid | Low | Low | Meaning unspecified; do not interpret as a valid fourth mode |

Read both as pull-up inputs, never drive them. Record stable values for all three detents and contact transitions before enabling routing. Input pull-ups and two bits are sufficient for observation; no expander reconfiguration or output probing is justified. This research did not identify a PCA9555 mode bit.

## Right OFF is a power-path question

The same [vendor table](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/README.md#42-right-controller) supplies **no right on/off position-detect GPIO**. P0.17 on the right is a shared charge/low-battery indicator, not the left's receiver detector; do not reuse the left pin mapping there.

Our saved [device observations](../device-observations.md) record normal right USB enumeration while its switch was OFF and USB remained attached. This verifies continued USB power/operation in that condition; it does not identify a defective contact or a software-readable OFF signal. The switch's battery-only isolation and any software-controlled power gating remain unmeasured. Firmware cannot disconnect an unswitchable USB supply. Achieving the owner's requested OFF-even-with-USB behavior therefore needs a verified sense line or confirmed control circuit; none is established here. Do not promise a firmware repair or assign an unused pin by guesswork.

## Current RMK API does not express strict physical modes

Inspected the actual production [fork pin c8e06c6](https://github.com/sarimabbas/rmk-nocfree/tree/c8e06c6edbda227114ab7deeb9234d2ecca671c6). The [state module](https://github.com/sarimabbas/rmk-nocfree/blob/c8e06c6edbda227114ab7deeb9234d2ecca671c6/rmk/src/state.rs) exposes `set_usb_state` for the real USB lifecycle. Preferred-output mutation and BLE-profile state are crate-private. The [profile manager](https://github.com/sarimabbas/rmk-nocfree/blob/c8e06c6edbda227114ab7deeb9234d2ecca671c6/rmk/src/ble/profile.rs) owns the private profile-action channel and dedicated receiver slot. `ConnectionStatusChangeEvent` reports state; it is not an absolute output-selection command. Existing [keyboard actions](https://github.com/sarimabbas/rmk-nocfree/blob/c8e06c6edbda227114ab7deeb9234d2ecca671c6/rmk/src/keyboard.rs) reach these internals inside RMK.

Critically, [ConnectionStatus::decide_active](https://github.com/sarimabbas/rmk-nocfree/blob/c8e06c6edbda227114ab7deeb9234d2ecca671c6/rmk-types/src/connection.rs) falls back to whichever transport is ready; preference only breaks a tie. Setting preferred BLE is **not** enough to implement “Bluetooth/receiver mode ignores USB cable.” Preferred USB alone does not implement “wired without USB is off,” because a ready BLE connection still wins when USB is absent. Faking `UsbState::Disabled` would corrupt the actual bus lifecycle rather than implement output policy.

The smallest appropriate extension is an RMK-owned **absolute output-selection API**, accepting USB, ordinary Bluetooth, or receiver. Board code only supplies the observed stable position. Inside RMK, reuse the existing profile manager, dedicated receiver slot, connection state, report-channel release and event machinery. Preserve the last ordinary host profile inside the framework, do not hard-code the private receiver slot, do not synthesize Fn key events, and do not toggle persisted preference on every sample. Strict selection needs an explicit policy in routing, not a second board-local transport engine. This is a proposed API, not an available or tested feature.

Before enabling it, require repeatable host coverage of startup/repeated selection, profile retention and old-output release, then all-role cross-builds and hardware tests of all positions, cable attach/detach, held modifiers, simultaneous cross-half input, disconnect/release recovery and wake latency. “Wired without cable produces no host output” can be implemented as routing policy; true electrical OFF and battery current require separate measurement.

## Prepared implementation

The fork at `acd4689a1284f27decd4d0755a1e316fddcbb8ff` adds strict output selection inside RMK. The board reads only the two passive left inputs, establishes a restricted route synchronously before tasks run, and applies changes after a stable 25 ms interval. Invalid startup contacts select no output; invalid transitions retain the last valid selection. RMK owns profile changes, last-host retention and release of queued reports from the old route. Bluetooth selection is resolved after profile storage loads. Receiver selection does not overwrite the saved ordinary host profile. Fn+B and Fn+U are removed from the compiled default keymap because the physical selector now owns this choice.

A read-only Vial `NCAD` getter reports the signed ADC sample, its age, actual ADC configuration registers and observed switch input levels. It neither changes sampling nor adjusts the percentage. All functionality remains enabled without a CDC logging interface. Eighteen focused RMK host tests cover routing, old-host release and both Vial getters, and the scanner harness covers the selector truth table. These are source/host results; physical switch behavior, reconnection, simultaneous input and latency still need acceptance.

The new fork revision changes the existing RMK storage hash, so first startup can reset bonds and settings. This update does not promise schema-compatible pairing retention. Right OFF with USB connected remains a hardware-sense limitation; do not claim that the left position inputs solve it.

## Owner acceptance, 2026-10-03

The installed left image matched its prepared bytes, padding and protected gap.
Normal USB startup in middle WIRED enumerated successfully; a fresh ADC snapshot
recorded both sense inputs high. The owner typed `qwert HJKL`, with Shift on the
left and letters on the right. In top dongle position, typing worked with left
USB connected, stopped when only the dongle was removed, and returned when it
was reconnected. In bottom Bluetooth position, the owner paired again and
confirmed typing with both halves' USB and the dongle unplugged. These establish
basic physical route selection and shared modifiers; they do not establish
measured wake latency, all release/disconnect edge cases, electrical OFF, or
right switch sensing.

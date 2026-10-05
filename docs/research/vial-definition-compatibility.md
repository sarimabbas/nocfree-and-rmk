# Vial definition and RMK compatibility

Research only, 2026-10-04. No devices accessed, firmware changed, or application restarted. Source inspection uses this repository and its pinned RMK fork `0afc58c9db423313faa0c47021fc7957f4416a11`. Current upstream documentation was also checked; upstream instructions are not evidence that this installed keyboard passed Vial acceptance.

## Finding

The project already has Vial support and an embedded definition. The physical geometry has now been improved and checked offline; the remaining acceptance work is checking the actual Vial application with the owner, rather than creating a new host protocol or requiring users to import JSON. RMK supports live key remapping and storage-backed persistence; Vial and Rynk are mutually exclusive. Upstream also documents `rmkit layout convert --to-vial keyboard.toml` when TOML contains a physical `[layout]` section. Our TOML does not contain that section: its keymap is authored in Rust. [RMK Vial documentation](https://rmk.rs/main/docs/features/vial_support), [rmkit layout tools](https://github.com/rmk-rs/rmkit#layout-tools), [local TOML](../../firmware/keyboard.toml), [local keymap](../../firmware/src/keymap.rs).

## Already present

| Item | Current source evidence |
| --- | --- |
| Definition | [firmware/vial.json](../../firmware/vial.json): name `NocFree RMK`, matrix 1 × 84, one combined split layout, lighting `none`. Root's read-only check found every coordinate `0,0` through `0,83` exactly once. |
| Firmware mapping | [keymap.rs](../../firmware/src/keymap.rs): 37 left positions followed by 47 right positions; two layers. [keyboard.toml](../../firmware/keyboard.toml) uses matching peripheral column offset 37. |
| Embedded payload | [build.rs](../../firmware/build.rs) parses JSON, compacts it, XZ-compresses it and writes `OUT_DIR/vial_definition.xz`; [vial.rs](../../firmware/src/vial.rs) includes those bytes. Root decoded all 31 existing generated payload artifacts and found they match current parsed JSON. This checks host build artifacts, not installed firmware. |
| Identity and unlock | [vial.rs](../../firmware/src/vial.rs) supplies a stable eight-byte community keyboard UID and unlock positions `(0,32)` + `(0,26)`, physical left Fn + Shift. Both left and dongle enable `vial` and `host_lock`; left enables storage through RMK's shared dependency configuration. [Cargo.toml](../../firmware/Cargo.toml). |

Vial retrieves the UID, compressed definition length and definition pages from firmware, then decompresses and renders the KLE data. This is distinct from a `.vil` saved user keymap. No public definition registry or manual JSON sideload is required for that embedded Vial route. The ordinary USB discovery path uses Vial's serial marker and raw-HID usage, not `vendorId`/`productId` from the downloaded definition. Those fields matter for VIA/sideload lookup; therefore the dongle's USB PID `0x4644` versus definition PID `0x4643` is not by itself a Vial auto-detection defect. [Vial definition retrieval](https://github.com/vial-kb/vial-gui/blob/master/src/main/python/protocol/keyboard_comm.py), [Vial discovery](https://github.com/vial-kb/vial-gui/blob/master/src/main/python/util.py), [sideload lookup](https://github.com/vial-kb/vial-gui/blob/master/src/main/python/autorefresh/autorefresh_thread.py), [local USB descriptors](../../firmware/src/main.rs).

## Reusing the diagrams

Vial accepts KLE geometry and matrix-coordinate legends, not an SVG picture. The drawing must identify each physical key by its firmware matrix position; importing the image alone cannot provide that mapping. Vial supports cap widths, offsets, split gaps and layout variants. Its official process is authoring KLE data and dummy-loading the resulting definition before hardware testing. [Vial definition guide](https://get.vial.today/docs/porting-to-via.html).

The initial JSON had all keys at default unit width and used only `x: 1.5` split gaps. The updated definition preserves all electrical labels and adopts the SVG widths and row stagger. The [left SVG](../../desktop/assets/nocfree-left.svg) and [right SVG](../../desktop/assets/nocfree-right.svg) contain staggered rows and varied cap widths that could supply those visual measurements. Each key is drawn twice, as base and cap: naïvely treating every rectangle as a key would double-count. The blank space keys also have no text label. No SVG element currently carries an explicit electrical coordinate. These are source observations, not dimensions measured from hardware.

Recommendation: keep the coordinate mapping unchanged and improve only KLE geometry, using the SVGs as a visual reference. For later shared generation, introduce one small explicit geometry table containing matrix coordinate, cap rectangle and half; generate KLE and SVG from that table. Do not guess electrical mapping from legends or rectangle order. The dongle has no remappable keys and does not belong in the Vial key layout.

## Transport and feature limits

| Route | What source inspection establishes | Acceptance still needed |
| --- | --- | --- |
| Left USB | Pinned RMK exposes a Vial USB service. | Actual Vial discovery, accurate rendering and a reversible remap with restart persistence. |
| Dongle USB | Pinned [dongle router](https://github.com/sarimabbas/rmk-nocfree/blob/0afc58c9db423313faa0c47021fc7957f4416a11/rmk/src/dongle/vial_router.rs) relays whole reports to left, aside from reserved Companion pairing commands. Both builds select Vial. | Same GUI checks through a live dongle link. |
| Direct Bluetooth | Pinned [BLE Vial transport](https://github.com/sarimabbas/rmk-nocfree/blob/0afc58c9db423313faa0c47021fc7957f4416a11/rmk/src/ble/host/vial.rs) implements 32-byte GATT reports. | Ordinary Vial desktop discovery is HID-based, so firmware GATT support does not establish macOS GUI compatibility. Test separately; offer USB or dongle configuration first. |
| Right USB | Right role does not enable Vial; the complete keymap belongs to left. | No separate right editor should be promised. |

Upstream describes the dongle as a protocol relay with no keymap of its own; matching host-protocol features on keyboard and dongle are required even when typing already works. [RMK dongle documentation](https://rmk.rs/main/docs/features/dongle).

The definition has no named custom keycodes. Current Bluetooth profile actions use `Action::User`; adding labels requires checking the exact pinned RMK conversion instead of copying contemporary QMK numeric constants. In this pin, `Action::User(n)` maps to `0x7e00 | n`; the definition guide's custom-keycode example uses a different namespace. [Pinned conversion](https://github.com/sarimabbas/rmk-nocfree/blob/0afc58c9db423313faa0c47021fc7957f4416a11/rmk/src/host/via/keycode_convert.rs), [Vial custom-keycode guide](https://get.vial.today/docs/custom_keycode.html).

Keep lighting `none` until the actual pinned protocol supports the GUI controls. Backlight operation in firmware is separate from Vial lighting configuration: this pin's keycode conversion rejects backlight/RGB configuration keycodes. Its reduced macro/combo/morse capacities also mean the full QMK/Vial feature set must not be advertised. [Pinned conversion](https://github.com/sarimabbas/rmk-nocfree/blob/0afc58c9db423313faa0c47021fc7957f4416a11/rmk/src/host/via/keycode_convert.rs), [local capacity configuration](../../firmware/keyboard.toml).

## Acceptance plan

1. Offline: verify all 84 coordinate labels remain unique/in bounds; dummy-load JSON in Vial and compare stagger and cap widths with the diagrams.
2. Owner-approved live test after returning to RMK: open Vial over left USB, unlock with left Fn + Shift, use Matrix Tester to check physical positions, temporarily remap one ordinary key, restore it, and check persistence after restart.
3. Repeat through dongle USB with direct host Bluetooth disconnected. Distinguish a working Companion getter or successful typing from a working Vial GUI.
4. Treat direct Bluetooth GUI compatibility and other hardware layouts as separate unverified acceptance gates.

No firmware update, scanner change or flashing is required merely to research these steps. Changing embedded geometry later requires a new guarded firmware build and separately authorized installation.

## Offline geometry acceptance, 2026-10-04

The updated definition retains every matrix address exactly once and all other
metadata unchanged. Its widths and stagger follow the existing SVG drawings at
48 pixels per key unit, with a 1.5-unit split gap. An independent review checked
the mapping against the Rust keymap, including Delete `(0,68)` and PageDown
`(0,76)`, both blank space keys, and KLE width/row reset behavior.

Root loaded the JSON with the official Vial GUI v0.7.5 KLE parser and rendered
its official Qt `KeyboardWidget` offscreen. All 84 keys were in bounds, with no
rectangle overlaps. The preview labels show matrix addresses deliberately, so
mapping can be inspected. XZ compression/decompression also round-tripped to
the same definition (388 compressed bytes). This exercised the actual parser
and renderer without starting device enumeration or connecting to a keyboard;
it is not a live Vial GUI/remapping or persistence test.

The local preview is `.evidence/vial-layout-preview/vial-layout.png`. Official
source: [Vial v0.7.5 KLE parser](https://github.com/vial-kb/vial-gui/blob/v0.7.5/src/main/python/kle_serial.py)
and [keyboard renderer](https://github.com/vial-kb/vial-gui/blob/v0.7.5/src/main/python/widgets/keyboard_widget.py).
No installed firmware or Companion firmware package was replaced by this work.

## Backlight conversion and OS presets, 2026-10-04

An isolated RMK fork commit `8d07593e4f829ec165f08a2286dd516dc7113c21` adds
round-trip conversion for backlight On, Off, Toggle, Down, Up and Step
(`0x7800` through `0x7805`). It leaves breathing and RGB unsupported and changes
no scanner, transport or backlight hold behavior. Until that dependency is
published and pinned, the earlier pinned firmware still has the limitation above.

The definition now labels User actions 0 through 7 for Bluetooth profiles,
next/previous profile and bond clearing. Optional Mac and Windows/Linux
[layout presets](../../firmware/presets/README.md) preserve the existing two-layer
keymap and Bluetooth/Fn/backlight actions. The PC preset uses ordinary F1-F12
on the base layer and omits Mac Mission Control from Fn+F3.

Validation: all 595 RMK host tests with backlight enabled passed; left, right
and receiver release cross-builds passed using the isolated patch. Four preset
checks passed, as did an independent source review. The official Vial v0.7.5
`DummyKeyboard.restore_layout` imported both presets and preserved all 168 numeric
keycodes per preset with HID access forbidden. These are offline checks, not
hardware remapping or restart-persistence acceptance. No connected devices,
installed Companion app or primary checkout were modified.

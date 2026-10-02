# Mac function row

Status: both Mac USB diagnostics are installed with exact readback and battery-first startup verified on macOS. The owner reports the requested brightness, Mission Control, Spotlight, playback/volume and Fn-function checks work. External-display brightness still requires Ctrl on this setup; F5/F6 backlight support is not implemented.

The optional `mac-keymap` Cargo feature selects the user's Mac function row in the
shared electrical default keymap. Both USB input diagnostics and the production left
central use it. Current features use the compiled keymap; enabling RMK's host
configuration later requires a separate review of stored-layout behavior.
This is an interim compile-time choice: it adds no runtime OS
detection, profile persistence, or keyboard engine. Without the feature, the
existing keymap and diagnostic USB names/greetings remain unchanged.

| Key | Plain press with `mac-keymap` | Fn held |
| --- | --- | --- |
| F1 | Display brightness down | F1 |
| F2 | Display brightness up | F2 |
| F3 | Mission Control | F3 |
| F4 | AC Search, intended for Spotlight | F4 |
| F5 | F5; keyboard backlight down pending | F5 |
| F6 | F6; keyboard backlight up pending | F6 |
| F7 | Previous track | F7 |
| F8 | Play/pause | F8 |
| F9 | Next track | F9 |
| F10 | Mute | F10 |
| F11 | Volume down | F11 |
| F12 | Volume up | F12 |

The intended final F5/F6 behavior adjusts the NocFree keyboard's own backlight.
The pinned RMK revision defines `BacklightDown` and `BacklightUp`, but documents
that light actions are ignored; its keyboard handler warns that light control is
unsupported. These keys therefore retain their ordinary function actions in
this candidate. No custom backlight handler or misleading inactive light action
is added.

RMK handles consumer usages and modifier reporting. `WwwSearch` maps to HID
consumer AC Search (`0x0221`); `MissionControl` maps to Desktop Show All Windows
(`0x029f`). Whether AC Search opens Spotlight and Mission Control opens the
expected view remains a macOS hardware acceptance check. Building a candidate
does not confirm those host behaviors.

In the existing left diagnostic, the owner verified that Control plus the
brightness action controls their external Apple display; the brightness action
alone did not. This candidate keeps the standard brightness usages. Hold Ctrl
with plain F1/F2 when the display requires it. It does not inject Ctrl into every
brightness press, which could change behavior for other displays.

The local diagnostics keep left Fn+Escape and right Fn+0 for the existing
bootloader entry. They still filter BLE profile actions because these USB-only
images run no BLE controller task. Mac diagnostics advertise `NocFree Input
Probe Left Mac` or `NocFree Input Probe Right Mac`, and include `mac-mode` in the
CDC greeting so an installation can be distinguished from the legacy candidate.
They keep the existing role-specific USB PIDs.

Build from `firmware` with the normal diagnostic safety features and the Mac
feature, for example:

```sh
cargo build --locked --release --target thumbv7em-none-eabihf \
  --bin input-probe --no-default-features \
  --features left,input-probe,usb-recovery-first,mac-keymap
```

Use `right` instead of `left` for the other local diagnostic. A build command is
not an installation command. Images still require the existing image/package
guards, device-specific evidence, and an authorized hardware trial.

Primary implementation evidence at the pinned RMK revision:

- [Consumer key mappings](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk-types/src/keycode/consumer.rs)
- [HID keycode mappings](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk-types/src/keycode/hid.rs)
- [Light action limitations](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk-types/src/action/light.rs)
- [Keyboard action handling](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/keyboard.rs)

## Guarded update candidates

Software checkpoint `2f88122` passed the 39-test host harness, formatting, all three production roles with both keymap choices in the explicit migration layout, both input diagnostic roles with both keymap choices in the factory layout, and the recovery diagnostic. These are software checks, not Mac-row hardware results.

Both packages preserve S140, the bootloader and the protected RAM floor, retain the existing recovery marker, and erase only `0x27000..0x36000`. Local files are in ignored `dist/input-probe-left-mac/` and `dist/input-probe-right-mac/`; existing working images remain available separately. At packaging time no Mac update had been installed; subsequent trial results are below.

| Role | BIN bytes | BIN end exclusive | Serial ZIP SHA-256 |
| --- | ---: | --- | --- |
| left | 60740 | `0x35d44` | `d41dea8009cf70fca2bfde42dbc5208234f83d145ab966873744d62ba6ee0f93` |
| right | 57908 | `0x35234` | `bd893c61129f6e3ff930e2ee4f6b95e086a95cac8f5d1bcb02c9b52fe47b4628` |

A proposed owner-authorized trial updates one identified half at a time, verifies exact readback and retained readable regions, and checks the distinct Mac greeting, startup and runtime update entry. Then test plain F1/F2 with Ctrl for the owner's external display, F3 Mission Control, F4 Spotlight, right media controls and Fn ordinary function keys. Do not count F5/F6 as backlight control. No radio migration or dongle write is part of this update.

## Approved Mac update progress

The owner explicitly approved both guarded Mac diagnostic updates. The left was identified through its normal role-specific USB identity/greeting, transitioned alone to MSC, and its full known-good readable hash was checked before transfer. The left Mac package installed successfully. Its distinct greeting and product name passed; the deliberate CDC update request returned to the existing bootloader.

Left readback matches the exact Mac BIN, expected FF tail through `0x36000`, unchanged S140 below `0x27000` and unchanged readable flash above `0x36000`. INFO content matched. Complete readable UF2 SHA-256 is `17306f31b8715e3e2221c4d66e7af7b951fcf2f60ba57f756b594166ada598e7`. Final left startup is pending owner participation before the right update. Mac media-row behavior remains untested. Private logs/readbacks are in ignored `.evidence/mac-trial/`; existing working restore images remain available. The dongle has not been touched.

The first reported post-update battery-first sequence left the left in MSC, rather than the expected Mac application. The readable image still matched `17306f31b8715e3e2221c4d66e7af7b951fcf2f60ba57f756b594166ada598e7`. This is a failed startup observation, not evidence of its cause. The right update has not started. Root requested a staged WIRED/USB-absent check followed by battery startup to isolate the sequence without another flash.

The staged left restart subsequently passed without another firmware write: root confirmed USB absence in middle WIRED, then the owner selected Bluetooth while unplugged, waited ten seconds and reattached USB. The left Mac product and exact greeting returned. The earlier failed observation is retained; its cause was not established.

The right was then correlated from its role-specific normal identity/greeting to a unique bootloader, and its full known-good readable hash `a1c5eebc633016e5c36a7eeac5465ac893a09c25475631d466574c6b3824d7eb` was checked before transfer. The guarded right Mac package installed successfully. Mac product/greeting and software update entry passed. Readback matched the exact candidate BIN and FF tail through `0x36000`, with S140 and readable flash outside the application erase extent unchanged. INFO content matched. Complete right readable UF2 SHA-256 is `3c18ec8d9287a2d7c97994f59753c4557ca982d59baa5fb9b691a89544432366`.

The right is currently in its existing bootloader awaiting owner-operated battery-first startup. The left is running its Mac diagnostic. Both media rows remain hardware-test pending; F5/F6 remain ordinary function keys because backlight support is still pending. The dongle remains untouched.

The owner then performed the right OFF/USB-absent five seconds, ON/battery-first ten seconds, USB-reattach sequence. The exact right Mac greeting passed, and the left Mac diagnostic remained enumerated. Both halves are now running the Mac diagnostics. Owner-assisted brightness, Mission Control, Spotlight, playback/volume and Fn-function-row checks have been requested; no results are assumed. Independent review verified both installed readbacks and preserved readable ranges.

## Owner-assisted Mac row result

The owner reported that all requested Mac-row checks worked: external display brightness with Ctrl+F1/F2, Mission Control, Spotlight, playback/volume controls and Fn ordinary function keys. This is an owner-assisted functional pass, not raw HID capture, an exhaustive key sweep or numerical reliability validation. Keyboard backlight was explicitly excluded.

The owner asked why their Magic Keyboard changes display brightness without Ctrl. The current firmware emits standard consumer brightness usages `0x0070`/`0x006f`; [Apple documents Control+brightness for supported external displays](https://support.apple.com/en-us/102650). The exact Magic Keyboard report/driver-routing difference has not been captured, so Apple-specific routing remains an inference rather than a measured explanation. A future external-display preference could map F1/F2 to the already tested Ctrl+brightness chord using RMK modifiers; do not inject Ctrl into every display/profile without making that choice explicit.

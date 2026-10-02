# Mac function row candidate

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

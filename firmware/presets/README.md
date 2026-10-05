# Vial OS presets

These presets require RMK backlight conversion fix `8d07593e4f829ec165f08a2286dd516dc7113c21` (or a later build containing it). Older builds can lose backlight bindings during Vial import/export. The fix preserves the existing firmware tap-and-hold brightness behavior; it does not introduce a lighting settings panel.

Import one preset into the complete NocFree RMK keyboard using Vial **File → Load saved layout…**:

- `nocfree-mac.vil`: Mac brightness (F14/F15), Mission Control, Search, keyboard backlight and media keys on the top row. Hold Fn for F1–F12.
- `nocfree-windows-linux.vil`: F1–F12 on the top row. Hold Fn for display brightness on F1/F2, keyboard backlight on F5/F6 and media keys on F7–F12. Fn+F3 and Fn+F4 pass through as ordinary F3/F4.

Both preserve Fn on each half, Fn+1 through Fn+5 for Bluetooth profiles and Fn+0 to clear the current Bluetooth bond. The physical mode switch still selects Wired, Bluetooth or Dongle.

First save your current layout with **File → Save current layout…** if you want to keep any remappings. Importing replaces both layers. It does not replace firmware, change the keyboard UID, clear pairing, or set QMK settings, macros, tap dances or combos. Connect the left half by USB, or use the connected RMK dongle. The right half has no separate Vial keymap. Factory firmware cannot import these presets.

These files were derived from `firmware/src/keymap.rs` with backlight enabled and the pinned RMK HID values. The Windows/Linux preset deliberately omits the Mac-specific Mission Control action on Fn+F3, using transparent pass-through instead. They use Vial's version-1 saved-layout format and the existing UID interpreted little-endian. If that source keymap changes, update both presets and run `python3 firmware/presets/test_presets.py`. The format has been checked against [Vial v0.7.5 save/import code](https://github.com/vial-kb/vial-gui/blob/v0.7.5/src/main/python/protocol/keyboard_comm.py); device remapping and restart persistence require separate owner testing.

Vial v0.7.5 maps `customKeycodes` entries to `USER00`, `USER01`, etc., using protocol-6 `QK_KB` (`0x7e00`). That matches this RMK pin's User actions. Do not substitute the `0x7e40` example from the current custom-keycode guide. Mission Control is stored as raw `0xc1`, preserving RMK's existing consumer usage without assuming Vial has a named alias. The lighting panel remains disabled; `BL_DEC`/`BL_INC` are individual key actions, not lighting-panel settings.

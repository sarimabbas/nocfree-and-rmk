use nocfree_input::layout::{KEY_COUNT, LEFT_FN, RIGHT_FN, USAGES};
use rmk::types::action::{Action, KeyAction};
use rmk::{a, k, mo};
pub fn default_keymap() -> [[[KeyAction; KEY_COUNT]; 1]; 2] {
    use rmk::types::keycode::{HidKeyCode, KeyCode};
    let base = core::array::from_fn(|index| {
        if index == LEFT_FN || index == RIGHT_FN {
            mo!(1)
        } else {
            KeyAction::Single(Action::Key(KeyCode::Hid(HidKeyCode::from(USAGES[index]))))
        }
    });
    let mut function = [a!(Transparent); KEY_COUNT];
    // Native RMK recovery runs on LEFT, including keys pressed on RIGHT.
    function[position(0x29)] = KeyAction::Single(Action::KeyboardControl(
        rmk::types::action::KeyboardAction::Bootloader,
    ));
    function[position(0x2b)] = KeyAction::Single(Action::User(10)); // Dongle profile; hold clears its bond.
    function[position(0x2c)] = KeyAction::Single(Action::User(8)); // Toggle USB/BLE preference.
    function[1] = k!(BrightnessDown);
    function[2] = k!(BrightnessUp);
    function[3] = k!(MissionControl);
    #[cfg(feature = "backlight")]
    {
        function[5] = KeyAction::Single(Action::User(11));
        function[6] = KeyAction::Single(Action::User(12));
    }
    function[position(0x40)] = k!(MediaPrevTrack);
    function[position(0x41)] = k!(MediaPlayPause);
    function[position(0x42)] = k!(MediaNextTrack);
    function[position(0x43)] = k!(AudioMute);
    function[position(0x44)] = k!(AudioVolDown);
    function[position(0x45)] = k!(AudioVolUp);
    function[8] = KeyAction::Single(Action::User(0));
    function[9] = KeyAction::Single(Action::User(1));
    function[10] = KeyAction::Single(Action::User(2));
    function[11] = KeyAction::Single(Action::User(3));
    function[12] = KeyAction::Single(Action::User(4));
    function[position(0x27)] = KeyAction::Single(Action::User(7)); // Clear current BLE bond (Fn+0).
    #[cfg(feature = "mac-keymap")]
    let base = {
        let mut base = base;
        // Match the captured NuPhy Mac brightness keys: F14/F15.
        base[1] = k!(F14);
        base[2] = k!(F15);
        base[3] = k!(MissionControl);
        base[4] = k!(WwwSearch);
        #[cfg(feature = "backlight")]
        {
            base[5] = KeyAction::Single(Action::User(11));
            base[6] = KeyAction::Single(Action::User(12));
        }
        base[position(0x40)] = k!(MediaPrevTrack);
        base[position(0x41)] = k!(MediaPlayPause);
        base[position(0x42)] = k!(MediaNextTrack);
        base[position(0x43)] = k!(AudioMute);
        base[position(0x44)] = k!(AudioVolDown);
        base[position(0x45)] = k!(AudioVolUp);
        function[1] = k!(F1);
        function[2] = k!(F2);
        function[3] = k!(F3);
        function[4] = k!(F4);
        function[5] = k!(F5);
        function[6] = k!(F6);
        function[position(0x40)] = k!(F7);
        function[position(0x41)] = k!(F8);
        function[position(0x42)] = k!(F9);
        function[position(0x43)] = k!(F10);
        function[position(0x44)] = k!(F11);
        function[position(0x45)] = k!(F12);
        base
    };
    [[base], [function]]
}

fn position(usage: u8) -> usize {
    USAGES
        .iter()
        .position(|&key| key == usage)
        .expect("default key exists in every physical layout")
}

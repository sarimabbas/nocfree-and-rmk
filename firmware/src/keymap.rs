use rmk::types::action::{Action, KeyAction};
use rmk::{a, k, mo};
// ANSI electrical order: 37 left keys, then 47 right keys.
pub fn default_keymap() -> [[[KeyAction; 84]; 1]; 2] {
    let base = [
        k!(Escape),
        k!(F1),
        k!(F2),
        k!(F3),
        k!(F4),
        k!(F5),
        k!(F6),
        k!(Grave),
        k!(Kc1),
        k!(Kc2),
        k!(Kc3),
        k!(Kc4),
        k!(Kc5),
        k!(Kc6),
        k!(Tab),
        k!(Q),
        k!(W),
        k!(E),
        k!(R),
        k!(T),
        k!(CapsLock),
        k!(A),
        k!(S),
        k!(D),
        k!(F),
        k!(G),
        k!(LShift),
        k!(Z),
        k!(X),
        k!(C),
        k!(V),
        k!(B),
        mo!(1),
        k!(LCtrl),
        k!(LAlt),
        k!(LGui),
        k!(Space),
        k!(F7),
        k!(F8),
        k!(F9),
        k!(F10),
        k!(F11),
        k!(F12),
        k!(PrintScreen),
        k!(Home),
        k!(Kc7),
        k!(Kc8),
        k!(Kc9),
        k!(Kc0),
        k!(Minus),
        k!(Equal),
        k!(Backspace),
        k!(PageUp),
        k!(Y),
        k!(U),
        k!(I),
        k!(O),
        k!(P),
        k!(LeftBracket),
        k!(RightBracket),
        k!(Backslash),
        k!(H),
        k!(J),
        k!(K),
        k!(L),
        k!(Semicolon),
        k!(Quote),
        k!(Enter),
        k!(Delete),
        k!(N),
        k!(M),
        k!(Comma),
        k!(Dot),
        k!(Slash),
        k!(RShift),
        k!(Up),
        k!(PageDown),
        k!(Space),
        k!(RGui),
        mo!(1),
        k!(RAlt),
        k!(Left),
        k!(Down),
        k!(Right),
    ];
    let mut function = [a!(Transparent); 84];
    // Recovery belongs to the local startup shim, not the typing keymap.
    function[1] = k!(BrightnessDown);
    function[2] = k!(BrightnessUp);
    function[3] = k!(MissionControl);
    #[cfg(feature = "backlight")]
    {
        use rmk::types::action::LightAction;
        function[5] = KeyAction::Single(Action::Light(LightAction::BacklightDown));
        function[6] = KeyAction::Single(Action::Light(LightAction::BacklightUp));
    }
    function[37] = k!(MediaPrevTrack);
    function[38] = k!(MediaPlayPause);
    function[39] = k!(MediaNextTrack);
    function[40] = k!(AudioMute);
    function[41] = k!(AudioVolDown);
    function[42] = k!(AudioVolUp);
    function[8] = KeyAction::Single(Action::User(0));
    function[9] = KeyAction::Single(Action::User(1));
    function[10] = KeyAction::Single(Action::User(2));
    function[11] = KeyAction::Single(Action::User(3));
    function[12] = KeyAction::Single(Action::User(4));
    function[48] = KeyAction::Single(Action::User(7)); // Clear current BLE bond (Fn+0).
    function[31] = KeyAction::Single(Action::User(8)); // Toggle USB/BLE preference (Fn+B).
    function[54] = KeyAction::Single(Action::User(10)); // Select/pair RMK receiver (Fn+U).
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
            use rmk::types::action::LightAction;
            base[5] = KeyAction::Single(Action::Light(LightAction::BacklightDown));
            base[6] = KeyAction::Single(Action::Light(LightAction::BacklightUp));
        }
        base[37] = k!(MediaPrevTrack);
        base[38] = k!(MediaPlayPause);
        base[39] = k!(MediaNextTrack);
        base[40] = k!(AudioMute);
        base[41] = k!(AudioVolDown);
        base[42] = k!(AudioVolUp);
        function[1] = k!(F1);
        function[2] = k!(F2);
        function[3] = k!(F3);
        function[4] = k!(F4);
        function[5] = k!(F5);
        function[6] = k!(F6);
        function[37] = k!(F7);
        function[38] = k!(F8);
        function[39] = k!(F9);
        function[40] = k!(F10);
        function[41] = k!(F11);
        function[42] = k!(F12);
        base
    };
    [[base], [function]]
}

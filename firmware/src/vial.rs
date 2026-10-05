//! One secure Vial definition for the complete split keyboard.
// Stable community keyboard ID, independent of factory firmware identifiers.
const KEYBOARD_ID: [u8; 8] = [0x9d, 0x60, 0x23, 0x2a, 0x87, 0xc4, 0x1e, 0xb5];
const LAYOUT_ID: [u8; 8] = {
    let mut id = KEYBOARD_ID;
    if cfg!(feature = "layout-iso") {
        id[7] = 0xb6;
    } else if cfg!(feature = "layout-jis") {
        id[7] = 0xb7;
    } else if cfg!(feature = "layout-kr") {
        id[7] = 0xb8;
    }
    id
};
const DEFINITION: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/vial_definition.xz"));
pub fn config() -> rmk::config::VialConfig<'static> {
    // Left physical Fn + Shift; Vial controls the unlock hold/release policy.
    rmk::config::VialConfig::new(
        &LAYOUT_ID,
        DEFINITION,
        &[
            (0, nocfree_input::layout::LEFT_FN as u8),
            (0, nocfree_input::layout::LEFT_SHIFT as u8),
        ],
    )
}

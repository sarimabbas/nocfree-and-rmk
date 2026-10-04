//! One secure Vial definition for the complete split keyboard.
// Stable community keyboard ID, independent of factory firmware identifiers.
const KEYBOARD_ID: [u8; 8] = [0x9d, 0x60, 0x23, 0x2a, 0x87, 0xc4, 0x1e, 0xb5];
const DEFINITION: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/vial_definition.xz"));
pub fn config() -> rmk::config::VialConfig<'static> {
    // Left physical Fn + Shift; Vial controls the unlock hold/release policy.
    rmk::config::VialConfig::new(&KEYBOARD_ID, DEFINITION, &[(0, 32), (0, 26)])
}

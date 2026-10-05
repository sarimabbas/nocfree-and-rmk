//! Physical rows and HID usages from jhkim0218/Nocfree-and-ZMK-rust.
//! Pinned at 5b0fefcff9af3cc4876bb420f86f6b471eed55ba; MIT, copyright 2026 jhkim0218.
#[cfg(any(
    all(feature = "layout-iso", feature = "layout-jis"),
    all(feature = "layout-iso", feature = "layout-kr"),
    all(feature = "layout-jis", feature = "layout-kr")
))]
compile_error!("Select only one physical layout");
#[cfg(feature = "layout-iso")]
#[path = "layout/iso.rs"]
mod selected;
#[cfg(feature = "layout-jis")]
#[path = "layout/jis.rs"]
mod selected;
#[cfg(feature = "layout-kr")]
#[path = "layout/kr.rs"]
mod selected;
#[cfg(not(any(feature = "layout-iso", feature = "layout-jis", feature = "layout-kr")))]
#[path = "layout/ansi.rs"]
mod selected;
pub use selected::*;
pub const KEY_COUNT: usize = LEFT_COUNT + RIGHT_COUNT;
pub const LEFT_BITS: [u8; LEFT_COUNT] = scan_bits(LEFT_ROWS);
pub const RIGHT_BITS: [u8; RIGHT_COUNT] = scan_bits(RIGHT_ROWS);
const fn scan_bits<const N: usize>(rows: [u8; 6]) -> [u8; N] {
    let mut bits = [0; N];
    let mut index = 0;
    let mut row = 0;
    while row < 6 {
        let mut col = 0;
        while col < rows[row] {
            bits[index] = row as u8 * 8 + col;
            index += 1;
            col += 1;
        }
        row += 1;
    }
    while index < N {
        // KR right's four extra inputs are 0x21 input port 0, bits 0..3.
        bits[index] = 48 + (index - 46) as u8;
        index += 1;
    }
    bits
}

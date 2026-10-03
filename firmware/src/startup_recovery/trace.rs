//! Feature-gated diagnostic; every word is initialized before any read.
use super::nrf::gate::Outcome;
use core::{
    mem::MaybeUninit,
    ptr::{addr_of_mut, read_volatile, write_volatile},
};
const MAGIC: u32 = 0x53484d31;
#[cfg_attr(not(test), unsafe(link_section = ".uninit.startup_trace"))]
static mut TRACE: MaybeUninit<[u32; 12]> = MaybeUninit::uninit();
fn base() -> *mut u32 {
    addr_of_mut!(TRACE).cast()
}
unsafe fn put(index: usize, value: u32) {
    unsafe { write_volatile(base().add(index), value) }
}
unsafe fn get(index: usize) -> u32 {
    unsafe { read_volatile(base().add(index)) }
}
/// Must run at the first pre-init entry, before all diagnostic branches.
pub unsafe fn initialize(reset: u32, usb: u32, enable: u32) {
    unsafe {
        for word in 0..12 {
            put(word, 0);
        }
        put(1, Outcome::Entered as u32);
        put(2, reset);
        put(3, usb);
        put(4, enable);
        put(0, MAGIC);
    }
}
pub fn outcome(reason: Outcome) {
    unsafe { put(1, reason as u32) }
}
pub fn snapshot(bits: u64) {
    unsafe {
        put(5, bits as u32);
        put(6, (bits >> 32) as u32);
        put(7, 1);
    }
}
pub fn transfer_error(address: u8, error: u32, tx: u32, rx: u32) {
    unsafe {
        put(8, address as u32);
        put(9, error);
        put(10, tx);
        put(11, rx);
    }
}
fn label(reason: u32, error: u32) -> &'static str {
    match reason {
        1 => "boot:entered",
        2 => "boot:noUSB",
        3 => "boot:owned",
        4 if error & 6 != 0 => "boot:initNACK",
        4 => "boot:initFail",
        5 if error & 6 != 0 => "boot:scanNACK",
        5 => "boot:scanFail",
        6 => "boot:noFn",
        7 => "boot:noKey",
        8 => "boot:noChord",
        9 => "boot:released",
        10 => "boot:usbLost",
        11 => "boot:deadline",
        12 => "boot:timerFail",
        13 => "boot:held",
        14 => "boot:stopFail",
        _ => "boot:unknown",
    }
}
/// Called after runtime initialization; this image always ran initialize first.
pub fn manufacturer() -> &'static str {
    unsafe {
        if get(0) == MAGIC {
            label(get(1), get(9))
        } else {
            "boot:unknown"
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_are_short_and_fault_sensitive() {
        for reason in 0..=15 {
            for error in [0, 1, 2, 4, 7] {
                assert!(label(reason, error).len() <= 20);
            }
        }
        assert_eq!(label(4, 0), label(4, 1)); // OVERRUN alone is not NACK.
        assert_ne!(label(4, 0), label(4, 2));
        assert_ne!(label(5, 0), label(5, 4));
    }
    #[test]
    fn initialized_trace_survives_all_handoff_updates() {
        unsafe {
            initialize(0x1234, 3, 0);
        }
        assert_eq!(manufacturer(), "boot:entered");
        snapshot((1 << 40) | 1);
        transfer_error(0x22, 2, 1, 0);
        outcome(Outcome::ScanFailed);
        unsafe {
            assert_eq!(get(0), MAGIC);
            assert_eq!([get(2), get(3), get(4)], [0x1234, 3, 0]);
            assert_eq!([get(5), get(6), get(7)], [1, 256, 1]);
            assert_eq!([get(8), get(9), get(10), get(11)], [0x22, 2, 1, 0]);
        }
        assert_eq!(manufacturer(), "boot:scanNACK");
    }
}

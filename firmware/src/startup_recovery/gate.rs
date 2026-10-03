//! Stack-only policy, callable before Rust data/BSS initialization.
#[derive(Clone, Copy)]
pub enum Role {
    Left,
    Right,
}
pub trait Inputs {
    fn usb(&self) -> bool;
    fn now_us(&mut self) -> u32;
    fn initialize(&mut self) -> bool;
    fn snapshot(&mut self) -> Option<u64>;
    fn wait_us(&mut self, us: u32) -> bool;
}
/// Errors and incomplete snapshots always launch the application.
pub fn held<I: Inputs>(io: &mut I, role: Role) -> bool {
    if !io.usb() {
        return false;
    }
    let deadline = io.now_us();
    if !io.initialize() {
        return false;
    }
    let chord = match role {
        Role::Left => (1 << 40) | 1,
        Role::Right => (1 << 42) | (1 << 14),
    };
    let start = io.now_us();
    // The cap also terminates with a broken/stalled timer.
    for _ in 0..128 {
        if !io.usb() || io.now_us().wrapping_sub(deadline) >= 200_000 {
            return false;
        }
        match io.snapshot() {
            Some(bits) if bits & chord == chord => (),
            _ => return false,
        }
        if io.now_us().wrapping_sub(start) >= 60_000 {
            return true;
        }
        if !io.wait_us(1_000) {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Bus {
        time: u32,
        bits: u64,
        scans: usize,
        fail_at: usize,
        release_at: usize,
        usb: bool,
        unplug_at: usize,
        init: bool,
        frozen: bool,
    }
    impl Inputs for Bus {
        fn usb(&self) -> bool {
            self.usb && self.scans < self.unplug_at
        }
        fn now_us(&mut self) -> u32 {
            self.time
        }
        fn initialize(&mut self) -> bool {
            self.init
        }
        fn snapshot(&mut self) -> Option<u64> {
            self.scans += 1;
            if !self.frozen {
                self.time = self.time.wrapping_add(1_500);
            }
            if self.scans == self.fail_at {
                None
            } else if self.scans >= self.release_at {
                Some(0)
            } else {
                Some(self.bits)
            }
        }
        fn wait_us(&mut self, us: u32) -> bool {
            if !self.frozen {
                self.time = self.time.wrapping_add(us);
            }
            true
        }
    }
    fn bus(bits: u64) -> Bus {
        Bus {
            time: 0,
            bits,
            scans: 0,
            fail_at: usize::MAX,
            release_at: usize::MAX,
            usb: true,
            unplug_at: usize::MAX,
            init: true,
            frozen: false,
        }
    }
    #[test]
    fn both_local_chords() {
        assert!(held(&mut bus((1 << 40) | 1), Role::Left));
        assert!(held(&mut bus((1 << 42) | (1 << 14)), Role::Right));
        assert!(!held(&mut bus((1 << 40) | 1), Role::Right));
    }
    #[test]
    fn ordinary_startup_does_not_wait_for_hold() {
        let mut b = bus(0);
        assert!(!held(&mut b, Role::Left));
        assert_eq!(b.scans, 1);
        b.usb = false;
        b.scans = 0;
        assert!(!held(&mut b, Role::Left));
        assert_eq!(b.scans, 0);
    }
    #[test]
    fn every_snapshot_failure_rejects_recovery() {
        for fail in 1..=25 {
            let mut b = bus((1 << 40) | 1);
            b.fail_at = fail;
            assert!(!held(&mut b, Role::Left));
        }
    }
    #[test]
    fn release_and_init_error_reject() {
        let mut b = bus((1 << 40) | 1);
        b.release_at = 15;
        assert!(!held(&mut b, Role::Left));
        b = bus((1 << 40) | 1);
        b.init = false;
        assert!(!held(&mut b, Role::Left));
        assert_eq!(b.scans, 0);
    }
    #[test]
    fn stalled_clock_terminates() {
        let mut b = bus((1 << 40) | 1);
        b.frozen = true;
        assert!(!held(&mut b, Role::Left));
        assert!(b.scans <= 128);
    }
    #[test]
    fn unplug_during_hold_rejects() {
        let mut b = bus((1 << 40) | 1);
        b.unplug_at = 10;
        assert!(!held(&mut b, Role::Left));
        assert_eq!(b.scans, 10);
    }
    #[test]
    fn wraparound_and_other_keys() {
        let mut b = bus(u64::MAX);
        b.time = u32::MAX - 20_000;
        assert!(held(&mut b, Role::Left));
    }
}

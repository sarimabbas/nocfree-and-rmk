//! Stack-only policy, callable before Rust data/BSS initialization.
#[derive(Clone, Copy)]
pub enum Role {
    Left,
    Right,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Outcome {
    Entered = 1,
    NoUsb,
    PeripheralOwned,
    InitFailed,
    ScanFailed,
    MissingFn,
    MissingOther,
    MissingBoth,
    Released,
    UsbLost,
    Deadline,
    TimerFailed,
    Held,
    StopFailed,
}
pub trait Inputs {
    fn usb(&self) -> bool;
    fn now_us(&mut self) -> u32;
    fn initialize(&mut self) -> bool;
    fn snapshot(&mut self) -> Option<u64>;
    fn wait_us(&mut self, us: u32) -> bool;
    fn outcome(&mut self, _outcome: Outcome) {}
    fn first_snapshot(&mut self, _bits: u64) {}
}
/// Errors and incomplete snapshots always launch the application.
pub fn held<I: Inputs>(io: &mut I, role: Role) -> bool {
    if !io.usb() {
        io.outcome(Outcome::NoUsb);
        return false;
    }
    let deadline = io.now_us();
    if !io.initialize() {
        io.outcome(Outcome::InitFailed);
        return false;
    }
    let chord = match role {
        Role::Left => (1 << 40) | 1,
        Role::Right => (1 << 42) | (1 << 14),
    };
    let start = io.now_us();
    // The cap also terminates with a broken/stalled timer.
    for scan in 0..128 {
        if !io.usb() {
            io.outcome(Outcome::UsbLost);
            return false;
        }
        if io.now_us().wrapping_sub(deadline) >= 200_000 {
            io.outcome(Outcome::Deadline);
            return false;
        }
        match io.snapshot() {
            Some(bits) => {
                if scan == 0 {
                    io.first_snapshot(bits);
                }
                if bits & chord != chord {
                    let fn_bit = match role {
                        Role::Left => 40,
                        Role::Right => 42,
                    };
                    let other_bit = match role {
                        Role::Left => 0,
                        Role::Right => 14,
                    };
                    io.outcome(if scan != 0 {
                        Outcome::Released
                    } else if bits & (1 << fn_bit) == 0 && bits & (1 << other_bit) == 0 {
                        Outcome::MissingBoth
                    } else if bits & (1 << fn_bit) == 0 {
                        Outcome::MissingFn
                    } else {
                        Outcome::MissingOther
                    });
                    return false;
                }
            }
            None => {
                io.outcome(Outcome::ScanFailed);
                return false;
            }
        }
        if io.now_us().wrapping_sub(start) >= 60_000 {
            io.outcome(Outcome::Held);
            return true;
        }
        if !io.wait_us(1_000) {
            io.outcome(Outcome::TimerFailed);
            return false;
        }
    }
    io.outcome(Outcome::TimerFailed);
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
        reason: Option<Outcome>,
        first: Option<u64>,
    }
    impl Inputs for Bus {
        fn outcome(&mut self, reason: Outcome) {
            self.reason = Some(reason);
        }
        fn first_snapshot(&mut self, bits: u64) {
            self.first = Some(bits);
        }
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
            reason: None,
            first: None,
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
    #[test]
    fn distinguishes_diagnostic_failures_without_changing_policy() {
        for (bits, reason) in [
            (0, Outcome::MissingBoth),
            (1, Outcome::MissingFn),
            (1 << 40, Outcome::MissingOther),
        ] {
            let mut b = bus(bits);
            assert!(!held(&mut b, Role::Left));
            assert_eq!(b.reason, Some(reason));
            assert_eq!(b.first, Some(bits));
        }
        let mut b = bus((1 << 40) | 1);
        b.fail_at = 1;
        assert!(!held(&mut b, Role::Left));
        assert_eq!(b.reason, Some(Outcome::ScanFailed));
        assert_eq!(b.first, None);
        b = bus((1 << 40) | 1);
        b.release_at = 2;
        assert!(!held(&mut b, Role::Left));
        assert_eq!(b.reason, Some(Outcome::Released));
        assert_eq!(b.first, Some((1 << 40) | 1));
        b = bus((1 << 40) | 1);
        b.init = false;
        assert!(!held(&mut b, Role::Left));
        assert_eq!(b.reason, Some(Outcome::InitFailed));
        b = bus((1 << 40) | 1);
        b.usb = false;
        assert!(!held(&mut b, Role::Left));
        assert_eq!(b.reason, Some(Outcome::NoUsb));
    }
}

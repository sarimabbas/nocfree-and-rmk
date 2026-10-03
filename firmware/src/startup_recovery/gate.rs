//! Stack-only policy, callable before Rust data/BSS initialization.
#[derive(Clone, Copy)]
pub enum Role {
    Left,
    Right,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Outcome {
    // Values 2 and 10 were USB gate outcomes in the first diagnostic image.
    // Keep other values stable when reading previously saved trace records.
    Entered = 1,
    PeripheralOwned = 3,
    InitFailed,
    ScanFailed,
    MissingFn,
    MissingOther,
    MissingBoth,
    Released,
    Deadline = 11,
    TimerFailed,
    Held,
    StopFailed,
}
pub trait Inputs {
    fn now_us(&mut self) -> u32;
    fn initialize(&mut self) -> bool;
    fn snapshot(&mut self) -> Option<u64>;
    fn wait_us(&mut self, us: u32) -> bool;
    fn outcome(&mut self, _outcome: Outcome) {}
    fn first_snapshot(&mut self, _bits: u64) {}
}
/// Rejected chords and incomplete snapshots return to the application.
/// An adapter unable to stop an active DMA transfer must reset without returning.
pub fn held<I: Inputs>(io: &mut I, role: Role) -> bool {
    let deadline = io.now_us();
    if !io.initialize() {
        io.outcome(Outcome::InitFailed);
        return false;
    }
    let chord = match role {
        Role::Left => (1 << 40) | (1 << 32),
        Role::Right => (1 << 42) | (1 << 37),
    };
    let start = io.now_us();
    // The cap also terminates with a broken/stalled timer.
    for scan in 0..128 {
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
                        Role::Left => 32,
                        Role::Right => 37,
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
        init: bool,
        init_us: u32,
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
        fn now_us(&mut self) -> u32 {
            self.time
        }
        fn initialize(&mut self) -> bool {
            self.time = self.time.wrapping_add(self.init_us);
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
            init: true,
            init_us: 0,
            frozen: false,
            reason: None,
            first: None,
        }
    }
    #[test]
    fn both_local_chords() {
        assert!(held(&mut bus((1 << 40) | (1 << 32)), Role::Left));
        assert!(held(&mut bus((1 << 42) | (1 << 37)), Role::Right));
        assert!(!held(&mut bus((1 << 40) | (1 << 32)), Role::Right));
    }
    #[test]
    fn only_the_same_halfs_shift_and_fn_recover() {
        for (role, bits) in [
            (Role::Left, (1 << 40) | 1),          // Prior Escape chord.
            (Role::Right, (1 << 42) | (1 << 14)), // Prior Backspace chord.
            (Role::Left, 1 << 40),
            (Role::Left, 1 << 32),
            (Role::Right, 1 << 42),
            (Role::Right, 1 << 37),
            (Role::Left, (1 << 40) | (1 << 37)),
            (Role::Right, (1 << 42) | (1 << 32)),
        ] {
            let mut b = bus(bits);
            assert!(!held(&mut b, role));
            assert_eq!(b.scans, 1);
        }
    }
    #[test]
    fn right_local_chord_rejects_release_and_bus_faults() {
        for fail in 1..=25 {
            let mut b = bus((1 << 42) | (1 << 37));
            b.fail_at = fail;
            assert!(!held(&mut b, Role::Right));
        }
        let mut b = bus((1 << 42) | (1 << 37));
        b.release_at = 2;
        assert!(!held(&mut b, Role::Right));
        assert_eq!(b.reason, Some(Outcome::Released));
        b = bus((1 << 42) | (1 << 37));
        b.init = false;
        assert!(!held(&mut b, Role::Right));
        assert_eq!(b.scans, 0);
    }
    #[test]
    fn ordinary_startup_does_not_wait_for_hold() {
        let mut b = bus(0);
        assert!(!held(&mut b, Role::Left));
        assert_eq!(b.scans, 1);
    }
    #[test]
    fn every_snapshot_failure_rejects_recovery() {
        for fail in 1..=25 {
            let mut b = bus((1 << 40) | (1 << 32));
            b.fail_at = fail;
            assert!(!held(&mut b, Role::Left));
        }
    }
    #[test]
    fn release_and_init_error_reject() {
        let mut b = bus((1 << 40) | (1 << 32));
        b.release_at = 15;
        assert!(!held(&mut b, Role::Left));
        b = bus((1 << 40) | (1 << 32));
        b.init = false;
        assert!(!held(&mut b, Role::Left));
        assert_eq!(b.scans, 0);
    }
    #[test]
    fn stalled_clock_terminates() {
        let mut b = bus((1 << 40) | (1 << 32));
        b.frozen = true;
        assert!(!held(&mut b, Role::Left));
        assert!(b.scans <= 128);
    }
    #[test]
    fn initialization_time_counts_toward_deadline() {
        let mut b = bus((1 << 40) | (1 << 32));
        b.init_us = 200_000;
        assert!(!held(&mut b, Role::Left));
        assert_eq!(b.scans, 0);
        assert_eq!(b.reason, Some(Outcome::Deadline));
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
            (1 << 32, Outcome::MissingFn),
            (1 << 40, Outcome::MissingOther),
        ] {
            let mut b = bus(bits);
            assert!(!held(&mut b, Role::Left));
            assert_eq!(b.reason, Some(reason));
            assert_eq!(b.first, Some(bits));
        }
        let mut b = bus((1 << 40) | (1 << 32));
        b.fail_at = 1;
        assert!(!held(&mut b, Role::Left));
        assert_eq!(b.reason, Some(Outcome::ScanFailed));
        assert_eq!(b.first, None);
        b = bus((1 << 40) | (1 << 32));
        b.release_at = 2;
        assert!(!held(&mut b, Role::Left));
        assert_eq!(b.reason, Some(Outcome::Released));
        assert_eq!(b.first, Some((1 << 40) | (1 << 32)));
        b = bus((1 << 40) | (1 << 32));
        b.init = false;
        assert!(!held(&mut b, Role::Left));
        assert_eq!(b.reason, Some(Outcome::InitFailed));
    }
}

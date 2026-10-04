//! Register-only watchdog recovery policy; no initialized statics or runtime services.
pub const RESETREAS: usize = 0x40000400;
pub const GPREGRET: usize = 0x4000051c;
pub const DOG: u32 = 1 << 1;
const WDT: usize = 0x40010000;

pub trait Registers {
    fn read(&mut self, address: usize) -> u32;
    fn write(&mut self, address: usize, value: u32);
}
/// W1C only the watchdog flag, then request the existing bootloader route.
/// Caller must follow a true result immediately with DSB and system reset.
pub fn request_after_watchdog(registers: &mut impl Registers) -> bool {
    if registers.read(RESETREAS) & DOG == 0 {
        return false;
    }
    registers.write(RESETREAS, DOG);
    registers.write(GPREGRET, 0x57);
    true
}
/// No feeds: this proof deliberately lets a startup hang expire.
/// Never attempt to reconfigure a watchdog inherited from an earlier stage.
pub fn arm_probe(registers: &mut impl Registers) {
    if registers.read(WDT + 0x400) != 0 {
        return;
    }
    registers.write(WDT + 0x308, 1); // INTENCLR.TIMEOUT; NVIC is untouched.
    registers.write(WDT + 0x504, 65_536); // (CRV+1)/32768 ~= two seconds.
    registers.write(WDT + 0x508, 1); // Reload channel 0, never written by this probe.
    registers.write(WDT + 0x50c, 1); // SLEEP=Run, HALT=Pause.
    registers.write(WDT, 1); // TASKS_START, after all configuration.
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        reason: u32,
        running: u32,
        writes: Vec<(usize, u32)>,
    }
    impl Registers for Fake {
        fn read(&mut self, address: usize) -> u32 {
            match address {
                RESETREAS => self.reason,
                value if value == WDT + 0x400 => self.running,
                _ => panic!("unexpected register read"),
            }
        }
        fn write(&mut self, address: usize, value: u32) {
            self.writes.push((address, value));
            if address == RESETREAS {
                self.reason &= !value;
            }
        }
    }
    #[test]
    fn watchdog_conversion_clears_only_dog_then_sets_request_without_looping() {
        let mut registers = Fake {
            reason: 0x10007,
            running: 0,
            writes: vec![],
        };
        assert!(request_after_watchdog(&mut registers));
        assert_eq!(registers.writes, [(RESETREAS, DOG), (GPREGRET, 0x57)]);
        assert_eq!(registers.reason, 0x10005);
        registers.writes.clear();
        assert!(!request_after_watchdog(&mut registers));
        assert!(registers.writes.is_empty());
    }
    #[test]
    fn proof_never_feeds_or_reconfigures_a_running_watchdog() {
        let mut registers = Fake {
            reason: 0,
            running: 0,
            writes: vec![],
        };
        assert!(!request_after_watchdog(&mut registers));
        arm_probe(&mut registers);
        assert_eq!(
            registers.writes,
            [
                (WDT + 0x308, 1),
                (WDT + 0x504, 65_536),
                (WDT + 0x508, 1),
                (WDT + 0x50c, 1),
                (WDT, 1)
            ]
        );
        registers.running = 1;
        registers.writes.clear();
        arm_probe(&mut registers);
        assert!(registers.writes.is_empty());
    }
}

//! Register-only watchdog recovery policy; no initialized statics or runtime services.
pub const RESETREAS: usize = 0x40000400;
pub const GPREGRET: usize = 0x4000051c;
pub const DOG: u32 = 1 << 1;
pub const RESETPIN: u32 = 1;
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
/// Match RMK Nrf52Watchdog::default_runner exactly, so Embassy can adopt it.
#[cfg(any(feature = "startup-watchdog", test))]
pub fn arm_startup(registers: &mut impl Registers) {
    arm(registers, 327_680);
}
fn arm(registers: &mut impl Registers, timeout_ticks: u32) {
    // This factory bootloader's SystemInit clears other reset reasons when
    // RESETPIN remains latched. Clear only that stale pin flag before a DOG reset.
    registers.write(RESETREAS, RESETPIN);
    if registers.read(WDT + 0x400) != 0 {
        return;
    }
    registers.write(WDT + 0x308, 1); // INTENCLR.TIMEOUT; NVIC is untouched.
    registers.write(WDT + 0x504, timeout_ticks); // (CRV+1)/32768 seconds.
    registers.write(WDT + 0x508, 1); // Enable reload channel 0; feeding belongs to the caller/framework.
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
    fn startup_policy_matches_framework_adoption_and_preserves_unrelated_flags() {
        let mut registers = Fake {
            reason: 0x10005,
            running: 0,
            writes: vec![],
        };
        arm_startup(&mut registers);
        assert_eq!(registers.reason, 0x10004);
        assert_eq!(
            registers.writes,
            [
                (RESETREAS, RESETPIN),
                (WDT + 0x308, 1),
                (WDT + 0x504, 327_680),
                (WDT + 0x508, 1),
                (WDT + 0x50c, 1),
                (WDT, 1)
            ]
        );
    }
    #[test]
    fn startup_does_not_reconfigure_a_running_watchdog() {
        let mut registers = Fake {
            reason: 0x10005,
            running: 0,
            writes: vec![],
        };
        assert!(!request_after_watchdog(&mut registers));
        arm_startup(&mut registers);
        assert_eq!(
            registers.writes,
            [
                (RESETREAS, RESETPIN),
                (WDT + 0x308, 1),
                (WDT + 0x504, 327_680),
                (WDT + 0x508, 1),
                (WDT + 0x50c, 1),
                (WDT, 1)
            ]
        );
        assert_eq!(registers.reason, 0x10004);
        registers.running = 1;
        registers.writes.clear();
        arm_startup(&mut registers);
        assert_eq!(registers.writes, [(RESETREAS, RESETPIN)]);
    }
}

// This pre-init hook runs before runtime RAM initialization. Its raw adapter
// accesses registers only; ordinary feeding remains RMK's responsibility.
#[cfg(all(feature = "startup-watchdog", not(test)))]
mod startup {
    use super::{Registers, arm_startup, request_after_watchdog};
    struct Raw;
    impl Registers for Raw {
        fn read(&mut self, address: usize) -> u32 {
            unsafe { core::ptr::read_volatile(address as *const u32) }
        }
        fn write(&mut self, address: usize, value: u32) {
            unsafe { core::ptr::write_volatile(address as *mut u32, value) }
        }
    }
    core::arch::global_asm!(
        ".pushsection .text.__pre_init,\"ax\",%progbits",
        ".balign 2",
        ".global __pre_init",
        ".type __pre_init,%function",
        ".thumb_func",
        "__pre_init:",
        "push {{r4, lr}}",
        "bl nocfree_watchdog_startup",
        "pop {{r4, pc}}",
        ".size __pre_init, .-__pre_init",
        ".popsection",
    );
    #[unsafe(no_mangle)]
    unsafe extern "C" fn nocfree_watchdog_startup() {
        let mut registers = Raw;
        if request_after_watchdog(&mut registers) {
            cortex_m::asm::dsb();
            cortex_m::peripheral::SCB::sys_reset();
        }
        arm_startup(&mut registers);
    }
}

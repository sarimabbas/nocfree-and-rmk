#![no_std]
#![no_main]

#[cfg(any(not(feature = "left"), feature = "right", feature = "receiver"))]
compile_error!("Watchdog rescue proof requires only LEFT");
#[cfg(any(
    not(feature = "reclaimed-softdevice"),
    feature = "usb-recovery-first",
    feature = "application-recovery-shim"
))]
compile_error!(
    "Watchdog proof requires lower layout and erased startup reserve, without another entry hook"
);

// Link the existing nRF52833 interrupt-vector table without initializing its HAL.
use nrf_pac as _;

#[path = "../watchdog_recovery.rs"]
mod recovery;
struct Raw;
impl recovery::Registers for Raw {
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
    "bl nocfree_watchdog_entry",
    "pop {{r4, pc}}",
    ".size __pre_init, .-__pre_init",
    ".popsection",
);
#[unsafe(no_mangle)]
unsafe extern "C" fn nocfree_watchdog_entry() {
    let mut registers = Raw;
    if recovery::request_after_watchdog(&mut registers) {
        cortex_m::asm::dsb();
        cortex_m::peripheral::SCB::sys_reset();
    }
    recovery::arm_probe(&mut registers);
}
#[cortex_m_rt::entry]
fn main() -> ! {
    // Deliberately simulate stuck startup. No HAL, USB, RMK, timers or feeds.
    loop {
        cortex_m::asm::nop();
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        cortex_m::asm::nop();
    }
}

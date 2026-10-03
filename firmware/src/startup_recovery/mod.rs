//! Application entry only; RMK owns all subsequent keyboard behavior.
#[cfg(any(feature = "receiver", feature = "usb-recovery-first"))]
compile_error!(
    "The application recovery shim requires a keyboard half without the USB-first marker"
);

mod nrf;
#[cfg(feature = "startup-recovery-diagnostic")]
mod trace;
#[cfg(feature = "startup-recovery-diagnostic")]
pub fn diagnostic_manufacturer() -> &'static str {
    trace::manufacturer()
}

// The hook runs before data/BSS initialization. Save the incoming interrupt mask
// and keep interrupts masked while the stack-only scanner owns TWIM0/TIMER1.
core::arch::global_asm!(
    ".pushsection .text.__pre_init,\"ax\",%progbits",
    ".balign 2",
    ".global __pre_init",
    ".type __pre_init,%function",
    ".thumb_func",
    "__pre_init:",
    "push {{r4, lr}}",
    "mrs r4, PRIMASK",
    "cpsid i",
    "bl nocfree_startup_recovery",
    "msr PRIMASK, r4",
    "pop {{r4, pc}}",
    ".size __pre_init, .-__pre_init",
    ".popsection",
);

#[unsafe(no_mangle)]
unsafe extern "C" fn nocfree_startup_recovery() {
    unsafe { nrf::check(cfg!(feature = "right")) };
}

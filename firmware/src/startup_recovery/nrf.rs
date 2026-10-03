//! nRF52833 polled TWIM0 adapter. No initialized statics, interrupts or UICR access.
use core::{
    ptr::{read_volatile, write_volatile},
    sync::atomic::{Ordering, compiler_fence},
};
#[path = "gate.rs"]
pub(super) mod gate;
use gate::{Inputs, Role};
const TWIM: usize = 0x40003000;
const TIMER: usize = 0x40009000;
const SDA_CNF: usize = 0x50000700 + 11 * 4;
const SCL_CNF: usize = 0x50000a00 + 9 * 4;
const LIMIT: usize = 200_000;
/// Never returns: DMA buffers in calling frames remain alive until reset.
unsafe fn factory_recovery() -> ! {
    unsafe {
        write(0x4000051c, 0x57);
        core::arch::asm!("dsb", options(nostack, preserves_flags));
        write(0xe000ed0c, (read(0xe000ed0c) & 0x700) | 0x05fa0004);
        core::arch::asm!("dsb", options(nostack, preserves_flags));
        loop {
            core::arch::asm!("nop", options(nostack, preserves_flags));
        }
    }
}
unsafe fn read(address: usize) -> u32 {
    unsafe { read_volatile(address as *const u32) }
}
unsafe fn write(address: usize, value: u32) {
    unsafe { write_volatile(address as *mut u32, value) }
}
struct Bus {
    tx: [u8; 3],
    rx: [u8; 2],
    pins: [u32; 2],
    active: bool,
}
impl Bus {
    unsafe fn begin(&mut self) -> bool {
        // Reject peripherals already owned by the preceding boot stage.
        if unsafe { read(TWIM + 0x500) } != 0 {
            return false;
        }
        self.pins = unsafe { [read(SDA_CNF), read(SCL_CNF)] };
        self.active = true;
        unsafe {
            write(0xe000e180, (1 << 3) | (1 << 9)); // ICER: TWIM0 and TIMER1.
            write(0xe000e280, (1 << 3) | (1 << 9)); // ICPR.
            write(TIMER + 0x004, 1);
            write(TIMER + 0x00c, 1);
            write(TIMER + 0x200, 0);
            write(TIMER + 0x308, u32::MAX);
            write(TIMER + 0x504, 0);
            write(TIMER + 0x508, 3);
            write(TIMER + 0x510, 4);
            write(TIMER, 1); // 1 MHz using the internal HF clock; no crystal wait.
            write(SDA_CNF, (3 << 2) | (6 << 8));
            write(SCL_CNF, (3 << 2) | (6 << 8));
            write(TWIM + 0x508, 41);
            write(TWIM + 0x50c, 11);
            write(TWIM + 0x524, 0x01980000);
            write(TWIM + 0x308, u32::MAX);
            write(TWIM + 0x540, 0);
            write(TWIM + 0x550, 0); // No DMA lists.
            write(TWIM + 0x500, 6);
        }
        true
    }
    fn ticks(&mut self) -> u32 {
        unsafe {
            write(TIMER + 0x040, 1);
            read(TIMER + 0x540)
        }
    }
    fn wait_stopped(&mut self, check_error: bool, us: u32) -> bool {
        let start = self.ticks();
        for _ in 0..LIMIT {
            if check_error && unsafe { read(TWIM + 0x124) } != 0 {
                return false;
            }
            if unsafe { read(TWIM + 0x104) } != 0 {
                return true;
            }
            if self.ticks().wrapping_sub(start) >= us {
                return false;
            }
        }
        false
    }
    fn transfer(&mut self, address: u8, len: usize, rx: bool) -> bool {
        unsafe {
            for event in [0x104, 0x124, 0x148, 0x14c, 0x150, 0x15c, 0x160] {
                write(TWIM + event, 0);
            }
            write(TWIM + 0x4c4, 7);
            write(TWIM + 0x588, address as u32);
            write(TWIM + 0x544, self.tx.as_ptr() as u32);
            write(TWIM + 0x548, len as u32);
            write(TWIM + 0x534, self.rx.as_mut_ptr() as u32);
            write(TWIM + 0x538, if rx { 2 } else { 0 });
            write(TWIM + 0x200, if rx { (1 << 7) | (1 << 12) } else { 1 << 9 });
            compiler_fence(Ordering::SeqCst);
            write(TWIM + 0x008, 1);
        }
        let ok = self.wait_stopped(true, 4_000);
        #[cfg(feature = "startup-recovery-diagnostic")]
        if !ok || unsafe { read(TWIM + 0x54c) != len as u32 || (rx && read(TWIM + 0x53c) != 2) } {
            super::trace::transfer_error(
                address,
                unsafe { read(TWIM + 0x4c4) },
                unsafe { read(TWIM + 0x54c) },
                unsafe { read(TWIM + 0x53c) },
            );
        }
        if !ok {
            unsafe {
                write(TWIM + 0x200, 0);
                write(TWIM + 0x020, 1);
                write(TWIM + 0x014, 1);
            }
            if !self.wait_stopped(false, 1_000) {
                #[cfg(feature = "startup-recovery-diagnostic")]
                super::trace::outcome(gate::Outcome::StopFailed);
                // Without STOPPED the peripheral may still own these buffers.
                // A DSB/ENABLE=0 is not proof of EasyDMA completion. Never
                // reuse or drop them: hardware reset is the bounded escape.
                unsafe { factory_recovery() }
            }
        }
        compiler_fence(Ordering::SeqCst);
        ok && unsafe { read(TWIM + 0x54c) == len as u32 && (!rx || read(TWIM + 0x53c) == 2) }
    }
    fn finish(&mut self) {
        if !self.active {
            return;
        }
        unsafe {
            write(TWIM + 0x500, 0);
            core::arch::asm!("dsb", options(nostack, preserves_flags));
            write(TWIM + 0x200, 0);
            write(TWIM + 0x308, u32::MAX);
            write(TWIM + 0x508, u32::MAX);
            write(TWIM + 0x50c, u32::MAX);
            write(TWIM + 0x534, 0);
            write(TWIM + 0x538, 0);
            write(TWIM + 0x544, 0);
            write(TWIM + 0x548, 0);
            write(SDA_CNF, self.pins[0]);
            write(SCL_CNF, self.pins[1]);
            write(TIMER + 0x004, 1);
            write(TIMER + 0x00c, 1);
            write(TIMER + 0x200, 0);
            write(TIMER + 0x308, u32::MAX);
            for event in [0x104, 0x124, 0x148, 0x14c, 0x150, 0x15c, 0x160] {
                write(TWIM + event, 0);
            }
            for event in [0x140, 0x144, 0x148, 0x14c] {
                write(TIMER + event, 0);
            }
            write(0xe000e280, (1 << 3) | (1 << 9));
        }
        self.active = false;
    }
}
impl Inputs for Bus {
    #[cfg(feature = "startup-recovery-diagnostic")]
    fn outcome(&mut self, reason: gate::Outcome) {
        super::trace::outcome(reason);
    }
    #[cfg(feature = "startup-recovery-diagnostic")]
    fn first_snapshot(&mut self, bits: u64) {
        super::trace::snapshot(bits);
    }
    fn now_us(&mut self) -> u32 {
        self.ticks()
    }
    fn initialize(&mut self) -> bool {
        for address in [0x20, 0x22, 0x24] {
            self.tx = [6, 255, 255];
            if !self.transfer(address, 3, false) {
                return false;
            }
            self.tx = [4, 0, 0];
            if !self.transfer(address, 3, false) {
                return false;
            }
        }
        true
    }
    fn snapshot(&mut self) -> Option<u64> {
        let mut bits = 0;
        for (index, address) in [0x20, 0x22, 0x24].into_iter().enumerate() {
            self.tx = [0, 0, 0];
            if !self.transfer(address, 1, true) {
                return None;
            }
            bits |= (!u16::from_le_bytes(self.rx) as u64) << (16 * index);
        }
        Some(bits)
    }
    fn wait_us(&mut self, us: u32) -> bool {
        let start = self.ticks();
        for _ in 0..LIMIT {
            if self.ticks().wrapping_sub(start) >= us {
                return true;
            }
        }
        false
    }
}
/// Caller must be before HAL/RMK initialization and before interrupts are enabled.
/// `right=false` is left Fn+Escape; `true` is right Fn+Backspace.
pub unsafe fn check(right: bool) {
    #[cfg(feature = "startup-recovery-diagnostic")]
    unsafe {
        super::trace::initialize(read(0x40000400), read(0x40000438), read(TWIM + 0x500));
    }
    let mut bus = Bus {
        tx: [0; 3],
        rx: [0; 2],
        pins: [0; 2],
        active: false,
    };
    if !unsafe { bus.begin() } {
        #[cfg(feature = "startup-recovery-diagnostic")]
        super::trace::outcome(gate::Outcome::PeripheralOwned);
        return;
    }
    let recover = gate::held(&mut bus, if right { Role::Right } else { Role::Left });
    bus.finish();
    if recover {
        unsafe { factory_recovery() }
    }
}

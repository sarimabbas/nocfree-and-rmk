//! Read the vendor's passive position-sense pins; RMK owns output selection.
use core::sync::atomic::{AtomicU8, Ordering};
use embassy_nrf::gpio::Input;
use embassy_time::Timer;
use rmk::{
    core_traits::Runnable,
    output_selection::{self, OutputSelection},
};
static LEVELS: AtomicU8 = AtomicU8::new(0);
pub fn levels() -> u8 {
    LEVELS.load(Ordering::Relaxed)
}
pub struct ModeSwitch<'a> {
    bluetooth: Input<'a>,
    receiver: Input<'a>,
}
impl<'a> ModeSwitch<'a> {
    pub fn new(bluetooth: Input<'a>, receiver: Input<'a>) -> Self {
        let this = Self {
            bluetooth,
            receiver,
        };
        output_selection::initialize(this.read().unwrap_or(OutputSelection::Off));
        this
    }
    fn read(&self) -> Option<OutputSelection> {
        let b = self.bluetooth.is_high();
        let r = self.receiver.is_high();
        LEVELS.store(
            1 | (u8::from(b) << 1) | (u8::from(r) << 2),
            Ordering::Relaxed,
        );
        match nocfree_input::switch_position(b, r) {
            Some(nocfree_input::SwitchPosition::Wired) => Some(OutputSelection::Wired),
            Some(nocfree_input::SwitchPosition::Bluetooth) => Some(OutputSelection::Bluetooth),
            Some(nocfree_input::SwitchPosition::Receiver) => Some(OutputSelection::Dongle),
            None => None,
        }
    }
}
impl Runnable for ModeSwitch<'_> {
    async fn run(&mut self) -> ! {
        let mut applied = None;
        // Routing was already restricted synchronously by the constructor.
        loop {
            let first = self.read();
            Timer::after_millis(25).await;
            if let Some(selection) = first.filter(|value| Some(*value) == self.read()) {
                if applied != Some(selection) {
                    output_selection::select(selection).await;
                    applied = Some(selection);
                }
            }
        }
    }
}

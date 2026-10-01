use embassy_time::{Duration, Timer};
use embedded_hal_async::i2c::I2c;
use nocfree_input::{Inputs, key_pressed};
use rmk::{
    core_traits::Runnable,
    debounce::{DebounceState, DebouncerTrait, default_debouncer::DefaultDebouncer},
    event::{KeyboardEvent, publish_event_async},
    matrix::KeyState,
};

/// Owns electrical scanning; RMK owns debounce and every key behavior.
pub struct Scanner<I, const N: usize> {
    inputs: Inputs<I>,
    bits: &'static [u8; N],
    states: [KeyState; N],
    debounce: DefaultDebouncer<1, N>,
}
impl<I: I2c, const N: usize> Scanner<I, N> {
    pub fn new(bus: I, bits: &'static [u8; N]) -> Self {
        Self {
            inputs: Inputs::new(bus),
            bits,
            states: [KeyState { pressed: false }; N],
            debounce: DefaultDebouncer::new(),
        }
    }
}
impl<I: I2c, const N: usize> Runnable for Scanner<I, N> {
    async fn run(&mut self) -> ! {
        while self.inputs.initialize().await.is_err() {
            defmt::error!("Expander initialization failed; retrying");
            Timer::after_millis(100).await;
        }
        loop {
            // 100 kHz conservative bus: ~1.5 ms wire time plus a 1 ms yield.
            // Never manufacture releases or commit a partial scan on an I2C failure.
            match self.inputs.snapshot().await {
                Ok(snapshot) => {
                    for col in 0..N {
                        let pressed = key_pressed(snapshot, self.bits[col]);
                        if matches!(
                            self.debounce.detect_change_with_debounce(
                                0,
                                col,
                                pressed,
                                &self.states[col]
                            ),
                            DebounceState::Debounced
                        ) {
                            self.states[col].pressed = pressed;
                            publish_event_async(KeyboardEvent::key(0, col as u8, pressed)).await;
                        }
                    }
                }
                Err(_) => defmt::warn!("Expander scan failed"),
            }
            Timer::after(Duration::from_millis(1)).await;
        }
    }
}

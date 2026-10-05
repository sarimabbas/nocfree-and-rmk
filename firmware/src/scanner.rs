use embassy_time::{Duration, Timer};
use embedded_hal_async::i2c::I2c;
use nocfree_input::{IdleGate, IdleRetry, IdleWake, Inputs, key_pressed};
use rmk::{
    core_traits::Runnable,
    debounce::{DebounceState, DebouncerTrait, default_debouncer::DefaultDebouncer},
    event::{KeyboardEvent, publish_event_async},
    matrix::KeyState,
};

pub struct NoInterrupt;
pub trait ScannerInterrupt {
    fn available(&self) -> bool;
    async fn wait(&mut self) -> Result<IdleWake, ()>;
}
impl ScannerInterrupt for NoInterrupt {
    fn available(&self) -> bool {
        false
    }
    async fn wait(&mut self) -> Result<IdleWake, ()> {
        Err(())
    }
}
#[cfg(feature = "async-scanner")]
pub struct InterruptPin<P>(P);
#[cfg(feature = "async-scanner")]
impl<P: embedded_hal::digital::InputPin + embedded_hal_async::digital::Wait> ScannerInterrupt
    for InterruptPin<P>
{
    fn available(&self) -> bool {
        true
    }
    async fn wait(&mut self) -> Result<IdleWake, ()> {
        nocfree_input::wait_for_interrupt(&mut self.0)
            .await
            .map_err(|_| ())
    }
}

/// Owns electrical scanning; RMK owns debounce and every key behavior.
pub struct Scanner<I, const N: usize, P = NoInterrupt> {
    inputs: Inputs<I>,
    bits: &'static [u8; N],
    states: [KeyState; N],
    debounce: DefaultDebouncer<1, N>,
    interrupt: P,
    idle: IdleGate,
    retry: IdleRetry,
}
impl<I: I2c, const N: usize> Scanner<I, N> {
    pub fn new(bus: I, bits: &'static [u8; N]) -> Self {
        Self::with_interrupt(bus, bits, NoInterrupt)
    }
    #[cfg(feature = "async-scanner")]
    pub fn new_with_interrupt<
        P: embedded_hal::digital::InputPin + embedded_hal_async::digital::Wait,
    >(
        bus: I,
        bits: &'static [u8; N],
        interrupt: P,
    ) -> Scanner<I, N, InterruptPin<P>> {
        Self::with_interrupt(bus, bits, InterruptPin(interrupt))
    }
    fn with_interrupt<P>(bus: I, bits: &'static [u8; N], interrupt: P) -> Scanner<I, N, P> {
        Scanner {
            inputs: if bits.iter().any(|&bit| bit >= 48) {
                Inputs::with_extra_port(bus)
            } else {
                Inputs::new(bus)
            },
            bits,
            states: [KeyState { pressed: false }; N],
            debounce: DefaultDebouncer::new(),
            interrupt,
            idle: IdleGate::default(),
            retry: IdleRetry::default(),
        }
    }
}
impl<I: I2c, const N: usize, P: ScannerInterrupt> Runnable for Scanner<I, N, P> {
    async fn run(&mut self) -> ! {
        while self.inputs.initialize().await.is_err() {
            defmt::error!("Expander initialization failed; retrying");
            Timer::after_millis(100).await;
        }
        loop {
            // 100 kHz conservative bus: ~1.5 ms wire time plus a 1 ms yield.
            // Never manufacture releases or commit a partial scan on an I2C failure.
            let snapshot = if self.interrupt.available() {
                self.inputs.snapshot_for_interrupt().await
            } else {
                self.inputs.snapshot().await
            };
            self.idle.begin_scan(snapshot.is_ok());
            match snapshot {
                Ok(snapshot) => {
                    for col in 0..N {
                        let pressed = key_pressed(snapshot, self.bits[col]);
                        let change = self.debounce.detect_change_with_debounce(
                            0,
                            col,
                            pressed,
                            &self.states[col],
                        );
                        if matches!(change, DebounceState::Debounced) {
                            self.states[col].pressed = pressed;
                            publish_event_async(KeyboardEvent::key(0, col as u8, pressed)).await;
                        }
                        self.idle.observe_key(
                            pressed,
                            self.states[col].pressed,
                            matches!(change, DebounceState::InProgress),
                        );
                    }
                }
                Err(_) => defmt::warn!("Expander scan failed"),
            }
            if self.interrupt.available() && self.idle.can_wait() {
                // This complete quiet scan clears all INT sources, parks command
                // pointers, then arms a level-sensitive wait. Any active/pending
                // key or failed read prevents waiting. Low/error IRQ retries are
                // delayed so unused inputs or a stuck wire cannot busy-loop.
                if self.retry.back_off(self.interrupt.wait().await) {
                    Timer::after_millis(10).await;
                }
            } else {
                self.retry.active();
            }
            Timer::after(Duration::from_millis(1)).await;
        }
    }
}

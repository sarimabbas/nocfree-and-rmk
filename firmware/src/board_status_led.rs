//! Board-owned blue indicator; RMK continues to own connection and sleep state.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pattern {
    Off,
    Blink,
    Connected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Indication {
    pattern: Pattern,
    profile: u8,
    started_ms: u64,
}

impl Indication {
    fn new() -> Self {
        Self {
            pattern: Pattern::Off,
            profile: 0,
            started_ms: 0,
        }
    }

    fn update(&mut self, pattern: Pattern, profile: u8, now_ms: u64) {
        if (self.pattern, self.profile) != (pattern, profile) {
            *self = Self {
                pattern,
                profile,
                started_ms: now_ms,
            };
        }
    }

    fn on(&self, now_ms: u64) -> bool {
        let elapsed = now_ms.saturating_sub(self.started_ms);
        match self.pattern {
            Pattern::Off => false,
            Pattern::Blink => (elapsed / 500).is_multiple_of(2),
            Pattern::Connected => elapsed < 30_000,
        }
    }
}

#[cfg(target_arch = "arm")]
pub struct StatusLed<P> {
    pin: P,
    active_low: bool,
}

#[cfg(target_arch = "arm")]
impl<P: embedded_hal::digital::OutputPin> StatusLed<P> {
    pub fn new(pin: P, active_low: bool) -> Self {
        Self { pin, active_low }
    }

    pub async fn run(&mut self) -> ! {
        use embassy_futures::select::{Either3, select3};
        use embassy_time::{Instant, Timer};
        use rmk::event::{
            ConnectionStatusChangeEvent, EventSubscriber, SleepStateEvent, SubscribableEvent,
        };
        use rmk::output_selection::OutputSelection;
        use rmk_types::ble::BleState;

        let mut connection = ConnectionStatusChangeEvent::subscriber();
        let mut sleep = SleepStateEvent::subscriber();
        let mut sleeping = false;
        let mut indication = Indication::new();
        let mut last = None;
        loop {
            let status = rmk::state::current_connection_status();
            let selected = match rmk::state::output_selection() {
                OutputSelection::Bluetooth => {
                    usize::from(status.ble.profile) < rmk_types::constants::NUM_BLE_PROFILE
                }
                OutputSelection::Dongle => {
                    usize::from(status.ble.profile) == rmk_types::constants::NUM_BLE_PROFILE
                }
                _ => false,
            };
            let pattern = match (selected && !sleeping, status.ble.state) {
                (true, BleState::Advertising) => Pattern::Blink,
                (true, BleState::Connected) => Pattern::Connected,
                _ => Pattern::Off,
            };
            let now = Instant::now().as_millis();
            indication.update(pattern, status.ble.profile, now);
            let on = indication.on(now);
            if last != Some(on) {
                if on != self.active_low {
                    self.pin.set_high().ok();
                } else {
                    self.pin.set_low().ok();
                }
                last = Some(on);
            }
            match select3(connection.next_event(), sleep.next_event(), async {
                match indication.pattern {
                    Pattern::Off => core::future::pending::<()>().await,
                    Pattern::Connected if !on => core::future::pending::<()>().await,
                    Pattern::Connected => {
                        Timer::at(Instant::from_millis(indication.started_ms + 30_000)).await
                    }
                    Pattern::Blink => Timer::after_millis(500).await,
                }
            })
            .await
            {
                Either3::Second(event) => sleeping = event.0,
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connected_expires_and_repeated_events_do_not_extend_it() {
        let mut light = Indication::new();
        light.update(Pattern::Connected, 1, 100);
        light.update(Pattern::Connected, 1, 20_000);
        assert!(light.on(30_099));
        assert!(!light.on(30_100));
        light.update(Pattern::Connected, 2, 31_000);
        assert!(light.on(31_000));
    }

    #[test]
    fn disconnect_or_sleep_cancels_and_reconnect_restarts() {
        let mut light = Indication::new();
        light.update(Pattern::Connected, 0, 0);
        light.update(Pattern::Off, 0, 100);
        assert!(!light.on(100));
        light.update(Pattern::Connected, 0, 200);
        assert!(light.on(200));
    }

    #[test]
    fn search_blinks_and_connection_resets_phase() {
        let mut light = Indication::new();
        light.update(Pattern::Blink, 0, 100);
        assert!(light.on(100));
        assert!(!light.on(600));
        assert!(light.on(1100));
        light.update(Pattern::Connected, 0, 1200);
        assert!(light.on(1200));
    }
}

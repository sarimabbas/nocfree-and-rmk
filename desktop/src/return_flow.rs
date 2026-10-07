//! Pure physical-return protocol; time and connection evidence arrive as events.
use statig::prelude::*;
use std::time::{Duration, Instant};
#[derive(Clone, Copy, Debug)]
pub enum Phase {
    Disconnect,
    PowerOn,
    StartWait { since: Instant },
    Reconnect,
    Complete,
}
#[derive(Clone, Copy)]
pub struct Observation {
    pub now: Instant,
    pub connected: bool,
    pub normal: bool,
    pub fresh_return: bool,
    pub completion_allowed: bool,
    pub needs_power_on: bool,
}
enum Event {
    Restart,
    Next(Observation),
    Confirm(Instant),
    #[cfg(test)]
    Seed(Phase),
}
#[derive(Default)]
struct Storage;
fn common(event: &Event) -> Option<Outcome<State>> {
    match event {
        Event::Restart => Some(Transition(State::disconnect())),
        Event::Next(o) if o.fresh_return && o.completion_allowed => {
            Some(Transition(State::complete()))
        }
        #[cfg(test)]
        Event::Seed(p) => Some(Transition(match p {
            Phase::Disconnect => State::disconnect(),
            Phase::PowerOn => State::power_on(),
            Phase::StartWait { since } => State::start_wait(*since),
            Phase::Reconnect => State::reconnect(),
            Phase::Complete => State::complete(),
        })),
        _ => None,
    }
}
#[state_machine(initial = "State::inactive()")]
impl Storage {
    #[state]
    fn inactive(event: &Event) -> Outcome<State> {
        match event {
            Event::Restart => Transition(State::disconnect()),
            #[cfg(test)]
            Event::Seed(_) => common(event).unwrap(),
            _ => Handled,
        }
    }
    #[state]
    fn disconnect(event: &Event) -> Outcome<State> {
        common(event).unwrap_or_else(|| match event {
            Event::Next(o) if !o.connected => Transition(if o.needs_power_on {
                State::power_on()
            } else {
                State::reconnect()
            }),
            _ => Handled,
        })
    }
    #[state]
    fn power_on(event: &Event) -> Outcome<State> {
        common(event).unwrap_or_else(|| match event {
            Event::Next(o) if o.connected => Transition(State::disconnect()),
            Event::Confirm(now) => Transition(State::start_wait(*now)),
            _ => Handled,
        })
    }
    #[state]
    fn start_wait(since: &Instant, event: &Event) -> Outcome<State> {
        common(event).unwrap_or_else(|| match event {
            Event::Next(o) if o.connected => Transition(State::disconnect()),
            Event::Next(o)
                if o.now.saturating_duration_since(*since) >= Duration::from_secs(10) =>
            {
                Transition(State::reconnect())
            }
            _ => Handled,
        })
    }
    #[state]
    fn reconnect(event: &Event) -> Outcome<State> {
        common(event).unwrap_or_else(|| match event {
            Event::Next(o) if o.normal && o.completion_allowed => Transition(State::complete()),
            Event::Next(o) if o.normal => Handled,
            Event::Next(o) if o.connected => Transition(State::disconnect()),
            _ => Handled,
        })
    }
    #[state]
    fn complete(event: &Event) -> Outcome<State> {
        common(event).unwrap_or_else(|| match event {
            Event::Next(o) if !o.normal || !o.completion_allowed => Transition(State::disconnect()),
            _ => Handled,
        })
    }
}
pub struct ReturnFlow {
    machine: StateMachine<Storage>,
    latest: Option<Observation>,
    absent_since: Option<Instant>,
}
impl Default for ReturnFlow {
    fn default() -> Self {
        Self {
            machine: Storage.state_machine(),
            latest: None,
            absent_since: None,
        }
    }
}
impl ReturnFlow {
    pub fn can_next(&self, now: Instant) -> bool {
        let Some(o) = self.latest else {
            return false;
        };
        if now.saturating_duration_since(o.now) > Duration::from_secs(5) {
            return false;
        }
        let returned = o.normal && o.completion_allowed;
        match self.phase() {
            Some(Phase::Disconnect) => {
                returned && o.fresh_return
                    || !o.connected
                        && self.absent_since.is_some_and(|absent| {
                            now.saturating_duration_since(absent) >= Duration::from_secs(5)
                        })
            }
            Some(Phase::PowerOn) => returned && o.fresh_return || !o.connected,
            Some(Phase::StartWait { since }) => {
                returned && o.fresh_return
                    || o.connected
                    || self.absent_since.is_some_and(|absent| {
                        now.saturating_duration_since(absent.max(since)) >= Duration::from_secs(10)
                    })
            }
            Some(Phase::Reconnect) => returned || o.connected && !o.normal,
            _ => false,
        }
    }
    pub fn next(&mut self, now: Instant) -> bool {
        if !self.can_next(now) {
            return false;
        }
        let mut o = self.latest.unwrap();
        o.now = now;
        if matches!(self.phase(), Some(Phase::PowerOn)) && !o.connected {
            self.machine.handle(&Event::Confirm(now));
        } else {
            self.machine.handle(&Event::Next(o));
        }
        true
    }
    pub fn phase(&self) -> Option<Phase> {
        match self.machine.state() {
            State::Inactive {} => None,
            State::Disconnect {} => Some(Phase::Disconnect),
            State::PowerOn {} => Some(Phase::PowerOn),
            State::StartWait { since } => Some(Phase::StartWait { since: *since }),
            State::Reconnect {} => Some(Phase::Reconnect),
            State::Complete {} => Some(Phase::Complete),
        }
    }
    pub fn restart(&mut self) {
        self.latest = None;
        self.absent_since = None;
        self.machine.handle(&Event::Restart);
    }
    pub fn observe(&mut self, observation: Observation) {
        if observation.connected {
            self.absent_since = None;
        } else {
            self.absent_since.get_or_insert(observation.now);
        }
        self.latest = Some(observation);
    }
    #[cfg(test)]
    pub fn confirm(&mut self, now: Instant) {
        self.machine.handle(&Event::Confirm(now));
    }
    #[cfg(test)]
    pub fn at(phase: Phase) -> Self {
        let mut flow = Self::default();
        flow.machine.handle(&Event::Seed(phase));
        flow
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn observation(now: Instant, connected: bool, normal: bool) -> Observation {
        Observation {
            now,
            connected,
            normal,
            fresh_return: normal,
            completion_allowed: normal,
            needs_power_on: false,
        }
    }
    #[test]
    fn observations_update_readiness_but_only_next_advances_each_return_step() {
        let now = Instant::now();
        let mut flow = ReturnFlow::default();
        flow.restart();
        flow.observe(observation(now, false, false));
        assert!(matches!(flow.phase(), Some(Phase::Disconnect)));
        assert!(!flow.next(now));
        flow.observe(observation(now + Duration::from_secs(5), false, false));
        assert!(matches!(flow.phase(), Some(Phase::Disconnect)));
        assert!(flow.next(now + Duration::from_secs(5)));
        flow.observe(observation(now + Duration::from_secs(6), true, true));
        assert!(matches!(flow.phase(), Some(Phase::Reconnect)));
        assert!(flow.next(now + Duration::from_secs(6)));
        assert!(matches!(flow.phase(), Some(Phase::Complete)));
    }
    #[test]
    fn lost_usb_absence_resets_physical_wait_and_stale_observation_cannot_advance() {
        let now = Instant::now();
        let mut flow = ReturnFlow::default();
        flow.restart();
        flow.observe(observation(now, false, false));
        flow.next(now);
        flow.observe(observation(now + Duration::from_secs(4), true, false));
        flow.observe(observation(now + Duration::from_secs(5), false, false));
        assert!(!flow.next(now + Duration::from_secs(6)));
        flow.observe(observation(now + Duration::from_secs(10), false, false));
        assert!(flow.next(now + Duration::from_secs(10)));
        flow.observe(observation(now + Duration::from_secs(11), true, true));
        assert!(!flow.next(now + Duration::from_secs(17)));
        assert!(matches!(flow.phase(), Some(Phase::Reconnect)));
    }
}

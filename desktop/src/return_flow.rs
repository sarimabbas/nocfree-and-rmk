//! Pure physical-return protocol; time and connection evidence arrive as events.
use statig::prelude::*;
use std::time::{Duration, Instant};
#[derive(Clone, Copy, Debug)]
pub enum Phase {
    Disconnect,
    OffWait { since: Instant },
    PowerOn,
    StartWait { since: Instant },
    Reconnect,
    Complete,
}
pub struct Observation {
    pub now: Instant,
    pub connected: bool,
    pub normal: bool,
    pub fresh_return: bool,
    pub needs_power_on: bool,
}
enum Event {
    Restart,
    Observe(Observation),
    Confirm(Instant),
    #[cfg(test)]
    Seed(Phase),
}
#[derive(Default)]
struct Storage;
fn common(event: &Event) -> Option<Outcome<State>> {
    match event {
        Event::Restart => Some(Transition(State::disconnect())),
        Event::Observe(o) if o.fresh_return => Some(Transition(State::complete())),
        #[cfg(test)]
        Event::Seed(p) => Some(Transition(match p {
            Phase::Disconnect => State::disconnect(),
            Phase::OffWait { since } => State::off_wait(*since),
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
            Event::Observe(o) if !o.connected => Transition(State::off_wait(o.now)),
            _ => Handled,
        })
    }
    #[state]
    fn off_wait(since: &Instant, event: &Event) -> Outcome<State> {
        common(event).unwrap_or_else(|| match event {
            Event::Observe(o) if o.connected => Transition(State::disconnect()),
            Event::Observe(o)
                if o.now.saturating_duration_since(*since) >= Duration::from_secs(5) =>
            {
                Transition(if o.needs_power_on {
                    State::power_on()
                } else {
                    State::reconnect()
                })
            }
            _ => Handled,
        })
    }
    #[state]
    fn power_on(event: &Event) -> Outcome<State> {
        common(event).unwrap_or_else(|| match event {
            Event::Observe(o) if o.connected => Transition(State::disconnect()),
            Event::Confirm(now) => Transition(State::start_wait(*now)),
            _ => Handled,
        })
    }
    #[state]
    fn start_wait(since: &Instant, event: &Event) -> Outcome<State> {
        common(event).unwrap_or_else(|| match event {
            Event::Observe(o) if o.connected => Transition(State::disconnect()),
            Event::Observe(o)
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
            Event::Observe(o) if o.normal => Transition(State::complete()),
            Event::Observe(o) if o.connected => Transition(State::disconnect()),
            _ => Handled,
        })
    }
    #[state]
    fn complete(event: &Event) -> Outcome<State> {
        common(event).unwrap_or_else(|| match event {
            Event::Observe(o) if !o.normal => Transition(State::disconnect()),
            _ => Handled,
        })
    }
}
pub struct ReturnFlow(StateMachine<Storage>);
impl Default for ReturnFlow {
    fn default() -> Self {
        Self(Storage.state_machine())
    }
}
impl ReturnFlow {
    pub fn phase(&self) -> Option<Phase> {
        match self.0.state() {
            State::Inactive {} => None,
            State::Disconnect {} => Some(Phase::Disconnect),
            State::OffWait { since } => Some(Phase::OffWait { since: *since }),
            State::PowerOn {} => Some(Phase::PowerOn),
            State::StartWait { since } => Some(Phase::StartWait { since: *since }),
            State::Reconnect {} => Some(Phase::Reconnect),
            State::Complete {} => Some(Phase::Complete),
        }
    }
    pub fn restart(&mut self) {
        self.0.handle(&Event::Restart);
    }
    pub fn observe(&mut self, observation: Observation) {
        self.0.handle(&Event::Observe(observation));
    }
    pub fn confirm(&mut self, now: Instant) {
        self.0.handle(&Event::Confirm(now));
    }
    #[cfg(test)]
    pub fn at(phase: Phase) -> Self {
        let mut flow = Self::default();
        flow.0.handle(&Event::Seed(phase));
        flow
    }
}

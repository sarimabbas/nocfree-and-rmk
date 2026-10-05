//! One explicitly selected component and one bounded recovery attempt at a time.
//! Ready means the transport observed a correlated recovery drive, not a firmware update.
use crate::runtime_recovery::Role;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Choose,
    Identify(Role),
    Guiding(Role, Procedure),
    Ready(Role),
    Failed(Role, String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Procedure {
    Reconnect,
    FactoryLeft,
    FactoryRight,
    FactoryReceiver,
    RuntimeApp,
}

impl Procedure {
    pub fn instruction(self, role: Role) -> &'static str {
        match self {
            Self::Reconnect => match role {
                Role::Left => {
                    "Unplug the other parts from USB. Unplug the left half, then reconnect it to the same USB port."
                }
                Role::Right => {
                    "Keep the left half in WIRED mode with USB connected. Unplug the right half, then reconnect it to the same USB port."
                }
                Role::Receiver => {
                    "Unplug both halves and the dongle. Wait five seconds, then reconnect only the dongle to the same USB port."
                }
            },
            Self::FactoryLeft => {
                "Keep the left half in WIRED mode with USB connected. Hold Fn + 5 for five seconds, then release both keys."
            }
            Self::FactoryRight => {
                "Keep the paired left half on factory firmware, in WIRED mode with USB connected. Turn the right half ON and connect its USB cable. Hold Fn + 0 on the number row for five seconds, then release both keys."
            }
            Self::FactoryReceiver => {
                "Keep only the dongle connected by USB. Use the paired left half with factory firmware and move its switch to top DONGLE. Hold Fn + 6 on the left half for five seconds, then release both keys."
            }
            Self::RuntimeApp => "Keep USB connected while the app opens the recovery drive.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attempt(u64);

mod machine {
    use super::{Attempt, Procedure, Role};
    use statig::prelude::*;

    #[derive(Default)]
    pub struct Recovery {
        pub generation: u64,
    }
    pub enum Event {
        Start(Role),
        Observe(Attempt, Role, Procedure),
        Complete(Attempt, Role, Result<(), String>),
        Cancel,
        Retry,
    }
    impl Recovery {
        fn next_attempt(&mut self) {
            self.generation = self
                .generation
                .checked_add(1)
                .expect("Recovery attempt counter exhausted");
        }
        fn start_attempt(&mut self, role: Role, context: &mut bool) -> Outcome<State> {
            self.next_attempt();
            *context = true;
            Transition(State::identify(role))
        }
        fn progress(&self, role: Role, event: &Event, context: &mut bool) -> Outcome<State> {
            match event {
                Event::Observe(attempt, selected, procedure)
                    if attempt.0 == self.generation && *selected == role =>
                {
                    *context = true;
                    Transition(State::guiding(role, *procedure))
                }
                Event::Complete(attempt, selected, result)
                    if attempt.0 == self.generation && *selected == role =>
                {
                    *context = true;
                    match result {
                        Ok(()) => Transition(State::ready(role)),
                        Err(error) => Transition(State::failed(role, error.clone())),
                    }
                }
                _ => Super,
            }
        }
    }
    #[state_machine(initial = "State::choose()", state(derive(Debug)))]
    impl Recovery {
        #[state(superstate = "cancellable")]
        fn choose(&mut self, event: &Event, context: &mut bool) -> Outcome<State> {
            match event {
                Event::Start(role) => self.start_attempt(*role, context),
                _ => Super,
            }
        }
        #[state(superstate = "cancellable")]
        fn identify(&mut self, role: &Role, event: &Event, context: &mut bool) -> Outcome<State> {
            self.progress(*role, event, context)
        }
        #[state(superstate = "cancellable")]
        fn guiding(
            &mut self,
            role: &Role,
            procedure: &Procedure,
            event: &Event,
            context: &mut bool,
        ) -> Outcome<State> {
            let _ = procedure;
            self.progress(*role, event, context)
        }
        #[state(superstate = "cancellable")]
        fn ready(&mut self, role: &Role, event: &Event, context: &mut bool) -> Outcome<State> {
            let _ = role;
            match event {
                Event::Start(role) => self.start_attempt(*role, context),
                _ => Super,
            }
        }
        #[state(superstate = "cancellable")]
        fn failed(
            &mut self,
            role: &Role,
            error: &String,
            event: &Event,
            context: &mut bool,
        ) -> Outcome<State> {
            let _ = error;
            match event {
                Event::Retry => self.start_attempt(*role, context),
                Event::Start(role) => self.start_attempt(*role, context),
                _ => Super,
            }
        }
        #[superstate]
        fn cancellable(&mut self, event: &Event, context: &mut bool) -> Outcome<State> {
            match event {
                Event::Cancel => {
                    self.next_attempt();
                    *context = true;
                    Transition(State::choose())
                }
                _ => Super,
            }
        }
    }
}

/// Statig owns the state; the public enum is an immutable rendering projection.
pub struct RecoveryJourney {
    machine: statig::blocking::StateMachine<machine::Recovery>,
}
impl Default for RecoveryJourney {
    fn default() -> Self {
        Self::new()
    }
}
impl RecoveryJourney {
    pub fn new() -> Self {
        use statig::prelude::IntoStateMachineExt;
        Self {
            machine: machine::Recovery::default().state_machine(),
        }
    }
    pub fn state(&self) -> State {
        match self.machine.state() {
            machine::State::Choose {} => State::Choose,
            machine::State::Identify { role } => State::Identify(*role),
            machine::State::Guiding { role, procedure } => State::Guiding(*role, *procedure),
            machine::State::Ready { role } => State::Ready(*role),
            machine::State::Failed { role, error } => State::Failed(*role, error.clone()),
        }
    }
    fn dispatch(&mut self, event: machine::Event) -> bool {
        let mut accepted = false;
        self.machine.handle_with_context(&event, &mut accepted);
        accepted
    }
    pub fn start(&mut self, role: Role) -> Option<Attempt> {
        self.dispatch(machine::Event::Start(role))
            .then(|| Attempt(self.machine.inner().generation))
    }
    pub fn observe(&mut self, attempt: Attempt, role: Role, procedure: Procedure) -> bool {
        self.dispatch(machine::Event::Observe(attempt, role, procedure))
    }
    pub fn complete(&mut self, attempt: Attempt, role: Role, result: Result<(), String>) -> bool {
        self.dispatch(machine::Event::Complete(attempt, role, result))
    }
    pub fn cancel(&mut self) {
        self.dispatch(machine::Event::Cancel);
    }
    pub fn retry(&mut self) -> Option<(Role, Attempt)> {
        if !self.dispatch(machine::Event::Retry) {
            return None;
        }
        let State::Identify(role) = self.state() else {
            unreachable!()
        };
        Some((role, Attempt(self.machine.inner().generation)))
    }
}

/// Only physical connection is required; normal RMK recovery has no startup window.
pub fn instruction(role: Role) -> &'static str {
    match role {
        Role::Left => "Connect the left half by USB. ",
        Role::Right => "Connect the right half by USB. ",
        Role::Receiver => "Plug in the USB dongle. ",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn factory_progress_and_cancel_retry_reject_stale_guides() {
        let mut flow = RecoveryJourney::new();
        let old = flow.start(Role::Left).unwrap();
        assert!(flow.observe(old, Role::Left, Procedure::FactoryLeft));
        assert_eq!(
            flow.state(),
            State::Guiding(Role::Left, Procedure::FactoryLeft)
        );
        flow.cancel();
        let current = flow.start(Role::Left).unwrap();
        assert!(!flow.observe(old, Role::Left, Procedure::RuntimeApp));
        assert!(!flow.complete(old, Role::Left, Ok(())));
        assert!(flow.observe(current, Role::Left, Procedure::FactoryLeft));
        assert!(flow.complete(current, Role::Left, Err("Different USB connection".into())));
        let (_, retry) = flow.retry().unwrap();
        assert!(!flow.observe(current, Role::Left, Procedure::FactoryLeft));
        assert_eq!(flow.state(), State::Identify(Role::Left));
        assert!(flow.observe(retry, Role::Left, Procedure::FactoryLeft));
    }
    #[test]
    fn cancellation_and_new_role_reject_the_previous_callback() {
        let mut flow = RecoveryJourney::new();
        let old = flow.start(Role::Left).unwrap();
        flow.cancel();
        assert!(!flow.complete(old, Role::Left, Ok(())));
        assert_eq!(flow.state(), State::Choose);
        let current = flow.start(Role::Right).unwrap();
        assert!(!flow.complete(old, Role::Left, Ok(())));
        assert!(!flow.complete(current, Role::Left, Ok(())));
        assert_eq!(flow.state(), State::Identify(Role::Right));
        assert!(flow.complete(current, Role::Right, Ok(())));
        assert_eq!(flow.state(), State::Ready(Role::Right));
    }
    #[test]
    fn failure_retry_is_a_fresh_explicit_attempt() {
        let mut flow = RecoveryJourney::new();
        assert!(flow.retry().is_none());
        let old = flow.start(Role::Receiver).unwrap();
        assert!(flow.start(Role::Left).is_none());
        assert!(flow.retry().is_none());
        assert!(flow.complete(old, Role::Receiver, Err("Drive did not appear".into())));
        let (role, current) = flow.retry().unwrap();
        assert_eq!(role, Role::Receiver);
        assert_ne!(current, old);
        assert!(!flow.complete(old, role, Ok(())));
        assert!(flow.complete(current, role, Ok(())));
        assert!(flow.retry().is_none());
        assert!(!flow.complete(current, role, Err("late error".into())));
        assert_eq!(flow.state(), State::Ready(role));
    }
    #[test]
    fn waiting_never_becomes_ready_without_a_correlated_transport_result() {
        let mut flow = RecoveryJourney::new();
        let attempt = flow.start(Role::Left).unwrap();
        assert_eq!(flow.state(), State::Identify(Role::Left));
        assert!(flow.complete(
            attempt,
            Role::Left,
            Err("Unsupported firmware or missing USB connection".into())
        ));
        assert!(matches!(flow.state(), State::Failed(Role::Left, _)));
    }
}

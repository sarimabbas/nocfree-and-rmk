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
    Manual,
    RuntimeApp,
}

impl Procedure {
    pub fn instruction(self, role: Role) -> &'static str {
        match self {
            Self::Reconnect => match role {
                Role::Left => {
                    "Disconnect the other parts from USB. Unplug the left USB cable, then reconnect it to the same port."
                }
                Role::Right => {
                    "Disconnect the other parts from USB. Unplug the right USB cable, then reconnect it to the same port."
                }
                Role::Receiver => {
                    "Disconnect both halves from USB. Unplug the receiver, then reconnect it to the same port."
                }
            },
            Self::FactoryLeft => {
                "Keep USB connected and the switch in WIRED. Hold Fn + 5 for five seconds, then release."
            }
            Self::FactoryRight => {
                "Turn the right half ON and keep USB connected. Hold Fn + the main-row 0 key for five seconds, then release."
            }
            Self::FactoryReceiver => {
                "Keep only the receiver connected by USB. Using its paired factory left half in 2.4G mode, hold Fn + 6 for five seconds. This requires the factory Fn-layer 6 key mapped to DongleDFU."
            }
            Self::Manual => {
                "Use the recovery procedure for your installed firmware, keeping the same USB port."
            }
            Self::RuntimeApp => "Keep USB connected. We’re opening its recovery drive.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attempt(u64);

pub struct RecoveryJourney {
    state: State,
    generation: u64,
}

impl Default for RecoveryJourney {
    fn default() -> Self {
        Self::new()
    }
}

impl RecoveryJourney {
    pub fn new() -> Self {
        Self {
            state: State::Choose,
            generation: 0,
        }
    }
    pub fn state(&self) -> &State {
        &self.state
    }
    fn invalidate(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("Recovery attempt counter exhausted");
    }
    pub fn start(&mut self, role: Role) -> Option<Attempt> {
        if matches!(self.state, State::Identify(_) | State::Guiding(_, _)) {
            return None;
        }
        self.invalidate();
        self.state = State::Identify(role);
        Some(Attempt(self.generation))
    }
    fn active(&self, attempt: Attempt, role: Role) -> bool {
        attempt == Attempt(self.generation)
            && match self.state {
                State::Identify(selected) | State::Guiding(selected, _) => selected == role,
                _ => false,
            }
    }
    pub fn observe(&mut self, attempt: Attempt, role: Role, procedure: Procedure) -> bool {
        if !self.active(attempt, role) {
            return false;
        }
        self.state = State::Guiding(role, procedure);
        true
    }
    /// Stale, cancelled, duplicate or wrong-role callbacks cannot replace current guidance.
    pub fn complete(&mut self, attempt: Attempt, role: Role, result: Result<(), String>) -> bool {
        if !self.active(attempt, role) {
            return false;
        }
        self.state = match result {
            Ok(()) => State::Ready(role),
            Err(error) => State::Failed(role, error),
        };
        true
    }
    pub fn cancel(&mut self) {
        self.invalidate();
        self.state = State::Choose;
    }
    pub fn retry(&mut self) -> Option<(Role, Attempt)> {
        let State::Failed(role, _) = &self.state else {
            return None;
        };
        let role = *role;
        self.start(role).map(|attempt| (role, attempt))
    }
}

/// Only physical connection is required; normal RMK recovery has no startup window.
pub fn instruction(role: Role) -> &'static str {
    match role {
        Role::Left => "Connect the left half by USB. We’ll find the steps for its firmware.",
        Role::Right => "Connect the right half by USB. We’ll find the steps for its firmware.",
        Role::Receiver => "Plug in the USB receiver. We’ll find the steps for its firmware.",
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
            &State::Guiding(Role::Left, Procedure::FactoryLeft)
        );
        flow.cancel();
        let current = flow.start(Role::Left).unwrap();
        assert!(!flow.observe(old, Role::Left, Procedure::RuntimeApp));
        assert!(!flow.complete(old, Role::Left, Ok(())));
        assert!(flow.observe(current, Role::Left, Procedure::FactoryLeft));
        assert!(flow.complete(current, Role::Left, Err("Different USB connection".into())));
        let (_, retry) = flow.retry().unwrap();
        assert!(!flow.observe(current, Role::Left, Procedure::FactoryLeft));
        assert_eq!(flow.state(), &State::Identify(Role::Left));
        assert!(flow.observe(retry, Role::Left, Procedure::FactoryLeft));
    }
    #[test]
    fn cancellation_and_new_role_reject_the_previous_callback() {
        let mut flow = RecoveryJourney::new();
        let old = flow.start(Role::Left).unwrap();
        flow.cancel();
        assert!(!flow.complete(old, Role::Left, Ok(())));
        assert_eq!(flow.state(), &State::Choose);
        let current = flow.start(Role::Right).unwrap();
        assert!(!flow.complete(old, Role::Left, Ok(())));
        assert!(!flow.complete(current, Role::Left, Ok(())));
        assert_eq!(flow.state(), &State::Identify(Role::Right));
        assert!(flow.complete(current, Role::Right, Ok(())));
        assert_eq!(flow.state(), &State::Ready(Role::Right));
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
        assert_eq!(flow.state(), &State::Ready(role));
    }
    #[test]
    fn waiting_never_becomes_ready_without_a_correlated_transport_result() {
        let mut flow = RecoveryJourney::new();
        let attempt = flow.start(Role::Left).unwrap();
        assert_eq!(flow.state(), &State::Identify(Role::Left));
        assert!(flow.complete(
            attempt,
            Role::Left,
            Err("Unsupported firmware or missing USB connection".into())
        ));
        assert!(matches!(flow.state(), State::Failed(Role::Left, _)));
    }
}

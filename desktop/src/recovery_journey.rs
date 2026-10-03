//! One explicitly selected component and one bounded recovery attempt at a time.
//! Ready means the transport observed a correlated recovery drive, not a firmware update.
use crate::experimental_recovery::Role;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Choose,
    Waiting(Role),
    Ready(Role),
    Failed(Role, String),
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
        if matches!(self.state, State::Waiting(_)) {
            return None;
        }
        self.invalidate();
        self.state = State::Waiting(role);
        Some(Attempt(self.generation))
    }
    /// Stale, cancelled, duplicate or wrong-role callbacks cannot replace current guidance.
    pub fn complete(&mut self, attempt: Attempt, role: Role, result: Result<(), String>) -> bool {
        if attempt != Attempt(self.generation) || self.state != State::Waiting(role) {
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

/// A physical cold-start guide. USB reconnection alone does not reset battery-powered halves.
pub fn instruction(role: Role) -> &'static str {
    match role {
        Role::Left => {
            "Move the left switch to WIRED. Unplug its USB cable for five seconds, then reconnect it."
        }
        Role::Right => {
            "Turn the right half OFF. Unplug its USB cable for five seconds, then reconnect it while still OFF."
        }
        Role::Receiver => "Unplug the receiver for five seconds, then plug it back in.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(flow.state(), &State::Waiting(Role::Right));
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
        assert_eq!(flow.state(), &State::Waiting(Role::Left));
        assert!(flow.complete(
            attempt,
            Role::Left,
            Err("Unsupported firmware or no cold start".into())
        ));
        assert!(matches!(flow.state(), State::Failed(Role::Left, _)));
    }
}

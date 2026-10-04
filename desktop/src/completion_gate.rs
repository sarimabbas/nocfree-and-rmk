//! Readiness adapter for effects that still use the shared gate interface.
//!
//! Freshly satisfied conditions are immediately ready. Visible journey stages
//! advance only through explicit Next events; there is no artificial hold.
//! This helper never performs effects or delays cancellation and failures.
use std::time::{Duration, Instant};

pub const DEFAULT_COMPLETION_DELAY: Duration = Duration::ZERO;

#[derive(Debug)]
pub struct CompletionGate<K> {
    pending: Option<(K, Instant)>,
}

impl<K> Default for CompletionGate<K> {
    fn default() -> Self {
        Self { pending: None }
    }
}

impl<K: Eq> CompletionGate<K> {
    /// Current readiness, with no artificial delay. Poll with freshly checked
    /// prerequisites, including unsuccessful observations.
    pub fn ready(&mut self, key: K, condition: bool, now: Instant) -> bool {
        if !condition {
            self.reset();
            return false;
        }
        match &self.pending {
            Some((pending, since)) if *pending == key => {
                if let Some(elapsed) = now.checked_duration_since(*since) {
                    return elapsed >= DEFAULT_COMPLETION_DELAY;
                }
            }
            _ => {}
        }
        self.pending = Some((key, now));
        DEFAULT_COMPLETION_DELAY.is_zero()
    }

    /// Discard a pending success after cancellation, failure, or explicit reset.
    pub fn reset(&mut self) {
        self.pending = None;
    }
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        if DEFAULT_COMPLETION_DELAY.is_zero() {
            return None;
        }
        self.pending.as_ref().map(|(_, since)| {
            DEFAULT_COMPLETION_DELAY.saturating_sub(now.saturating_duration_since(*since))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_satisfied_conditions_are_ready_without_a_timer() {
        let now = Instant::now();
        let mut gate = CompletionGate::default();
        assert_eq!(DEFAULT_COMPLETION_DELAY, Duration::ZERO);
        assert!(gate.ready("left", true, now));
        assert!(gate.remaining(now).is_none());
    }

    #[test]
    fn readiness_loss_clears_pending_proof_immediately() {
        let now = Instant::now();
        let mut gate = CompletionGate::default();
        assert!(gate.ready("left", true, now));
        assert!(!gate.ready("left", false, now));
        assert!(gate.pending.is_none());
        assert!(gate.ready("left", true, now));
    }

    #[test]
    fn new_identity_uses_its_current_condition() {
        let now = Instant::now();
        let mut gate = CompletionGate::default();
        assert!(gate.ready(("left", 1), true, now));
        assert!(!gate.ready(("right", 2), false, now));
        assert!(gate.ready(("right", 2), true, now));
        assert_eq!(gate.pending.as_ref().unwrap().0, ("right", 2));
    }

    #[test]
    fn backwards_time_does_not_invent_a_hold() {
        let now = Instant::now();
        let mut gate = CompletionGate::default();
        assert!(gate.ready("left", true, now + Duration::from_secs(10)));
        assert!(gate.ready("left", true, now));
        assert!(!gate.ready("left", false, now));
    }

    #[test]
    fn explicit_reset_discards_the_previous_observation() {
        let now = Instant::now();
        let mut gate = CompletionGate::default();
        assert!(gate.ready("left", true, now));
        gate.reset();
        assert!(gate.pending.is_none());
        assert!(!gate.ready("left", false, now));
    }
}

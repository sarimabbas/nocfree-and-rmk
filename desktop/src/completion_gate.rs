//! Default pacing for successful, automatic journey steps.
//!
//! Poll with the current step identity and freshly checked prerequisites. A
//! different step or lost prerequisite starts the full delay again. This gate
//! never sleeps, performs effects, or delays cancellation and failure handling.
use std::time::{Duration, Instant};

pub const DEFAULT_COMPLETION_DELAY: Duration = Duration::from_secs(5);

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
    /// True only after the same step's prerequisites remain satisfied for five
    /// seconds. Call on each fresh observation, including unsuccessful ones.
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
        false
    }

    /// Discard a pending success after cancellation, failure, or explicit reset.
    pub fn reset(&mut self) {
        self.pending = None;
    }
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        self.pending.as_ref().map(|(_, since)| {
            DEFAULT_COMPLETION_DELAY.saturating_sub(now.saturating_duration_since(*since))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_waits_the_full_default_delay() {
        let now = Instant::now();
        let mut gate = CompletionGate::default();
        assert!(!gate.ready("left", true, now));
        assert!(!gate.ready("left", true, now + Duration::from_millis(4999)));
        assert!(gate.ready("left", true, now + DEFAULT_COMPLETION_DELAY));
    }

    #[test]
    fn lost_prerequisite_discards_even_an_expired_success() {
        let now = Instant::now();
        let mut gate = CompletionGate::default();
        assert!(!gate.ready("left", true, now));
        assert!(!gate.ready("left", false, now + DEFAULT_COMPLETION_DELAY));
        assert!(!gate.ready("left", true, now + Duration::from_secs(6)));
        assert!(!gate.ready("left", true, now + Duration::from_secs(10)));
        assert!(gate.ready("left", true, now + Duration::from_secs(11)));
    }

    #[test]
    fn new_step_cannot_inherit_another_steps_elapsed_time() {
        let now = Instant::now();
        let mut gate = CompletionGate::default();
        assert!(!gate.ready(("left", 1), true, now));
        assert!(!gate.ready(("right", 1), true, now + Duration::from_secs(6)));
        assert!(!gate.ready(("right", 2), true, now + Duration::from_secs(12)));
        assert!(gate.ready(("right", 2), true, now + Duration::from_secs(17)));
    }

    #[test]
    fn backwards_time_restarts_the_window_instead_of_advancing() {
        let now = Instant::now();
        let mut gate = CompletionGate::default();
        assert!(!gate.ready("left", true, now + Duration::from_secs(10)));
        assert!(!gate.ready("left", true, now + Duration::from_secs(2)));
        assert!(!gate.ready("left", true, now + Duration::from_secs(6)));
        assert!(gate.ready("left", true, now + Duration::from_secs(7)));
    }

    #[test]
    fn explicit_reset_prevents_reusing_completed_step() {
        let now = Instant::now();
        let mut gate = CompletionGate::default();
        assert!(!gate.ready("left", true, now));
        assert!(gate.ready("left", true, now + DEFAULT_COMPLETION_DELAY));
        gate.reset();
        assert!(!gate.ready("left", true, now + Duration::from_secs(6)));
    }
}

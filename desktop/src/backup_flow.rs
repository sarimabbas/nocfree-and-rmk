//! UI orchestration for the backup journey and its asynchronous recovery worker.
//! One state replaces overlapping started/completed/stopped/recovery flags.
use crate::journey;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Choose,
    Guiding,
    Recovering,
    RecoveryFailed,
    Saving,
    Returning,
    Paused,
    Failed,
    Complete,
}
#[derive(Clone, Copy, Debug)]
pub enum Event {
    Select,
    RecoveryStarted,
    RecoveryFinished(bool),
    SaveStarted,
    Observed(journey::State),
    Pause,
    Retry,
    Finish,
}
impl State {
    pub fn active(self) -> bool {
        !matches!(self, Self::Choose | Self::Paused | Self::Complete)
    }
    pub fn recovery(self) -> bool {
        matches!(self, Self::Recovering | Self::RecoveryFailed)
    }
    pub fn shown(self) -> bool {
        self.active() || self == Self::Complete
    }
    pub fn failed(self) -> bool {
        matches!(self, Self::Failed | Self::RecoveryFailed)
    }
    /// Invalid events are rejected; delayed callbacks cannot restart a cancelled flow.
    pub fn transition(&mut self, event: Event) -> bool {
        use Event::*;
        let next = match (*self, event) {
            (Self::Choose | Self::Paused | Self::Complete, Select) => Self::Guiding,
            (Self::Guiding, RecoveryStarted) => Self::Recovering,
            (Self::Recovering, RecoveryFinished(true)) => Self::Guiding,
            (Self::Recovering, RecoveryFinished(false)) => Self::RecoveryFailed,
            (Self::Guiding, SaveStarted) => Self::Saving,
            (Self::Guiding | Self::Returning | Self::Saving, Observed(state)) => match state {
                journey::State::Guiding | journey::State::ReadyToSave => Self::Guiding,
                journey::State::Returning => Self::Returning,
                journey::State::Paused => Self::Paused,
                journey::State::Failed => Self::Failed,
                journey::State::Complete => Self::Complete,
            },
            (
                Self::Guiding
                | Self::Recovering
                | Self::RecoveryFailed
                | Self::Returning
                | Self::Failed,
                Pause,
            ) => Self::Paused,
            (Self::Failed, Retry) => Self::Guiding,
            (Self::RecoveryFailed, Retry) => Self::Recovering,
            (Self::Complete, Finish) => Self::Choose,
            _ => return false,
        };
        *self = next;
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancel_recovery_then_ignore_late_result_and_start_fresh() {
        let mut state = State::Choose;
        assert!(state.transition(Event::Select));
        assert!(state.transition(Event::RecoveryStarted));
        assert!(state.transition(Event::Pause));
        assert!(!state.transition(Event::RecoveryFinished(true)));
        assert_eq!(state, State::Paused);
        assert!(state.transition(Event::Select));
        assert!(state.transition(Event::RecoveryStarted));
        assert!(state.transition(Event::RecoveryFinished(true)));
        assert!(state.transition(Event::SaveStarted));
        assert!(!state.transition(Event::Pause));
        assert!(state.transition(Event::Observed(journey::State::Returning)));
        assert!(state.transition(Event::Observed(journey::State::Complete)));
        assert!(state.transition(Event::Finish));
        assert_eq!(state, State::Choose);
    }
    #[test]
    fn recovery_and_save_errors_have_distinct_retry_paths() {
        let mut state = State::Guiding;
        assert!(state.transition(Event::RecoveryStarted));
        assert!(state.transition(Event::RecoveryFinished(false)));
        assert!(state.recovery());
        assert!(state.transition(Event::Retry));
        assert!(state.transition(Event::RecoveryFinished(true)));
        assert!(state.transition(Event::SaveStarted));
        assert!(state.transition(Event::Observed(journey::State::Failed)));
        assert!(!state.recovery());
        assert!(state.transition(Event::Retry));
        assert_eq!(state, State::Guiding);
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    #[test]
    fn choose_again_after_completion_but_not_while_saving_or_recovering() {
        for source in [State::Choose, State::Paused, State::Complete] {
            let mut state = source;
            assert!(state.transition(Event::Select));
            assert_eq!(state, State::Guiding);
        }
        for source in [State::Saving, State::Recovering, State::Returning] {
            let mut state = source;
            assert!(!state.transition(Event::Select));
            assert_eq!(state, source);
        }
    }
}

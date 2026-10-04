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
}

mod machine {
    use super::{Event, journey};
    use statig::prelude::*;
    #[derive(Default)]
    pub struct Backup;
    fn accepted(state: State, context: &mut bool) -> Outcome<State> {
        *context = true;
        Transition(state)
    }
    fn observed(state: journey::State, context: &mut bool) -> Outcome<State> {
        accepted(
            match state {
                journey::State::Guiding | journey::State::ReadyToSave => State::guiding(),
                journey::State::Returning => State::returning(),
                journey::State::Paused => State::paused(),
                journey::State::Failed => State::failed(),
                journey::State::Complete => State::complete(),
            },
            context,
        )
    }
    #[state_machine(initial = "State::choose()", state(derive(Debug)))]
    impl Backup {
        #[state]
        fn choose(event: &Event, context: &mut bool) -> Outcome<State> {
            match event {
                Event::Select => accepted(State::guiding(), context),
                _ => Super,
            }
        }
        #[state]
        fn guiding(event: &Event, context: &mut bool) -> Outcome<State> {
            match event {
                Event::RecoveryStarted => accepted(State::recovering(), context),
                Event::SaveStarted => accepted(State::saving(), context),
                Event::Observed(state) => observed(*state, context),
                Event::Pause => accepted(State::paused(), context),
                _ => Super,
            }
        }
        #[state]
        fn recovering(event: &Event, context: &mut bool) -> Outcome<State> {
            match event {
                Event::RecoveryFinished(true) => accepted(State::guiding(), context),
                Event::RecoveryFinished(false) => accepted(State::recovery_failed(), context),
                Event::Pause => accepted(State::paused(), context),
                _ => Super,
            }
        }
        #[state]
        fn recovery_failed(event: &Event, context: &mut bool) -> Outcome<State> {
            match event {
                Event::Retry => accepted(State::recovering(), context),
                Event::Pause => accepted(State::paused(), context),
                _ => Super,
            }
        }
        #[state]
        fn saving(event: &Event, context: &mut bool) -> Outcome<State> {
            match event {
                Event::Observed(state) => observed(*state, context),
                _ => Super,
            }
        }
        #[state]
        fn returning(event: &Event, context: &mut bool) -> Outcome<State> {
            match event {
                Event::Observed(state) => observed(*state, context),
                Event::Pause => accepted(State::paused(), context),
                _ => Super,
            }
        }
        #[state]
        fn paused(event: &Event, context: &mut bool) -> Outcome<State> {
            match event {
                Event::Select => accepted(State::guiding(), context),
                _ => Super,
            }
        }
        #[state]
        fn failed(event: &Event, context: &mut bool) -> Outcome<State> {
            match event {
                Event::Retry => accepted(State::guiding(), context),
                Event::Pause => accepted(State::paused(), context),
                _ => Super,
            }
        }
        #[state]
        fn complete(event: &Event, context: &mut bool) -> Outcome<State> {
            match event {
                Event::Select => accepted(State::guiding(), context),
                Event::Finish => accepted(State::choose(), context),
                _ => Super,
            }
        }
    }
}

/// The Statig state is authoritative; State only projects it for rendering.
pub struct Machine {
    machine: statig::blocking::StateMachine<machine::Backup>,
    pending: Option<journey::State>,
}
impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}
impl Machine {
    pub fn new() -> Self {
        use statig::prelude::IntoStateMachineExt;
        Self {
            machine: machine::Backup.state_machine(),
            pending: None,
        }
    }
    pub fn state(&self) -> State {
        match self.machine.state() {
            machine::State::Choose {} => State::Choose,
            machine::State::Guiding {} => State::Guiding,
            machine::State::Recovering {} => State::Recovering,
            machine::State::RecoveryFailed {} => State::RecoveryFailed,
            machine::State::Saving {} => State::Saving,
            machine::State::Returning {} => State::Returning,
            machine::State::Paused {} => State::Paused,
            machine::State::Failed {} => State::Failed,
            machine::State::Complete {} => State::Complete,
        }
    }
    pub fn transition(&mut self, event: Event) -> bool {
        if let Event::Observed(target) = event
            && !matches!(target, journey::State::Failed | journey::State::Paused)
        {
            let projected = match target {
                journey::State::Guiding | journey::State::ReadyToSave => State::Guiding,
                journey::State::Returning => State::Returning,
                journey::State::Complete => State::Complete,
                _ => unreachable!(),
            };
            self.pending = (matches!(
                self.state(),
                State::Guiding | State::Saving | State::Returning
            ) && projected != self.state())
            .then_some(target);
            return false;
        }
        self.pending = None;
        self.dispatch(event)
    }
    fn dispatch(&mut self, event: Event) -> bool {
        let mut accepted = false;
        self.machine.handle_with_context(&event, &mut accepted);
        accepted
    }
    pub fn can_next(&self) -> bool {
        self.pending.is_some()
    }
    pub fn next(&mut self) -> bool {
        self.pending
            .take()
            .is_some_and(|target| self.dispatch(Event::Observed(target)))
    }
    pub fn active(&self) -> bool {
        self.state().active()
    }
    pub fn recovery(&self) -> bool {
        self.state().recovery()
    }
    pub fn shown(&self) -> bool {
        self.state().shown()
    }
    pub fn failed(&self) -> bool {
        self.state().failed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observations_enable_next_without_advancing_and_updates_replace_pending_step() {
        let mut flow = Machine::new();
        flow.transition(Event::Select);
        flow.transition(Event::SaveStarted);
        assert!(!flow.transition(Event::Observed(journey::State::Returning)));
        assert_eq!(flow.state(), State::Saving);
        assert!(flow.can_next());
        assert!(flow.next());
        assert_eq!(flow.state(), State::Returning);
        assert!(!flow.next());
        flow.transition(Event::Observed(journey::State::Complete));
        assert!(flow.can_next());
        flow.transition(Event::Observed(journey::State::Returning));
        assert!(!flow.can_next());
        assert!(!flow.next());
        flow.transition(Event::Observed(journey::State::Failed));
        assert_eq!(flow.state(), State::Failed);
        assert!(!flow.next());
    }
    pub(super) fn settled(machine: &mut Machine, event: Event) -> bool {
        machine.transition(event) || machine.next()
    }
    #[test]
    fn cancel_recovery_then_ignore_late_result_and_start_fresh() {
        let mut state = Machine::new();
        assert!(settled(&mut state, Event::Select));
        assert!(settled(&mut state, Event::RecoveryStarted));
        assert!(settled(&mut state, Event::Pause));
        assert!(!settled(&mut state, Event::RecoveryFinished(true)));
        assert_eq!(state.state(), State::Paused);
        assert!(settled(&mut state, Event::Select));
        assert!(settled(&mut state, Event::RecoveryStarted));
        assert!(settled(&mut state, Event::RecoveryFinished(true)));
        assert!(settled(&mut state, Event::SaveStarted));
        assert!(!settled(&mut state, Event::Pause));
        assert!(settled(
            &mut state,
            Event::Observed(journey::State::Returning)
        ));
        assert!(settled(
            &mut state,
            Event::Observed(journey::State::Complete)
        ));
        assert!(settled(&mut state, Event::Finish));
        assert_eq!(state.state(), State::Choose);
    }
    #[test]
    fn duplicate_callbacks_and_invalid_events_do_not_change_the_machine() {
        let mut flow = Machine::new();
        assert!(!settled(&mut flow, Event::RecoveryFinished(true)));
        assert!(!settled(&mut flow, Event::SaveStarted));
        assert_eq!(flow.state(), State::Choose);
        assert!(settled(&mut flow, Event::Select));
        assert!(settled(&mut flow, Event::RecoveryStarted));
        assert!(settled(&mut flow, Event::RecoveryFinished(true)));
        assert!(!settled(&mut flow, Event::RecoveryFinished(false)));
        assert_eq!(flow.state(), State::Guiding);
        assert!(settled(&mut flow, Event::SaveStarted));
        assert!(!settled(&mut flow, Event::Select));
        assert!(!settled(&mut flow, Event::Pause));
        assert!(!settled(&mut flow, Event::SaveStarted));
        assert_eq!(flow.state(), State::Saving);
        assert!(settled(
            &mut flow,
            Event::Observed(journey::State::Complete)
        ));
        assert!(!settled(&mut flow, Event::Observed(journey::State::Failed)));
        assert_eq!(flow.state(), State::Complete);
    }
    #[test]
    fn recovery_and_save_errors_have_distinct_retry_paths() {
        let mut state = Machine::new();
        assert!(settled(&mut state, Event::Select));
        assert!(settled(&mut state, Event::RecoveryStarted));
        assert!(settled(&mut state, Event::RecoveryFinished(false)));
        assert!(state.recovery());
        assert!(settled(&mut state, Event::Retry));
        assert!(settled(&mut state, Event::RecoveryFinished(true)));
        assert!(settled(&mut state, Event::SaveStarted));
        assert!(settled(&mut state, Event::Observed(journey::State::Failed)));
        assert!(!state.recovery());
        assert!(settled(&mut state, Event::Retry));
        assert_eq!(state.state(), State::Guiding);
    }
}

#[cfg(test)]
mod selection_tests {
    use super::tests::settled;
    use super::*;
    #[test]
    fn choose_again_after_completion_but_not_while_saving_or_recovering() {
        for source in [State::Choose, State::Paused, State::Complete] {
            let mut state = Machine::new();
            if source != State::Choose {
                assert!(settled(&mut state, Event::Select));
                match source {
                    State::Paused => {
                        assert!(settled(&mut state, Event::Pause));
                    }
                    State::Complete => {
                        assert!(settled(
                            &mut state,
                            Event::Observed(journey::State::Complete)
                        ));
                    }
                    State::Saving => {
                        assert!(settled(&mut state, Event::SaveStarted));
                    }
                    State::Recovering => {
                        assert!(settled(&mut state, Event::RecoveryStarted));
                    }
                    State::Returning => {
                        assert!(settled(
                            &mut state,
                            Event::Observed(journey::State::Returning)
                        ));
                    }
                    _ => unreachable!(),
                }
            }
            assert!(settled(&mut state, Event::Select));
            assert_eq!(state.state(), State::Guiding);
        }
        for source in [State::Saving, State::Recovering, State::Returning] {
            let mut state = Machine::new();
            if source != State::Choose {
                assert!(settled(&mut state, Event::Select));
                match source {
                    State::Paused => {
                        assert!(settled(&mut state, Event::Pause));
                    }
                    State::Complete => {
                        assert!(settled(
                            &mut state,
                            Event::Observed(journey::State::Complete)
                        ));
                    }
                    State::Saving => {
                        assert!(settled(&mut state, Event::SaveStarted));
                    }
                    State::Recovering => {
                        assert!(settled(&mut state, Event::RecoveryStarted));
                    }
                    State::Returning => {
                        assert!(settled(
                            &mut state,
                            Event::Observed(journey::State::Returning)
                        ));
                    }
                    _ => unreachable!(),
                }
            }
            assert!(!settled(&mut state, Event::Select));
            assert_eq!(state.state(), source);
        }
    }
}

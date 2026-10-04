//! Orders existing child journeys; completing a child never performs device I/O.
use crate::{runtime_recovery::Role, scope::Scope};
use statig::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

static GENERATION: AtomicU64 = AtomicU64::new(1);
fn generation() -> u64 {
    GENERATION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .expect("Peripheral journey generation exhausted")
}
struct Batch {
    roles: Vec<Role>,
    ticket: u64,
}
enum Event {
    Done(u64),
    RequestNext(u64),
    Detached(u64),
    Cancel,
}
#[state_machine(initial = "State::active(0)")]
impl Batch {
    #[state]
    fn active(&mut self, index: &mut usize, event: &Event, context: &mut bool) -> Outcome<State> {
        match event {
            Event::Done(ticket) if *ticket == self.ticket => {
                self.ticket = generation();
                *context = true;
                if *index + 1 == self.roles.len() {
                    Transition(State::complete())
                } else {
                    Transition(State::active(*index + 1))
                }
            }
            Event::Cancel => {
                self.ticket = generation();
                Transition(State::cancelled(*index))
            }
            Event::RequestNext(ticket)
                if *ticket == self.ticket && *index + 1 < self.roles.len() =>
            {
                self.ticket = generation();
                *context = true;
                Transition(State::waiting_detach(*index))
            }
            _ => Handled,
        }
    }
    #[state]
    fn waiting_detach(
        &mut self,
        index: &mut usize,
        event: &Event,
        context: &mut bool,
    ) -> Outcome<State> {
        match event {
            Event::Detached(ticket) if *ticket == self.ticket => {
                self.ticket = generation();
                *context = true;
                Transition(State::active(*index + 1))
            }
            Event::Cancel => {
                self.ticket = generation();
                Transition(State::cancelled(*index))
            }
            _ => Handled,
        }
    }
    #[state]
    fn complete(event: &Event) -> Outcome<State> {
        let _ = event;
        Handled
    }
    #[state]
    fn cancelled(index: &usize, event: &Event) -> Outcome<State> {
        let _ = (index, event);
        Handled
    }
}
pub(crate) struct Machine(StateMachine<Batch>);
impl Machine {
    pub fn new(scope: Scope) -> Self {
        Self(
            Batch {
                roles: scope.roles(),
                ticket: generation(),
            }
            .state_machine(),
        )
    }
    pub fn role(&self) -> Option<Role> {
        match self.0.state() {
            State::Active { index } | State::WaitingDetach { index } => {
                self.0.inner().roles.get(*index).copied()
            }
            _ => None,
        }
    }
    pub fn index(&self) -> usize {
        match self.0.state() {
            State::Active { index }
            | State::WaitingDetach { index }
            | State::Cancelled { index } => *index,
            State::Complete {} => self.len(),
        }
    }
    pub fn len(&self) -> usize {
        self.0.inner().roles.len()
    }
    pub fn complete(&self) -> bool {
        matches!(self.0.state(), State::Complete {})
    }
    pub fn ticket(&self) -> u64 {
        self.0.inner().ticket
    }
    pub fn done(&mut self, ticket: u64) -> bool {
        let mut accepted = false;
        self.0
            .handle_with_context(&Event::Done(ticket), &mut accepted);
        accepted
    }
    pub fn request_next(&mut self, ticket: u64) -> bool {
        let mut accepted = false;
        self.0
            .handle_with_context(&Event::RequestNext(ticket), &mut accepted);
        accepted
    }
    pub fn waiting_detach(&self) -> bool {
        matches!(self.0.state(), State::WaitingDetach { .. })
    }
    pub fn detached(&mut self, ticket: u64) -> bool {
        let mut accepted = false;
        self.0
            .handle_with_context(&Event::Detached(ticket), &mut accepted);
        accepted
    }
    pub fn cancel(&mut self) {
        self.0.handle_with_context(&Event::Cancel, &mut false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn whole_orders_one_child_at_a_time_and_rejects_duplicate_completion() {
        let mut batch = Machine::new(Scope::Whole);
        assert_eq!(batch.len(), 3);
        for (index, role) in [Role::Left, Role::Right, Role::Receiver]
            .into_iter()
            .enumerate()
        {
            assert_eq!(batch.index(), index);
            assert_eq!(batch.role(), Some(role));
            let ticket = batch.ticket();
            assert!(batch.done(ticket));
            assert!(!batch.done(ticket));
            assert_ne!(ticket, batch.ticket());
        }
        assert!(batch.complete());
        assert_eq!(batch.index(), 3);
        assert_eq!(batch.role(), None);
        assert!(!batch.done(batch.ticket()));
    }
    #[test]
    fn part_never_starts_another_part() {
        for role in [Role::Left, Role::Right, Role::Receiver] {
            let mut batch = Machine::new(Scope::Part(role));
            assert_eq!(batch.len(), 1);
            assert_eq!(batch.role(), Some(role));
            assert!(batch.done(batch.ticket()));
            assert!(batch.complete());
            assert_eq!(batch.role(), None);
        }
    }
    #[test]
    fn cancelled_and_replacement_journeys_reject_old_tickets() {
        let mut batch = Machine::new(Scope::Whole);
        let ticket = batch.ticket();
        batch.cancel();
        assert_ne!(ticket, batch.ticket());
        assert!(!batch.done(ticket));
        assert!(!batch.done(batch.ticket()));
        assert!(!batch.complete());
        assert_eq!(batch.role(), None);
        let mut replacement = Machine::new(Scope::Whole);
        assert!(!replacement.done(ticket));
        assert_eq!(replacement.role(), Some(Role::Left));
    }
    #[test]
    fn recovery_requires_detachment_before_next_part_and_rejects_late_events() {
        let mut batch = Machine::new(Scope::Whole);
        let opened = batch.ticket();
        assert!(!batch.detached(opened));
        assert!(batch.request_next(opened));
        assert!(batch.waiting_detach());
        assert_eq!(batch.role(), Some(Role::Left));
        assert!(!batch.done(batch.ticket()));
        assert!(!batch.detached(opened));
        let waiting = batch.ticket();
        assert!(batch.detached(waiting));
        assert!(!batch.waiting_detach());
        assert_eq!(batch.role(), Some(Role::Right));
        assert!(!batch.detached(waiting));
        assert!(batch.request_next(batch.ticket()));
        batch.cancel();
        assert!(!batch.detached(batch.ticket()));
        assert_eq!(batch.role(), None);
        let mut single = Machine::new(Scope::Part(Role::Left));
        assert!(!single.request_next(single.ticket()));
        assert!(single.done(single.ticket()));
    }
}

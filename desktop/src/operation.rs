//! Async effects are executed by the UI; this machine only accepts their lifecycle events.
//! A completion must carry the ticket issued when the operation began.
use crate::{firmware_journey::View, runtime_recovery::Role};
use statig::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Pairing,
    PrepareFirmware,
    Firmware,
    Backup,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket(u64);
#[derive(Clone)]
pub struct Pending {
    ticket: Ticket,
    kind: Kind,
    role: Option<Role>,
    firmware: Option<View>,
}
enum Event {
    Begin(Pending),
    Complete(Ticket),
    Fail(String),
    ClearError,
}
#[derive(Default)]
struct Storage;
#[state_machine(initial = "State::idle()")]
impl Storage {
    #[state]
    fn idle(event: &Event, context: &mut bool) -> Outcome<State> {
        match event {
            Event::Begin(pending) => {
                *context = true;
                Transition(State::running(pending.clone()))
            }
            Event::Fail(message) => Transition(State::failed(message.clone())),
            _ => Handled,
        }
    }
    #[state]
    fn failed(message: &String, event: &Event, context: &mut bool) -> Outcome<State> {
        let _ = message;
        match event {
            Event::Begin(pending) => {
                *context = true;
                Transition(State::running(pending.clone()))
            }
            Event::Fail(message) => Transition(State::failed(message.clone())),
            Event::ClearError => Transition(State::idle()),
            _ => Handled,
        }
    }
    #[state]
    fn running(pending: &Pending, event: &Event, context: &mut bool) -> Outcome<State> {
        match event {
            Event::Complete(ticket) if *ticket == pending.ticket => {
                *context = true;
                Transition(State::idle())
            }
            _ => Handled,
        }
    }
}
pub struct Operation {
    machine: StateMachine<Storage>,
    sequence: u64,
}
impl Default for Operation {
    fn default() -> Self {
        Self {
            machine: Storage.state_machine(),
            sequence: 0,
        }
    }
}
impl Operation {
    pub fn busy(&self) -> bool {
        matches!(self.machine.state(), State::Running { .. })
    }
    pub fn begin(
        &mut self,
        kind: Kind,
        role: Option<Role>,
        firmware: Option<View>,
    ) -> Option<Ticket> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .expect("Operation counter exhausted");
        let ticket = Ticket(self.sequence);
        let mut accepted = false;
        self.machine.handle_with_context(
            &Event::Begin(Pending {
                ticket,
                kind,
                role,
                firmware,
            }),
            &mut accepted,
        );
        if accepted {
            crate::diagnostics::event(
                crate::diagnostics::Category::Operation,
                crate::diagnostics::Event::Started,
            );
        }
        accepted.then_some(ticket)
    }
    pub fn complete(&mut self, ticket: Ticket) -> bool {
        let mut accepted = false;
        self.machine
            .handle_with_context(&Event::Complete(ticket), &mut accepted);
        if accepted {
            crate::diagnostics::event(
                crate::diagnostics::Category::Operation,
                crate::diagnostics::Event::Completed,
            );
        }
        accepted
    }
    pub fn error(&self) -> Option<&String> {
        match self.machine.state() {
            State::Failed { message } => Some(message),
            _ => None,
        }
    }
    pub fn fail(&mut self, message: String) {
        crate::diagnostics::event(
            crate::diagnostics::Category::Operation,
            crate::diagnostics::Event::Failed,
        );
        self.machine
            .handle_with_context(&Event::Fail(message), &mut false);
    }
    pub fn clear_error(&mut self) {
        self.machine
            .handle_with_context(&Event::ClearError, &mut false);
    }
    pub fn role(&self, kind: Kind) -> Option<Role> {
        match self.machine.state() {
            State::Running { pending } if pending.kind == kind => pending.role,
            _ => None,
        }
    }
    pub fn firmware_view(&self) -> Option<View> {
        match self.machine.state() {
            State::Running { pending } => pending.firmware.clone(),
            _ => None,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn errors_are_states_and_cannot_replace_an_active_job() {
        let mut flow = Operation::default();
        flow.fail("failed".into());
        assert_eq!(flow.error().map(String::as_str), Some("failed"));
        let ticket = flow.begin(Kind::Backup, Some(Role::Right), None).unwrap();
        assert!(flow.error().is_none());
        flow.fail("late error".into());
        flow.clear_error();
        assert!(flow.busy());
        assert!(flow.complete(ticket));
        flow.fail("new failure".into());
        flow.clear_error();
        assert!(flow.error().is_none());
    }
    #[test]
    fn duplicate_and_old_completions_cannot_finish_a_new_job() {
        let mut flow = Operation::default();
        let first = flow.begin(Kind::Backup, Some(Role::Right), None).unwrap();
        assert!(flow.begin(Kind::Pairing, None, None).is_none());
        assert!(flow.complete(first));
        let next = flow.begin(Kind::Firmware, Some(Role::Left), None).unwrap();
        assert!(!flow.complete(first));
        assert!(flow.busy());
        assert_eq!(flow.role(Kind::Firmware), Some(Role::Left));
        assert!(flow.complete(next));
        assert!(!flow.complete(next));
    }
}

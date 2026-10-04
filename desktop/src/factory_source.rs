//! Factory image provenance selection. Paths are drafts, never proof of image validity.
//! The release loader validates contents separately before enabling transfer.
use crate::{runtime_recovery::Role, scope::Scope};
use statig::prelude::*;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static GENERATION: AtomicU64 = AtomicU64::new(1);
fn next_generation() -> u64 {
    GENERATION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .expect("Factory source generation exhausted")
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Source {
    #[default]
    Backups,
    Supplied,
}
#[derive(Default, Debug)]
struct Files([Option<PathBuf>; 3]);
fn index(role: Role) -> usize {
    match role {
        Role::Left => 0,
        Role::Right => 1,
        Role::Receiver => 2,
    }
}
mod machine {
    use super::*;
    pub struct Selection {
        pub generation: u64,
    }
    pub enum Event {
        Select(Source),
        Accept(u64, Role, PathBuf),
    }
    #[state_machine(initial = "State::backups()", state(derive(Debug)))]
    impl Selection {
        #[state]
        fn backups(&mut self, event: &Event, context: &mut bool) -> Outcome<State> {
            match event {
                Event::Select(Source::Supplied) => {
                    self.generation = next_generation();
                    *context = true;
                    Transition(State::supplied(Files::default()))
                }
                _ => Handled,
            }
        }
        #[state]
        fn supplied(
            &mut self,
            files: &mut Files,
            event: &Event,
            context: &mut bool,
        ) -> Outcome<State> {
            match event {
                Event::Select(Source::Backups) => {
                    self.generation = next_generation();
                    *context = true;
                    Transition(State::backups())
                }
                Event::Accept(ticket, role, path) if *ticket == self.generation => {
                    files.0[index(*role)] = Some(path.clone());
                    *context = true;
                    Handled
                }
                _ => Handled,
            }
        }
    }
}
pub(crate) struct Machine(StateMachine<machine::Selection>);
impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}
impl Machine {
    pub fn new() -> Self {
        Self(
            machine::Selection {
                generation: next_generation(),
            }
            .state_machine(),
        )
    }
    pub fn source(&self) -> Source {
        match self.0.state() {
            machine::State::Backups {} => Source::Backups,
            machine::State::Supplied { .. } => Source::Supplied,
        }
    }
    pub fn ticket(&self) -> u64 {
        self.0.inner().generation
    }
    /// A changed source discards every draft path. Re-selecting the same source is a no-op.
    pub fn select(&mut self, source: Source) {
        self.0
            .handle_with_context(&machine::Event::Select(source), &mut false);
    }
    pub fn accept(&mut self, ticket: u64, role: Role, path: PathBuf) -> bool {
        let mut accepted = false;
        self.0
            .handle_with_context(&machine::Event::Accept(ticket, role, path), &mut accepted);
        accepted
    }
    pub fn file(&self, role: Role) -> Option<&Path> {
        match self.0.state() {
            machine::State::Supplied { files } => files.0[index(role)].as_deref(),
            machine::State::Backups {} => None,
        }
    }
    /// Validated release completeness is required for either source. Supplied
    /// images additionally require an explicit accepted path for every role.
    #[cfg(test)]
    pub fn ready(&self, validated_release_complete: bool) -> bool {
        self.ready_for(validated_release_complete, Scope::Whole)
    }
    /// The validity argument covers the selected roles, not unrelated parts.
    pub fn ready_for(&self, validated_release_complete: bool, scope: Scope) -> bool {
        validated_release_complete
            && match self.0.state() {
                machine::State::Backups {} => true,
                machine::State::Supplied { files } => match scope {
                    Scope::Whole => files.0.iter().all(Option::is_some),
                    Scope::Part(role) => files.0[index(role)].is_some(),
                },
            }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_changes_clear_all_paths_and_reject_old_picker_callbacks() {
        let mut selection = Machine::new();
        assert_eq!(selection.source(), Source::Backups);
        let backup_ticket = selection.ticket();
        assert!(!selection.accept(backup_ticket, Role::Left, "left.uf2".into()));
        selection.select(Source::Supplied);
        let ticket = selection.ticket();
        assert!(ticket > backup_ticket);
        for role in [Role::Left, Role::Right, Role::Receiver] {
            assert!(selection.accept(ticket, role, "firmware.uf2".into()));
        }
        selection.select(Source::Supplied);
        assert_eq!(selection.ticket(), ticket);
        assert!(selection.ready(true));
        assert!(!selection.ready(false));
        selection.select(Source::Backups);
        assert!(selection.ticket() > ticket);
        selection.select(Source::Supplied);
        for role in [Role::Left, Role::Right, Role::Receiver] {
            assert!(selection.file(role).is_none());
            assert!(!selection.accept(ticket, role, "late.uf2".into()));
        }
        assert!(!selection.ready(true));
    }
    #[test]
    fn supplied_requires_each_role_even_when_backups_are_complete() {
        let mut selection = Machine::new();
        assert!(!selection.ready(false));
        assert!(selection.ready(true));
        selection.select(Source::Supplied);
        let ticket = selection.ticket();
        for role in [Role::Left, Role::Right] {
            selection.accept(ticket, role, "keyboard.uf2".into());
            assert!(!selection.ready(true));
        }
        selection.accept(ticket, Role::Receiver, "dongle.uf2".into());
        assert_eq!(
            selection.file(Role::Receiver),
            Some(Path::new("dongle.uf2"))
        );
        assert!(selection.ready(true));
        assert!(!selection.ready(false));
    }
    #[test]
    fn supplied_part_needs_only_its_accepted_validated_source() {
        let mut selection = Machine::new();
        selection.select(Source::Supplied);
        selection.accept(selection.ticket(), Role::Right, "right.uf2".into());
        assert!(selection.ready_for(true, Scope::Part(Role::Right)));
        assert!(!selection.ready_for(false, Scope::Part(Role::Right)));
        assert!(!selection.ready_for(true, Scope::Part(Role::Left)));
        assert!(!selection.ready(true));
    }
    #[test]
    fn replacement_machine_does_not_accept_previous_machine_ticket() {
        let mut old = Machine::new();
        old.select(Source::Supplied);
        let mut current = Machine::new();
        current.select(Source::Supplied);
        assert!(!current.accept(old.ticket(), Role::Left, "old.uf2".into()));
        assert!(current.file(Role::Left).is_none());
    }
}

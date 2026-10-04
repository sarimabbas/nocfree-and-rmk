//! Page entry is passive. Only Next accepts a setup selection and starts a journey.
use crate::{runtime_recovery::Role, scope::Scope};
use statig::prelude::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Home,
    Backups,
    Recovery,
    Pairing,
    Firmware,
    Restore,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    Backup(Scope),
    Recovery(Scope),
    Pairing,
    Firmware(Scope),
    Restore(Scope),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Readiness {
    #[default]
    Unknown,
    Needed,
    AlreadyLatest,
    AlreadyFactory,
}
impl Readiness {
    fn permits_start(self) -> bool {
        matches!(self, Self::Unknown | Self::Needed)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Choose(Readiness),
    Setup { scope: Scope, readiness: Readiness },
    Active { scope: Scope },
}
enum Event {
    Navigate(Page),
    #[cfg(test)]
    Select(Scope),
    Available([bool; 3]),
    Checked(Role, bool),
    ChooseNext,
    Next,
    Previous,
    Reset,
    Observe(Readiness),
}
#[derive(Default)]
struct Storage {
    available: [bool; 3],
    checked: [bool; 3],
    customized: bool,
}
fn role_index(role: Role) -> usize {
    match role {
        Role::Left => 0,
        Role::Right => 1,
        Role::Receiver => 2,
    }
}
impl Storage {
    fn draft(&self) -> Option<Scope> {
        Scope::from_roles(
            [Role::Left, Role::Right, Role::Receiver]
                .into_iter()
                .filter(|role| {
                    self.checked[role_index(*role)] && self.available[role_index(*role)]
                }),
        )
    }
    fn connected(&self, scope: Scope) -> bool {
        scope
            .roles()
            .into_iter()
            .all(|role| self.available[role_index(role)])
    }
    fn reset_selection(&mut self) {
        self.checked = self.available;
        self.customized = false;
    }
}
fn select(page: &Page) -> Outcome<State> {
    let stage = if *page == Page::Pairing {
        Stage::Setup {
            scope: Scope::Whole,
            readiness: Readiness::Unknown,
        }
    } else {
        Stage::Choose(Readiness::Unknown)
    };
    Transition(match page {
        Page::Home | Page::Backups => State::backups(stage),
        Page::Recovery => State::recovery(stage),
        Page::Pairing => State::pairing(stage),
        Page::Firmware => State::firmware(stage),
        Page::Restore => State::restore(stage),
    })
}
fn entry(
    storage: &mut Storage,
    page: Page,
    stage: &mut Stage,
    event: &Event,
    context: &mut Option<Start>,
) -> Outcome<State> {
    match event {
        Event::Reset => {
            storage.reset_selection();
            select(&page)
        }
        Event::Navigate(next) if *next != page => {
            storage.reset_selection();
            select(next)
        }
        Event::Available(available) => {
            storage.available = *available;
            if matches!(stage, Stage::Choose(_)) {
                for (index, connected) in available.iter().copied().enumerate() {
                    if !connected || !storage.customized {
                        storage.checked[index] = connected;
                    }
                }
            }
            Handled
        }
        Event::Checked(role, checked) if matches!(stage, Stage::Choose(_)) => {
            let index = role_index(*role);
            if storage.available[index] {
                storage.checked[index] = *checked;
                storage.customized = true;
            }
            Handled
        }
        Event::ChooseNext if page != Page::Pairing => {
            if let Stage::Choose(readiness) = *stage
                && let Some(scope) = storage.draft()
            {
                *stage = Stage::Setup { scope, readiness };
            }
            Handled
        }
        #[cfg(test)]
        Event::Select(scope) if page != Page::Pairing => {
            if let Stage::Choose(readiness) = *stage
                && storage.connected(*scope)
            {
                storage.checked =
                    [Role::Left, Role::Right, Role::Receiver].map(|role| scope.contains(role));
                storage.customized = true;
                *stage = Stage::Setup {
                    scope: *scope,
                    readiness,
                };
            }
            Handled
        }
        Event::Observe(latest) if matches!(page, Page::Firmware | Page::Restore) => {
            match stage {
                Stage::Choose(readiness) | Stage::Setup { readiness, .. } => *readiness = *latest,
                Stage::Active { .. } => {}
            }
            Handled
        }
        Event::Previous if page != Page::Pairing && matches!(stage, Stage::Setup { .. }) => {
            for index in 0..3 {
                storage.checked[index] &= storage.available[index];
            }
            *stage = Stage::Choose(Readiness::Unknown);
            Handled
        }
        Event::Next => {
            if let Stage::Setup { scope, readiness } = *stage
                && readiness.permits_start()
                && (page == Page::Pairing || storage.connected(scope))
            {
                *context = Some(match page {
                    Page::Home | Page::Backups => Start::Backup(scope),
                    Page::Recovery => Start::Recovery(scope),
                    Page::Pairing => Start::Pairing,
                    Page::Firmware => Start::Firmware(scope),
                    Page::Restore => Start::Restore(scope),
                });
                *stage = Stage::Active { scope };
            }
            Handled
        }
        _ => Handled,
    }
}
#[state_machine(initial = "State::backups(Stage::Choose(Readiness::Unknown))")]
impl Storage {
    #[state]
    fn backups(
        &mut self,
        stage: &mut Stage,
        event: &Event,
        context: &mut Option<Start>,
    ) -> Outcome<State> {
        entry(self, Page::Backups, stage, event, context)
    }
    #[state]
    fn recovery(
        &mut self,
        stage: &mut Stage,
        event: &Event,
        context: &mut Option<Start>,
    ) -> Outcome<State> {
        entry(self, Page::Recovery, stage, event, context)
    }
    #[state]
    fn pairing(
        &mut self,
        stage: &mut Stage,
        event: &Event,
        context: &mut Option<Start>,
    ) -> Outcome<State> {
        entry(self, Page::Pairing, stage, event, context)
    }
    #[state]
    fn restore(
        &mut self,
        stage: &mut Stage,
        event: &Event,
        context: &mut Option<Start>,
    ) -> Outcome<State> {
        entry(self, Page::Restore, stage, event, context)
    }
    #[state]
    fn firmware(
        &mut self,
        stage: &mut Stage,
        event: &Event,
        context: &mut Option<Start>,
    ) -> Outcome<State> {
        entry(self, Page::Firmware, stage, event, context)
    }
}
pub struct Navigation(StateMachine<Storage>);
impl Default for Navigation {
    fn default() -> Self {
        Self(Storage::default().state_machine())
    }
}
impl Navigation {
    fn snapshot(&self) -> (Page, Stage) {
        match self.0.state() {
            State::Backups { stage } => (Page::Backups, *stage),
            State::Recovery { stage } => (Page::Recovery, *stage),
            State::Pairing { stage } => (Page::Pairing, *stage),
            State::Firmware { stage } => (Page::Firmware, *stage),
            State::Restore { stage } => (Page::Restore, *stage),
        }
    }
    pub fn observe_available(&mut self, available: [bool; 3]) {
        self.0
            .handle_with_context(&Event::Available(available), &mut None);
    }
    pub fn available(&self, role: Role) -> bool {
        self.0.inner().available[role_index(role)]
    }
    pub fn checked(&self, role: Role) -> bool {
        self.0.inner().checked[role_index(role)]
    }
    pub fn set_checked(&mut self, role: Role, checked: bool) {
        self.0
            .handle_with_context(&Event::Checked(role, checked), &mut None);
    }
    pub fn draft_scope(&self) -> Option<Scope> {
        self.0.inner().draft()
    }
    pub fn choose_next(&mut self) -> bool {
        let before = self.choosing();
        self.0.handle_with_context(&Event::ChooseNext, &mut None);
        before && !self.choosing()
    }
    pub fn page(&self) -> Page {
        self.snapshot().0
    }
    /// Idle setup includes the picker. Only a selected Setup can start.
    pub fn setup(&self) -> bool {
        !matches!(self.snapshot().1, Stage::Active { .. })
    }
    pub fn choosing(&self) -> bool {
        matches!(self.snapshot().1, Stage::Choose(_))
    }
    pub fn scope(&self) -> Option<Scope> {
        match self.snapshot().1 {
            Stage::Setup { scope, .. } | Stage::Active { scope } => Some(scope),
            Stage::Choose(_) => None,
        }
    }
    #[cfg(test)]
    pub fn selected(&self) -> Option<Role> {
        match self.scope() {
            Some(Scope::Part(role)) => Some(role),
            _ => None,
        }
    }
    pub fn readiness(&self) -> Readiness {
        match self.snapshot().1 {
            Stage::Choose(readiness) | Stage::Setup { readiness, .. } => readiness,
            Stage::Active { .. } => Readiness::Unknown,
        }
    }
    pub fn observe(&mut self, readiness: Readiness) {
        self.0
            .handle_with_context(&Event::Observe(readiness), &mut None);
    }
    pub fn can_start(&self) -> bool {
        matches!(self.snapshot().1, Stage::Setup { scope, readiness } if readiness.permits_start() && (self.page() == Page::Pairing || self.0.inner().connected(scope)))
    }
    pub fn navigate(&mut self, page: Page) {
        self.0
            .handle_with_context(&Event::Navigate(page), &mut None);
    }
    pub fn reset(&mut self) {
        self.0.handle_with_context(&Event::Reset, &mut None);
    }
    #[cfg(test)]
    pub fn select(&mut self, role: Role) {
        self.select_scope(Scope::Part(role));
    }
    #[cfg(test)]
    pub fn select_scope(&mut self, scope: Scope) {
        self.0.handle_with_context(&Event::Select(scope), &mut None);
    }
    pub fn previous(&mut self) -> bool {
        let accepted =
            self.page() != Page::Pairing && matches!(self.snapshot().1, Stage::Setup { .. });
        self.0.handle_with_context(&Event::Previous, &mut None);
        accepted
    }
    pub fn next(&mut self) -> Option<Start> {
        let mut effect = None;
        self.0.handle_with_context(&Event::Next, &mut effect);
        effect
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkbox_selection_is_passive_and_empty_selection_cannot_proceed() {
        let mut nav = Navigation::default();
        assert_eq!(nav.draft_scope(), None);
        assert!(!nav.choose_next());
        nav.observe_available([true; 3]);
        assert_eq!(nav.draft_scope(), Some(Scope::Whole));
        for role in [Role::Left, Role::Right, Role::Receiver] {
            nav.set_checked(role, false);
            assert!(nav.choosing());
            assert_eq!(nav.next(), None);
        }
        assert_eq!(nav.draft_scope(), None);
        assert!(!nav.choose_next());
        nav.set_checked(Role::Left, true);
        nav.set_checked(Role::Receiver, true);
        assert_eq!(
            nav.draft_scope(),
            Some(Scope::Pair(Role::Left, Role::Receiver))
        );
        assert!(nav.choose_next());
        assert!(!nav.choose_next());
        assert_eq!(
            nav.next(),
            Some(Start::Backup(Scope::Pair(Role::Left, Role::Receiver)))
        );
        assert_eq!(nav.next(), None);
    }
    #[test]
    fn available_parts_default_selected_until_owner_customizes() {
        let mut nav = Navigation::default();
        nav.observe_available([true, false, false]);
        assert_eq!(nav.draft_scope(), Some(Scope::Part(Role::Left)));
        nav.set_checked(Role::Right, true);
        assert!(!nav.checked(Role::Right));
        nav.observe_available([true, true, false]);
        assert!(nav.checked(Role::Right));
        nav.set_checked(Role::Right, false);
        nav.observe_available([true; 3]);
        assert!(!nav.checked(Role::Receiver));
        assert_eq!(nav.draft_scope(), Some(Scope::Part(Role::Left)));
        nav.observe_available([false, true, true]);
        assert!(!nav.checked(Role::Left));
        assert_eq!(nav.draft_scope(), None);
        nav.observe_available([true; 3]);
        assert!(!nav.checked(Role::Left));
    }
    #[test]
    fn availability_gates_setup_but_active_disconnect_never_changes_plan() {
        let mut nav = Navigation::default();
        nav.observe_available([true; 3]);
        assert!(nav.choose_next());
        nav.observe_available([true, false, true]);
        assert_eq!(nav.scope(), Some(Scope::Whole));
        assert!(!nav.can_start());
        assert_eq!(nav.next(), None);
        nav.observe_available([true; 3]);
        assert_eq!(nav.next(), Some(Start::Backup(Scope::Whole)));
        nav.observe_available([false; 3]);
        nav.set_checked(Role::Left, false);
        assert_eq!(nav.scope(), Some(Scope::Whole));
        assert_eq!(nav.next(), None);
        nav.reset();
        assert_eq!(nav.draft_scope(), None);
    }
    #[test]
    fn every_nonempty_subset_has_an_explicit_setup_then_start() {
        for mask in 1..8 {
            let mut nav = Navigation::default();
            let available = std::array::from_fn(|index| mask & (1 << index) != 0);
            nav.observe_available(available);
            let scope = nav.draft_scope().unwrap();
            assert_eq!(
                scope.roles().len(),
                available.into_iter().filter(|v| *v).count()
            );
            assert_eq!(nav.next(), None);
            assert!(nav.choose_next());
            assert_eq!(nav.next(), Some(Start::Backup(scope)));
        }
    }
    #[test]
    fn all_task_cards_are_passive_and_next_starts_selected_scope_once() {
        for page in [
            Page::Home,
            Page::Backups,
            Page::Recovery,
            Page::Firmware,
            Page::Restore,
        ] {
            for scope in [
                Scope::Whole,
                Scope::Part(Role::Left),
                Scope::Part(Role::Right),
                Scope::Part(Role::Receiver),
            ] {
                let mut nav = Navigation::default();
                nav.observe_available([true; 3]);
                nav.navigate(page);
                assert!(nav.choosing());
                assert!(nav.setup());
                assert!(!nav.can_start());
                assert_eq!(nav.next(), None);
                nav.select_scope(scope);
                assert!(!nav.choosing());
                assert!(nav.setup());
                assert_eq!(nav.scope(), Some(scope));
                assert!(nav.can_start());
                let expected = match page {
                    Page::Home | Page::Backups => Start::Backup(scope),
                    Page::Recovery => Start::Recovery(scope),
                    Page::Firmware => Start::Firmware(scope),
                    Page::Restore => Start::Restore(scope),
                    _ => unreachable!(),
                };
                assert_eq!(nav.next(), Some(expected));
                assert_eq!(nav.next(), None);
                assert!(!nav.setup());
                assert_eq!(nav.scope(), Some(scope));
                nav.select_scope(Scope::Part(Role::Left));
                nav.observe(Readiness::AlreadyLatest);
                assert_eq!(nav.scope(), Some(scope));
                assert!(!nav.previous());
                nav.navigate(nav.page());
                assert!(!nav.setup());
                nav.reset();
                assert!(nav.choosing());
                assert_eq!(nav.scope(), None);
            }
        }
    }
    #[test]
    fn previous_returns_to_picker_without_starting_and_clears_scope() {
        let mut nav = Navigation::default();
        nav.observe_available([true; 3]);
        nav.navigate(Page::Firmware);
        nav.select_scope(Scope::Whole);
        nav.observe(Readiness::AlreadyLatest);
        assert!(nav.previous());
        assert!(nav.choosing());
        assert_eq!(nav.scope(), None);
        assert_eq!(nav.readiness(), Readiness::Unknown);
        assert_eq!(nav.next(), None);
        assert!(!nav.previous());
        nav.select(Role::Right);
        assert_eq!(nav.selected(), Some(Role::Right));
        assert_eq!(nav.next(), Some(Start::Firmware(Scope::Part(Role::Right))));
    }
    #[test]
    fn readiness_updates_picker_and_setup_but_cannot_change_active_scope() {
        for (page, already) in [
            (Page::Firmware, Readiness::AlreadyLatest),
            (Page::Restore, Readiness::AlreadyFactory),
        ] {
            let mut nav = Navigation::default();
            nav.observe_available([true; 3]);
            nav.navigate(page);
            nav.observe(already);
            assert!(nav.choosing());
            nav.select_scope(Scope::Whole);
            assert_eq!(nav.readiness(), already);
            assert!(!nav.can_start());
            assert_eq!(nav.next(), None);
            nav.observe(Readiness::Needed);
            assert!(nav.can_start());
            assert!(nav.next().is_some());
            nav.observe(already);
            nav.select(Role::Right);
            assert_eq!(nav.scope(), Some(Scope::Whole));
            assert_eq!(nav.next(), None);
        }
    }
    #[test]
    fn pairing_has_no_picker_and_navigation_drops_old_scope() {
        let mut nav = Navigation::default();
        nav.observe_available([true; 3]);
        nav.select(Role::Receiver);
        nav.navigate(Page::Pairing);
        assert!(!nav.choosing());
        assert!(nav.setup());
        assert_eq!(nav.scope(), Some(Scope::Whole));
        nav.select(Role::Right);
        assert_eq!(nav.scope(), Some(Scope::Whole));
        assert!(!nav.previous());
        assert_eq!(nav.next(), Some(Start::Pairing));
        assert_eq!(nav.next(), None);
        nav.navigate(Page::Home);
        assert_eq!(nav.page(), Page::Backups);
        assert!(nav.choosing());
        assert_eq!(nav.scope(), None);
    }
}

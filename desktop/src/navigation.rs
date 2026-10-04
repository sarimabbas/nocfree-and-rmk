//! Page entry is passive. Only Next accepts a setup selection and starts a journey.
use crate::runtime_recovery::Role;
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
    Backup(Role),
    Recovery(Role),
    Pairing,
    Firmware,
    Restore,
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
    Setup {
        selected: Option<Role>,
        readiness: Readiness,
    },
    Active,
}
enum Event {
    Navigate(Page),
    Select(Role),
    Next,
    Reset,
    Observe(Readiness),
}
#[derive(Default)]
struct Storage;
fn select(page: &Page) -> Outcome<State> {
    let stage = Stage::Setup {
        selected: None,
        readiness: Readiness::Unknown,
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
    page: Page,
    stage: &mut Stage,
    event: &Event,
    context: &mut Option<Start>,
) -> Outcome<State> {
    match event {
        Event::Reset => select(&page),
        Event::Navigate(next) if *next != page => select(next),
        Event::Select(role)
            if matches!(page, Page::Home | Page::Backups | Page::Recovery)
                && matches!(stage, Stage::Setup { .. }) =>
        {
            if let Stage::Setup { selected, .. } = stage {
                *selected = Some(*role);
            }
            Handled
        }
        Event::Observe(latest) if matches!(page, Page::Firmware | Page::Restore) => {
            if let Stage::Setup { readiness, .. } = stage {
                *readiness = *latest;
            }
            Handled
        }
        Event::Next if matches!(stage, Stage::Setup { readiness, .. } if readiness.permits_start()) =>
        {
            let start = match (page, *stage) {
                (
                    Page::Home | Page::Backups,
                    Stage::Setup {
                        selected: Some(role),
                        ..
                    },
                ) => Some(Start::Backup(role)),
                (
                    Page::Recovery,
                    Stage::Setup {
                        selected: Some(role),
                        ..
                    },
                ) => Some(Start::Recovery(role)),
                (Page::Pairing, _) => Some(Start::Pairing),
                (Page::Firmware, _) => Some(Start::Firmware),
                (Page::Restore, _) => Some(Start::Restore),
                _ => None,
            };
            if let Some(start) = start {
                *stage = Stage::Active;
                *context = Some(start);
            }
            Handled
        }
        _ => Handled,
    }
}
#[state_machine(
    initial = "State::backups(Stage::Setup { selected: None, readiness: Readiness::Unknown })"
)]
impl Storage {
    #[state]
    fn backups(stage: &mut Stage, event: &Event, context: &mut Option<Start>) -> Outcome<State> {
        entry(Page::Backups, stage, event, context)
    }
    #[state]
    fn recovery(stage: &mut Stage, event: &Event, context: &mut Option<Start>) -> Outcome<State> {
        entry(Page::Recovery, stage, event, context)
    }
    #[state]
    fn pairing(stage: &mut Stage, event: &Event, context: &mut Option<Start>) -> Outcome<State> {
        entry(Page::Pairing, stage, event, context)
    }
    #[state]
    fn restore(stage: &mut Stage, event: &Event, context: &mut Option<Start>) -> Outcome<State> {
        entry(Page::Restore, stage, event, context)
    }
    #[state]
    fn firmware(stage: &mut Stage, event: &Event, context: &mut Option<Start>) -> Outcome<State> {
        entry(Page::Firmware, stage, event, context)
    }
}
pub struct Navigation(StateMachine<Storage>);
impl Default for Navigation {
    fn default() -> Self {
        Self(Storage.state_machine())
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
    pub fn page(&self) -> Page {
        self.snapshot().0
    }
    pub fn setup(&self) -> bool {
        matches!(self.snapshot().1, Stage::Setup { .. })
    }
    pub fn selected(&self) -> Option<Role> {
        match self.snapshot().1 {
            Stage::Setup { selected, .. } => selected,
            _ => None,
        }
    }
    pub fn readiness(&self) -> Readiness {
        match self.snapshot().1 {
            Stage::Setup { readiness, .. } => readiness,
            Stage::Active => Readiness::Unknown,
        }
    }
    pub fn observe(&mut self, readiness: Readiness) {
        self.0
            .handle_with_context(&Event::Observe(readiness), &mut None);
    }
    pub fn can_start(&self) -> bool {
        self.setup()
            && self.readiness().permits_start()
            && (matches!(self.page(), Page::Pairing | Page::Firmware | Page::Restore)
                || self.selected().is_some())
    }
    pub fn navigate(&mut self, page: Page) {
        self.0
            .handle_with_context(&Event::Navigate(page), &mut None);
    }
    pub fn reset(&mut self) {
        self.0.handle_with_context(&Event::Reset, &mut None);
    }
    pub fn select(&mut self, role: Role) {
        self.0.handle_with_context(&Event::Select(role), &mut None);
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
    fn every_page_is_passive_until_next_and_duplicate_next_cannot_start_twice() {
        for page in [
            Page::Home,
            Page::Backups,
            Page::Recovery,
            Page::Pairing,
            Page::Firmware,
            Page::Restore,
        ] {
            let mut nav = Navigation::default();
            nav.navigate(page);
            assert!(nav.setup());
            if matches!(page, Page::Home | Page::Backups | Page::Recovery) {
                assert!(!nav.can_start());
                assert_eq!(nav.next(), None);
                nav.select(Role::Right);
                assert!(nav.setup());
            }
            assert!(nav.can_start());
            assert!(nav.next().is_some());
            assert!(!nav.setup());
            assert_eq!(nav.next(), None);
            nav.navigate(nav.page());
            assert!(!nav.setup());
            nav.reset();
            assert!(nav.setup());
            assert_eq!(nav.selected(), None);
        }
    }
    #[test]
    fn current_targets_block_next_but_new_facts_can_enable_setup() {
        for (page, already) in [
            (Page::Firmware, Readiness::AlreadyLatest),
            (Page::Restore, Readiness::AlreadyFactory),
        ] {
            let mut nav = Navigation::default();
            nav.navigate(page);
            assert_eq!(nav.readiness(), Readiness::Unknown);
            assert!(nav.can_start());
            nav.observe(already);
            assert!(nav.setup());
            assert_eq!(nav.readiness(), already);
            assert!(!nav.can_start());
            assert_eq!(nav.next(), None);
            nav.observe(Readiness::Needed);
            assert_eq!(nav.readiness(), Readiness::Needed);
            assert!(nav.can_start());
            assert!(nav.next().is_some());
            nav.observe(already);
            assert!(!nav.setup());
            assert_eq!(nav.next(), None);
        }
    }
    #[test]
    fn observations_are_passive_scoped_and_do_not_cross_navigation() {
        let mut nav = Navigation::default();
        nav.observe(Readiness::AlreadyLatest);
        assert_eq!(nav.readiness(), Readiness::Unknown);
        nav.select(Role::Left);
        assert_eq!(nav.next(), Some(Start::Backup(Role::Left)));
        nav.observe(Readiness::Needed);
        assert!(!nav.setup());
        nav.navigate(Page::Firmware);
        nav.observe(Readiness::AlreadyLatest);
        nav.navigate(Page::Firmware);
        assert_eq!(nav.readiness(), Readiness::AlreadyLatest);
        nav.navigate(Page::Restore);
        assert_eq!(nav.readiness(), Readiness::Unknown);
        assert_eq!(nav.next(), Some(Start::Restore));
        nav.observe(Readiness::AlreadyFactory);
        assert!(!nav.setup());
        nav.reset();
        assert!(nav.setup());
        assert_eq!(nav.readiness(), Readiness::Unknown);
    }
    #[test]
    fn selection_is_draft_and_does_not_leak_to_another_page() {
        let mut nav = Navigation::default();
        nav.select(Role::Receiver);
        nav.navigate(Page::Recovery);
        assert_eq!(nav.selected(), None);
        nav.select(Role::Left);
        assert_eq!(nav.next(), Some(Start::Recovery(Role::Left)));
        nav.select(Role::Right);
        assert_eq!(nav.next(), None);
        nav.navigate(Page::Pairing);
        assert!(nav.setup());
        assert_eq!(nav.next(), Some(Start::Pairing));
        nav.navigate(Page::Home);
        assert_eq!(nav.page(), Page::Backups);
        assert!(nav.setup());
        assert_eq!(nav.next(), None);
    }
}

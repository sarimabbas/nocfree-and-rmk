//! The visible page is a projection of this machine, never a separately assigned flag.
use statig::prelude::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Home,
    Backups,
    Recovery,
    Pairing,
    Firmware,
}
#[derive(Default)]
struct Storage;
fn select(page: &Page) -> Outcome<State> {
    Transition(match page {
        Page::Home => State::home(),
        Page::Backups => State::backups(),
        Page::Recovery => State::recovery(),
        Page::Pairing => State::pairing(),
        Page::Firmware => State::firmware(),
    })
}
#[state_machine(initial = "State::backups()")]
impl Storage {
    #[state]
    fn home(event: &Page) -> Outcome<State> {
        select(event)
    }
    #[state]
    fn backups(event: &Page) -> Outcome<State> {
        select(event)
    }
    #[state]
    fn recovery(event: &Page) -> Outcome<State> {
        select(event)
    }
    #[state]
    fn pairing(event: &Page) -> Outcome<State> {
        select(event)
    }
    #[state]
    fn firmware(event: &Page) -> Outcome<State> {
        select(event)
    }
}
pub struct Navigation(StateMachine<Storage>);
impl Default for Navigation {
    fn default() -> Self {
        Self(Storage.state_machine())
    }
}
impl Navigation {
    pub fn page(&self) -> Page {
        match self.0.state() {
            State::Home {} => Page::Home,
            State::Backups {} => Page::Backups,
            State::Recovery {} => Page::Recovery,
            State::Pairing {} => Page::Pairing,
            State::Firmware {} => Page::Firmware,
        }
    }
    pub fn navigate(&mut self, page: Page) {
        self.0.handle(&page);
    }
}

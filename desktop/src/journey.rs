//! Guided journeys reuse Session's identification, archive and return checks.
//! Firmware transfer remains unavailable until a reviewed installer is connected.
use crate::{
    device::Snapshot,
    runtime_recovery::Role,
    session::{Session, View},
};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Guiding,
    ReadyToSave,
    Returning,
    Paused,
    Failed,
    Complete,
}

mod machine {
    use statig::prelude::*;
    #[derive(Default)]
    pub struct Backup;
    pub enum Event {
        Evidence {
            error: Option<String>,
            saved: bool,
            returned: bool,
            can_save: bool,
        },
        Pause,
        Resume,
        Retry,
    }
    fn reconcile(event: &Event) -> Outcome<State> {
        match event {
            Event::Evidence {
                error: Some(error), ..
            } => Transition(State::failed(error.clone())),
            Event::Evidence {
                saved: true,
                returned: true,
                ..
            } => Transition(State::complete()),
            Event::Evidence { saved: true, .. } => Transition(State::returning()),
            Event::Evidence { can_save: true, .. } => Transition(State::ready_to_save()),
            Event::Evidence { .. } => Transition(State::guiding()),
            _ => Super,
        }
    }
    #[state_machine(initial = "State::guiding()", state(derive(Debug)))]
    impl Backup {
        #[state(superstate = "active")]
        fn guiding(event: &Event) -> Outcome<State> {
            reconcile(event)
        }
        #[state(superstate = "active")]
        fn ready_to_save(event: &Event) -> Outcome<State> {
            reconcile(event)
        }
        #[state(superstate = "active")]
        fn returning(event: &Event) -> Outcome<State> {
            reconcile(event)
        }
        #[superstate]
        fn active(event: &Event) -> Outcome<State> {
            match event {
                Event::Pause => Transition(State::paused()),
                _ => Super,
            }
        }
        #[state]
        fn paused(event: &Event) -> Outcome<State> {
            match event {
                Event::Resume => Transition(State::guiding()),
                _ => Super,
            }
        }
        #[state]
        fn failed(error: &String, event: &Event) -> Outcome<State> {
            let _ = error;
            match event {
                Event::Retry => Transition(State::guiding()),
                Event::Pause => Transition(State::paused()),
                _ => Super,
            }
        }
        #[state]
        fn complete(event: &Event) -> Outcome<State> {
            let _ = event;
            Super
        }
    }
}

pub struct Journey {
    session: Session,
    role: Role,
    machine: statig::blocking::StateMachine<machine::Backup>,
    archives: Vec<PathBuf>,
}

impl Journey {
    fn new_machine() -> statig::blocking::StateMachine<machine::Backup> {
        use statig::prelude::IntoStateMachineExt;
        machine::Backup.state_machine()
    }

    pub fn backup() -> Self {
        Self::backup_part(Role::Left)
    }
    pub fn backup_part(role: Role) -> Self {
        let mut session = Session::new();
        session.select_recovery_role(role);
        Self {
            session,
            role,
            machine: Self::new_machine(),
            archives: vec![],
        }
    }
    #[cfg(test)]
    pub(crate) fn from_saved_test_session(session: Session) -> Self {
        let role = session
            .selected_component()
            .expect("test selected component");
        let archives = vec![session.view().backup_path.clone().unwrap()];
        let mut journey = Self {
            session,
            role,
            machine: Self::new_machine(),
            archives,
        };
        journey.advance();
        journey
    }
    pub fn component(&self) -> Role {
        self.role
    }
    /// Compose the shared recovery worker with this journey. An asynchronous result
    /// may be adopted only while this component is still waiting for recovery.
    /// Saving then performs fresh discovery before and after reading the archive.
    pub fn accept_recovery(&mut self, session: Session) -> bool {
        if self.state() != State::Guiding
            || session.selected_component() != Some(self.role)
            || !session.view().can_save
            || session.view().backup_path.is_some()
            || self.session.view().backup_path.is_some()
        {
            return false;
        }
        self.session = session;
        // A local recovery endpoint proves only this selected component, not an unseen partner.
        self.advance();
        true
    }
    pub fn role(&self) -> Role {
        self.role
    }
    pub fn state(&self) -> State {
        match self.machine.state() {
            machine::State::Guiding {} => State::Guiding,
            machine::State::ReadyToSave {} => State::ReadyToSave,
            machine::State::Returning {} => State::Returning,
            machine::State::Paused {} => State::Paused,
            machine::State::Failed { .. } => State::Failed,
            machine::State::Complete {} => State::Complete,
        }
    }
    pub fn is_complete(&self) -> bool {
        self.state() == State::Complete
    }
    pub fn archives(&self) -> &[PathBuf] {
        &self.archives
    }
    pub fn view(&self) -> View {
        let mut view = self.session.view();
        if matches!(
            self.state(),
            State::Paused | State::Failed | State::Complete
        ) {
            view.can_save = false;
            view.needs_power_on_ack = false;
        }
        if let machine::State::Failed { error } = self.machine.state() {
            view.error = Some(error.clone());
        }
        view
    }
    pub fn observe(&mut self, observation: Result<Snapshot, String>) {
        if matches!(
            self.state(),
            State::Paused | State::Failed | State::Complete
        ) {
            return;
        }
        self.session.observe(observation);
        self.advance();
    }
    pub(crate) fn observe_with_mode(
        &mut self,
        observation: Result<Snapshot, String>,
        mode: Option<crate::device_status::Mode>,
    ) {
        if matches!(
            self.state(),
            State::Paused | State::Failed | State::Complete
        ) {
            return;
        }
        self.session.observe_with_mode(observation, mode);
        self.advance();
    }
    pub(crate) fn can_next_return(&self) -> bool {
        self.session.can_next_return()
    }
    pub(crate) fn next_return(&mut self) -> bool {
        let changed = self.session.next_return();
        if changed {
            self.advance();
        }
        changed
    }
    fn advance(&mut self) {
        let view = self.session.view();
        self.machine.handle(&machine::Event::Evidence {
            error: view.error,
            saved: view.backup_path.is_some(),
            returned: view.return_complete,
            can_save: view.can_save,
        });
    }
    pub fn save_backup(&mut self) -> Result<PathBuf, String> {
        if self.state() != State::ReadyToSave {
            return Err("The keyboard isn’t ready to save yet.".into());
        }
        match self.session.save_backup() {
            Ok(path) => {
                self.archives.push(path.clone());
                self.advance();
                Ok(path)
            }
            Err(error) => {
                self.machine.handle(&machine::Event::Evidence {
                    error: Some(error.clone()),
                    saved: false,
                    returned: false,
                    can_save: false,
                });
                Err(error)
            }
        }
    }
    /// Cancel pauses guidance. Saved archives are retained; no keyboard state is undone.
    pub fn pause(&mut self) {
        self.machine.handle(&machine::Event::Pause);
    }
    pub fn resume(&mut self) {
        if self.state() == State::Paused {
            self.restart_step();
            self.machine.handle(&machine::Event::Resume);
        }
    }
    pub fn retry(&mut self) {
        if self.state() == State::Failed {
            self.restart_step();
            self.machine.handle(&machine::Event::Retry);
        }
    }
    fn restart_step(&mut self) {
        // Fresh identification or a freshly observed disconnect must replace timed evidence.
        self.session.retry();
    }
    pub fn confirm_power_on(&mut self) {
        if !matches!(
            self.state(),
            State::Paused | State::Failed | State::Complete
        ) {
            self.session.confirm_power_on();
            self.advance();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::Device;
    fn normal() -> Snapshot {
        Snapshot {
            devices: vec![Device {
                location: 7,
                vendor: 0x2886,
                product: 0x8029,
                name: "NocFree & ANSI".into(),
            }],
            mounts: vec![],
        }
    }
    #[test]
    fn every_backup_is_one_explicitly_selected_part() {
        let mut snapshot = normal();
        snapshot.devices.push(Device {
            location: 8,
            vendor: 0x4c4b,
            product: 0x4671,
            name: "NocFree RMK Right".into(),
        });
        for role in [Role::Left, Role::Right, Role::Receiver] {
            let mut journey = Journey::backup_part(role);
            journey.observe(Ok(snapshot.clone()));
            assert_eq!(journey.component(), role);
            assert!(journey.archives().is_empty());
        }
    }
    #[test]
    fn adopted_backup_must_match_the_explicit_component() {
        let mounted = || Snapshot {
            devices: vec![Device {
                location: 7,
                vendor: 0x239a,
                product: 0x29,
                name: "NocFree &".into(),
            }],
            mounts: vec![crate::device::BootMount {
                path: "/fixture".into(),
                info: "UF2 Bootloader 0.9.2-39-g0147d71\nModel: NocFree &\nBoard-ID: NocFree &"
                    .into(),
            }],
        };
        for selected in [Role::Left, Role::Right, Role::Receiver] {
            for recovered in [Role::Left, Role::Right, Role::Receiver] {
                let mut journey = Journey::backup_part(selected);
                let mut session = Session::new();
                session.select_recovery_role(recovered);
                assert!(session.adopt_archive_drive(mounted()).unwrap());
                assert_eq!(journey.accept_recovery(session), selected == recovered);
                assert_eq!(journey.component(), selected);
                assert_eq!(
                    journey.state(),
                    if selected == recovered {
                        State::ReadyToSave
                    } else {
                        State::Guiding
                    }
                );
            }
        }
    }
    #[test]
    fn pause_ignores_observations_and_resume_requires_fresh_identification() {
        let mut journey = Journey::backup();
        journey.observe(Ok(normal()));
        assert_eq!(journey.view().title, "Hold Fn + 5");
        journey.pause();
        journey.observe(Ok(normal()));
        assert_eq!(journey.state(), State::Paused);
        assert!(journey.save_backup().is_err());
        journey.resume();
        assert_eq!(journey.view().title, "Connect the left half");
        journey.observe(Ok(normal()));
        assert_eq!(journey.view().title, "Hold Fn + 5");
    }
    #[test]
    fn error_stops_automatic_work_until_explicit_retry() {
        let mut journey = Journey::backup();
        journey.observe(Err("USB discovery failed".into()));
        assert_eq!(journey.state(), State::Failed);
        journey.observe(Ok(normal()));
        assert_eq!(journey.state(), State::Failed);
        journey.retry();
        assert_eq!(journey.state(), State::Guiding);
        assert_eq!(journey.view().error, None);
        assert_eq!(journey.role(), Role::Left);
    }
    #[test]
    fn invalid_control_events_cannot_restart_an_active_or_failed_journey() {
        let mut journey = Journey::backup_part(Role::Right);
        journey.resume();
        journey.retry();
        assert_eq!(journey.state(), State::Guiding);
        journey.observe(Err("discovery denied".into()));
        journey.resume();
        assert_eq!(journey.state(), State::Failed);
        journey.pause();
        journey.retry();
        assert_eq!(journey.state(), State::Paused);
        journey.observe(Ok(normal()));
        assert_eq!(journey.state(), State::Paused);
        journey.resume();
        assert_eq!(journey.state(), State::Guiding);
        assert_eq!(journey.component(), Role::Right);
    }
    #[test]
    fn a_copy_alone_does_not_complete_or_advance_the_half() {
        let mut journey = Journey::backup();
        journey.observe(Ok(normal()));
        journey.advance();
        assert_eq!(journey.role(), Role::Left);
        assert!(!journey.is_complete());
        assert!(journey.archives().is_empty());
        assert!(journey.save_backup().is_err());
    }
}

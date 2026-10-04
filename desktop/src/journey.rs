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

pub struct Journey {
    session: Session,
    role: Role,
    state: State,
    archives: Vec<PathBuf>,
    error: Option<String>,
}

impl Journey {
    pub fn backup() -> Self {
        Self::backup_part(Role::Left)
    }
    pub fn backup_part(role: Role) -> Self {
        let mut session = Session::new();
        session.select_recovery_role(role);
        Self {
            session,
            role,
            state: State::Guiding,
            archives: vec![],
            error: None,
        }
    }
    #[cfg(test)]
    pub(crate) fn from_saved_test_session(session: Session) -> Self {
        let role = session
            .selected_component()
            .expect("test selected component");
        let archives = vec![session.view().backup_path.clone().unwrap()];
        Self {
            session,
            role,
            state: State::Returning,
            archives,
            error: None,
        }
    }
    pub fn component(&self) -> Role {
        self.role
    }
    /// Compose the shared recovery worker with this journey. An asynchronous result
    /// may be adopted only while this component is still waiting for recovery.
    /// Saving then performs fresh discovery before and after reading the archive.
    pub fn accept_recovery(&mut self, session: Session) -> bool {
        if self.state != State::Guiding
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
        self.state
    }
    pub fn is_complete(&self) -> bool {
        self.state == State::Complete
    }
    pub fn archives(&self) -> &[PathBuf] {
        &self.archives
    }
    pub fn view(&self) -> View {
        let mut view = self.session.view();
        if matches!(self.state, State::Paused | State::Failed | State::Complete) {
            view.can_save = false;
            view.needs_power_on_ack = false;
        }
        if let Some(error) = &self.error {
            view.error = Some(error.clone());
        }
        view
    }
    pub fn observe(&mut self, observation: Result<Snapshot, String>) {
        if matches!(self.state, State::Paused | State::Failed | State::Complete) {
            return;
        }
        self.session.observe(observation);
        self.advance();
    }
    fn advance(&mut self) {
        let view = self.session.view();
        if let Some(error) = view.error {
            self.error = Some(error);
            self.state = State::Failed;
        } else if view.backup_path.is_some() && view.return_complete {
            self.state = State::Complete;
        } else if view.backup_path.is_some() {
            self.state = State::Returning;
        } else if view.can_save {
            self.state = State::ReadyToSave;
        } else {
            self.state = State::Guiding;
        }
    }
    pub fn save_backup(&mut self) -> Result<PathBuf, String> {
        if self.state != State::ReadyToSave {
            return Err("The keyboard isn’t ready to save yet.".into());
        }
        match self.session.save_backup() {
            Ok(path) => {
                self.archives.push(path.clone());
                self.advance();
                Ok(path)
            }
            Err(error) => {
                self.error = Some(error.clone());
                self.state = State::Failed;
                Err(error)
            }
        }
    }
    /// Cancel pauses guidance. Saved archives are retained; no keyboard state is undone.
    pub fn pause(&mut self) {
        if !self.is_complete() {
            self.state = State::Paused;
        }
    }
    pub fn resume(&mut self) {
        if self.state == State::Paused {
            self.restart_step();
        }
    }
    pub fn retry(&mut self) {
        if self.state == State::Failed {
            self.restart_step();
        }
    }
    fn restart_step(&mut self) {
        // Fresh identification or a freshly observed disconnect must replace timed evidence.
        self.session.retry();
        self.error = None;
        self.state = State::Guiding;
    }
    pub fn confirm_power_on(&mut self) {
        if !matches!(self.state, State::Paused | State::Failed | State::Complete) {
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

//! Guided journeys reuse Session's identification, archive and return checks.
//! Firmware transfer remains unavailable until a reviewed installer is connected.
use crate::{
    device::{Role, Snapshot},
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
    include_right: bool,
    plan_known: bool,
}

impl Journey {
    pub fn backup() -> Self {
        let mut session = Session::new();
        session.select(Role::Left);
        Self {
            session,
            role: Role::Left,
            state: State::Guiding,
            archives: vec![],
            error: None,
            include_right: false,
            plan_known: false,
        }
    }
    #[cfg(test)]
    pub(crate) fn from_saved_test_session(session: Session, include_right: bool) -> Self {
        let archives = vec![session.view().backup_path.clone().unwrap()];
        Self {
            session,
            role: Role::Left,
            state: State::Returning,
            archives,
            error: None,
            include_right,
            plan_known: true,
        }
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
        let supported_right = observation.as_ref().is_ok_and(|snapshot| {
            snapshot
                .devices
                .iter()
                .filter(|d| d.role() == Some(Role::Right))
                .count()
                == 1
        });
        self.session.observe(observation);
        if !self.plan_known && self.role == Role::Left && self.session.identified_normal() {
            self.include_right = supported_right;
            self.plan_known = true;
        }
        self.advance();
    }
    fn advance(&mut self) {
        let view = self.session.view();
        if let Some(error) = view.error {
            self.error = Some(error);
            self.state = State::Failed;
        } else if view.backup_path.is_some() && view.return_complete {
            if self.role == Role::Left && self.include_right {
                self.role = Role::Right;
                self.session.select(Role::Right);
                self.state = State::Guiding;
            } else {
                self.state = State::Complete;
            }
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
    fn only_supported_right_observed_at_initial_identification_joins_plan() {
        let mut with_right = normal();
        with_right.devices.push(Device {
            location: 8,
            vendor: 0x4c4b,
            product: 0x4651,
            name: "NocFree Input Probe Right Mac".into(),
        });
        let mut both = Journey::backup();
        both.observe(Ok(with_right.clone()));
        assert!(both.include_right);
        let mut left_only = Journey::backup();
        left_only.observe(Ok(normal()));
        left_only.observe(Ok(with_right.clone()));
        assert!(!left_only.include_right);
        with_right.devices[1].product = 0x4643;
        with_right.devices[1].name = "NocFree RMK Right".into();
        let mut unknown = Journey::backup();
        unknown.observe(Ok(with_right));
        assert!(!unknown.include_right);
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

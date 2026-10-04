//! Read-only presentation of verified journey facts. These values never advance a device flow.
use crate::{
    journey::{Journey, State as BackupState},
    recovery_journey::State as RecoveryState,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowProgress {
    pub labels: [&'static str; 3],
    /// Three means every step is complete. Display only; never attach skip actions.
    pub current: usize,
}

pub fn recovery(state: &RecoveryState) -> FlowProgress {
    let current = match state {
        RecoveryState::Choose => 0,
        RecoveryState::Identify(_) | RecoveryState::Failed(_, _) => 1,
        RecoveryState::Guiding(_, _) => 2,
        RecoveryState::Ready(_) => 3,
    };
    FlowProgress {
        labels: ["Choose", "Connect", "Recovery"],
        current,
    }
}

pub fn backup(journey: &Journey) -> FlowProgress {
    backup_view(journey.state(), journey.view().backup_path.is_some())
}

pub fn saving_backup() -> FlowProgress {
    FlowProgress {
        labels: ["Prepare", "Save copy", "Return"],
        current: 1,
    }
}

fn backup_view(state: BackupState, copy_saved: bool) -> FlowProgress {
    let current = match state {
        BackupState::Guiding => 0,
        BackupState::ReadyToSave => 1,
        BackupState::Returning => 2,
        BackupState::Complete => 3,
        BackupState::Paused | BackupState::Failed if copy_saved => 2,
        BackupState::Paused | BackupState::Failed => 0,
    };
    FlowProgress {
        labels: ["Prepare", "Save copy", "Return"],
        current,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        recovery_journey::{Procedure, RecoveryJourney},
        runtime_recovery::Role,
    };
    #[test]
    fn recovery_progress_waits_for_actual_drive_result_and_cancel_clears_it() {
        let mut journey = RecoveryJourney::new();
        assert_eq!(recovery(&journey.state()).current, 0);
        let attempt = journey.start(Role::Left).unwrap();
        assert_eq!(recovery(&journey.state()).current, 1);
        journey.observe(attempt, Role::Left, Procedure::FactoryLeft);
        let waiting = recovery(&journey.state());
        assert_eq!(waiting.current, 2);
        journey.complete(attempt, Role::Left, Ok(()));
        assert_eq!(recovery(&journey.state()).current, 3);
        journey.cancel();
        assert_eq!(recovery(&journey.state()).current, 0);
        assert_eq!(recovery(&journey.state()).current, 0);
    }
    #[test]
    fn saved_copy_and_return_wait_cannot_show_completed_steps() {
        assert_eq!(backup_view(BackupState::Returning, true).current, 2);
        assert_eq!(backup_view(BackupState::Paused, true).current, 2);
        assert_eq!(backup_view(BackupState::Failed, true).current, 2);
        assert_eq!(backup_view(BackupState::Complete, true).current, 3);
        assert_eq!(backup(&Journey::backup()).current, 0);
        assert_eq!(saving_backup().current, 1);
    }
}

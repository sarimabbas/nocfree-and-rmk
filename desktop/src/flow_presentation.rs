//! Read-only presentation of verified journey facts. These values never advance a device flow.
use crate::{
    journey::{Journey, State as BackupState},
    recovery_journey::State as RecoveryState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentProgress {
    pub completed: usize,
    pub total: usize,
}
impl ComponentProgress {
    pub fn fraction(self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        self.completed.min(self.total) as f32 / self.total as f32
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowProgress {
    pub labels: [&'static str; 3],
    /// Three means every step is complete. Display only; never attach skip actions.
    pub current: usize,
    pub attention: bool,
    pub components: Option<ComponentProgress>,
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
        attention: matches!(state, RecoveryState::Failed(_, _)),
        // Selecting a role or identifying its USB interface isn't completed recovery.
        components: (!matches!(state, RecoveryState::Choose)).then_some(ComponentProgress {
            completed: usize::from(matches!(state, RecoveryState::Ready(_))),
            total: 1,
        }),
    }
}

pub fn backup(journey: &Journey) -> FlowProgress {
    let components = journey
        .component_progress()
        .map(|(completed, total)| ComponentProgress { completed, total });
    backup_view(
        journey.state(),
        journey.view().backup_path.is_some(),
        components,
    )
}

fn backup_view(
    state: BackupState,
    copy_saved: bool,
    components: Option<ComponentProgress>,
) -> FlowProgress {
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
        attention: matches!(state, BackupState::Paused | BackupState::Failed),
        components,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        experimental_recovery::Role,
        recovery_journey::{Procedure, RecoveryJourney},
    };
    #[test]
    fn recovery_progress_waits_for_actual_drive_result_and_cancel_clears_it() {
        let mut journey = RecoveryJourney::new();
        assert!(recovery(journey.state()).components.is_none());
        let attempt = journey.start(Role::Left).unwrap();
        assert_eq!(recovery(journey.state()).current, 1);
        journey.observe(attempt, Role::Left, Procedure::FactoryLeft);
        let waiting = recovery(journey.state());
        assert_eq!(waiting.current, 2);
        assert_eq!(waiting.components.unwrap().fraction(), 0.0);
        journey.complete(attempt, Role::Left, Ok(()));
        assert_eq!(
            recovery(journey.state()).components.unwrap().fraction(),
            1.0
        );
        assert_eq!(recovery(journey.state()).current, 3);
        journey.cancel();
        assert_eq!(recovery(journey.state()).current, 0);
        assert!(recovery(journey.state()).components.is_none());
    }
    #[test]
    fn saved_copy_and_return_wait_cannot_show_complete_component_progress() {
        let waiting = backup_view(
            BackupState::Returning,
            true,
            Some(ComponentProgress {
                completed: 0,
                total: 1,
            }),
        );
        assert_eq!(waiting.current, 2);
        assert_eq!(waiting.components.unwrap().fraction(), 0.0);
        let paused = backup_view(BackupState::Paused, true, waiting.components);
        assert!(paused.attention);
        assert_eq!(paused.current, 2);
        assert_eq!(paused.components.unwrap().fraction(), 0.0);
        let completed = backup_view(
            BackupState::Complete,
            true,
            Some(ComponentProgress {
                completed: 1,
                total: 1,
            }),
        );
        assert_eq!(completed.current, 3);
        assert_eq!(completed.components.unwrap().fraction(), 1.0);
    }
    #[test]
    fn unidentified_backup_never_invents_a_plan_or_percentage() {
        let flow = backup(&Journey::backup());
        assert_eq!(flow.current, 0);
        assert!(flow.components.is_none());
    }
}

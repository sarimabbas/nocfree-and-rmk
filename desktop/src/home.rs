//! Read-only home state; USB names do not establish installed release versions.
use crate::device::Snapshot;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirmwareVersion(pub String);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UpdateAssessment {
    #[default]
    Unknown,
    Current,
    Available,
}

impl UpdateAssessment {
    /// Call only with actual installed and available release metadata.
    pub fn compare(
        installed: Option<&FirmwareVersion>,
        available: Option<&FirmwareVersion>,
        approved_newer_release: bool,
    ) -> Self {
        match (installed, available) {
            (Some(installed), Some(available)) if installed == available => Self::Current,
            (Some(_), Some(_)) if approved_newer_release => Self::Available,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Home {
    #[default]
    Connect,
    Recovery,
    Factory,
    Rmk(UpdateAssessment),
}

impl Home {
    pub fn observe(result: Result<Snapshot, String>, update: UpdateAssessment) -> Self {
        let Ok(snapshot) = result else {
            return Self::Connect;
        };
        let bootloaders = snapshot.devices.iter().filter(|d| d.bootloader()).count();
        let mut lefts = snapshot
            .devices
            .iter()
            .filter(|d| d.factory_left() || d.rmk_left());
        let left = lefts.next();
        if lefts.next().is_some() {
            return Self::Connect;
        }
        if bootloaders == 1 && left.is_none() && snapshot.mounts.len() <= 1 {
            return Self::Recovery;
        }
        if bootloaders != 0 || !snapshot.mounts.is_empty() {
            return Self::Connect;
        }
        let Some(device) = left else {
            return Self::Connect;
        };
        if device.factory_left() {
            Self::Factory
        } else if device.rmk_left() {
            Self::Rmk(update)
        } else {
            Self::Connect
        }
    }

    pub fn connection_label(self) -> Option<&'static str> {
        match self {
            Self::Factory | Self::Rmk(_) => Some("Connected by USB"),
            Self::Recovery => Some("Recovery mode"),
            Self::Connect => None,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Connect => "Connect your keyboard",
            Self::Recovery => "Recovery mode",
            Self::Factory => "Make it your own",
            Self::Rmk(UpdateAssessment::Current) => "You’re up to date",
            Self::Rmk(UpdateAssessment::Available) => "An update is ready",
            Self::Rmk(UpdateAssessment::Unknown) => "Running RMK",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Connect => "Connect the left half by USB to get started.",
            Self::Recovery => "Return to normal mode so we can recognize your keyboard.",
            Self::Factory => "Switch to RMK, with your factory firmware saved first.",
            Self::Rmk(UpdateAssessment::Available) => {
                "Your current firmware will be saved before updating."
            }
            Self::Rmk(_) => "Your keyboard is ready to use.",
        }
    }

    pub fn action(self) -> Option<&'static str> {
        match self {
            Self::Factory => Some("Install RMK"),
            Self::Rmk(UpdateAssessment::Available) => Some("Update RMK"),
            _ => None,
        }
    }

    pub fn can_restore(self) -> bool {
        matches!(self, Self::Rmk(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::Device;

    fn snapshot(vendor: u64, product: u64, name: &str) -> Snapshot {
        Snapshot {
            devices: vec![Device {
                location: 1,
                vendor,
                product,
                name: name.into(),
            }],
            mounts: vec![],
        }
    }
    #[test]
    fn identifies_only_exact_single_normal_keyboard() {
        assert_eq!(
            Home::observe(
                Ok(snapshot(0x2886, 0x8029, "NocFree & ANSI")),
                UpdateAssessment::Unknown
            ),
            Home::Factory
        );
        assert_eq!(
            Home::observe(
                Ok(snapshot(0x4c4b, 0x4643, "NocFree RMK")),
                UpdateAssessment::Unknown
            ),
            Home::Rmk(UpdateAssessment::Unknown)
        );
        assert_eq!(
            Home::observe(
                Ok(snapshot(0x4c4b, 0x4643, "NocFree AND RMK Receiver")),
                UpdateAssessment::Unknown
            ),
            Home::Connect
        );
        assert_eq!(
            Home::observe(Ok(Snapshot::default()), UpdateAssessment::Unknown),
            Home::Connect
        );
        assert_eq!(
            Home::observe(Err("discovery failed".into()), UpdateAssessment::Unknown),
            Home::Connect
        );
    }
    #[test]
    fn recovery_does_not_infer_a_role_and_ambiguity_clears_state() {
        let mut s = snapshot(0x239a, 0x0029, "anything");
        assert_eq!(
            Home::observe(Ok(s.clone()), UpdateAssessment::Unknown),
            Home::Recovery
        );
        s.devices.push(s.devices[0].clone());
        assert_eq!(
            Home::observe(Ok(s), UpdateAssessment::Unknown),
            Home::Connect
        );
        let mut s = snapshot(0x4c4b, 0x4643, "NocFree RMK");
        s.devices.push(s.devices[0].clone());
        assert_eq!(
            Home::observe(Ok(s), UpdateAssessment::Unknown),
            Home::Connect
        );
    }
    #[test]
    fn independent_receiver_does_not_hide_the_left() {
        let mut s = snapshot(0x4c4b, 0x4643, "NocFree RMK");
        s.devices.push(Device {
            location: 2,
            vendor: 0x4c4b,
            product: 0x4643,
            name: "NocFree AND RMK Receiver".into(),
        });
        assert_eq!(
            Home::observe(Ok(s), UpdateAssessment::Unknown),
            Home::Rmk(UpdateAssessment::Unknown)
        );
        let mut s = snapshot(0x239a, 0x0029, "NocFree &");
        s.devices.push(Device {
            location: 2,
            vendor: 0x4c4b,
            product: 0x4643,
            name: "NocFree AND RMK Receiver".into(),
        });
        assert_eq!(
            Home::observe(Ok(s), UpdateAssessment::Unknown),
            Home::Recovery
        );
    }

    #[test]
    fn actual_versions_control_update_claims() {
        let installed = FirmwareVersion("1.0.0".into());
        let same = FirmwareVersion("1.0.0".into());
        let newer = FirmwareVersion("1.1.0".into());
        assert_eq!(
            UpdateAssessment::compare(Some(&installed), None, false),
            UpdateAssessment::Unknown
        );
        assert_eq!(
            UpdateAssessment::compare(None, Some(&newer), false),
            UpdateAssessment::Unknown
        );
        let current = UpdateAssessment::compare(Some(&installed), Some(&same), false);
        assert_eq!(current, UpdateAssessment::Current);
        assert_eq!(Home::Rmk(current).action(), None);
        assert_eq!(
            UpdateAssessment::compare(Some(&newer), Some(&installed), false),
            UpdateAssessment::Unknown
        );
        let available = UpdateAssessment::compare(Some(&installed), Some(&newer), true);
        assert_eq!(available, UpdateAssessment::Available);
        assert_eq!(Home::Rmk(available).action(), Some("Update RMK"));
        assert_ne!(
            Home::Rmk(UpdateAssessment::Unknown).title(),
            Home::Rmk(current).title()
        );
    }
}

//! Application versions observed on USB, never inferred from a release filename.
use crate::runtime_recovery::Role;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub location: u64,
    pub factory: bool,
    pub version: String,
    pub role: Role,
}

fn stamp(manufacturer: Option<&str>) -> Option<String> {
    let version = manufacturer?.strip_prefix("NocFree RMK;fw=")?;
    // Require canonical SemVer: no arbitrary USB string becomes a release label.
    let parsed = semver::Version::parse(version).ok()?;
    (parsed.to_string() == version).then(|| version.to_owned())
}

pub fn read() -> Vec<Observation> {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return Vec::new();
    };
    runtime.block_on(async {
        let Ok(Ok(devices)) =
            tokio::time::timeout(std::time::Duration::from_secs(3), nusb::list_devices()).await
        else {
            return Vec::new();
        };
        let mut observations = Vec::new();
        for device in devices {
            #[cfg(target_os = "macos")]
            let location = u64::from(device.location_id());
            #[cfg(not(target_os = "macos"))]
            let location = 0;
            let role = [Role::Left, Role::Right, Role::Receiver]
                .into_iter()
                .find(|role| {
                    device.vendor_id() == 0x4c4b
                        && device.product_id() == role.product()
                        && device.product_string() == Some(role.name())
                });
            if let Some(role) = role {
                if let Some(version) = stamp(device.manufacturer_string()) {
                    observations.push((
                        device.id(),
                        Observation {
                            location,
                            factory: false,
                            version,
                            role,
                        },
                    ));
                }
            } else if crate::factory_version::matches(&device)
                && let Ok(version) = crate::factory_version::read(&device).await
            {
                observations.push((
                    device.id(),
                    Observation {
                        location,
                        factory: true,
                        version,
                        role: Role::Left,
                    },
                ));
            }
        }
        // A reconnect may reuse its physical port. Bind versions to the same
        // enumerated device instance as well as the UI's location snapshot.
        let Ok(Ok(current)) =
            tokio::time::timeout(std::time::Duration::from_secs(3), nusb::list_devices()).await
        else {
            return Vec::new();
        };
        let current: Vec<_> = current.map(|device| device.id()).collect();
        observations
            .into_iter()
            .filter_map(|(id, observation)| current.contains(&id).then_some(observation))
            .collect()
    })
}

pub(crate) fn connected_label(
    devices: &crate::device_status::UsbKey,
    observations: &[Observation],
) -> String {
    if crate::device_status::factory_left(devices) {
        label(true, observations)
    } else if !crate::device_status::battery_source(devices).is_empty() {
        label(false, observations)
    } else if devices.iter().any(|(_, vendor, product, name)| {
        crate::device::factory_dongle_identity(*vendor, *product, name)
    }) {
        label(true, observations)
    } else {
        "Firmware not detected".into()
    }
}

pub fn label(factory: bool, observations: &[Observation]) -> String {
    let kind = if factory { "factory" } else { "RMK" };
    let version = [Role::Left, Role::Receiver, Role::Right]
        .into_iter()
        .find_map(|role| {
            let mut matching = observations
                .iter()
                .filter(|o| o.factory == factory && o.role == role);
            let first = matching.next()?;
            matching.next().is_none().then_some(first.version.as_str())
        })
        .unwrap_or("version unknown");
    format!("Running {kind} firmware · {version}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn factory_dongle_only_reports_firmware_family_without_inventing_a_version() {
        let devices = vec![(1, 0x2886, 0x8029, "NocFree_Dongle".into())];
        assert_eq!(
            connected_label(&devices, &[]),
            "Running factory firmware · version unknown"
        );
        assert_eq!(connected_label(&vec![], &[]), "Firmware not detected");
        let unknown = vec![(1, 0x2886, 0x8029, "Other dongle".into())];
        assert_eq!(connected_label(&unknown, &[]), "Firmware not detected");
    }
    #[test]
    fn only_project_semver_stamp_is_a_version() {
        assert_eq!(stamp(Some("NocFree RMK;fw=1.2.3")), Some("1.2.3".into()));
        assert_eq!(
            stamp(Some("NocFree RMK;fw=1.2.3-beta.1")),
            Some("1.2.3-beta.1".into())
        );
        for value in [
            "NocFree RMK community",
            "vial:123;rmk:0.9.0",
            "NocFree RMK;fw=01.2.3",
            "NocFree RMK;fw=1.2",
            "NocFree RMK;fw=1.2.3 garbage",
        ] {
            assert_eq!(stamp(Some(value)), None);
        }
        assert_eq!(stamp(None), None);
    }
    #[test]
    fn keyboard_label_prefers_observed_left_and_separates_factory() {
        let observations = vec![
            Observation {
                location: 1,
                factory: false,
                version: "1.1.0".into(),
                role: Role::Receiver,
            },
            Observation {
                location: 2,
                factory: false,
                version: "1.2.0".into(),
                role: Role::Left,
            },
            Observation {
                location: 3,
                factory: true,
                version: "2.0.0".into(),
                role: Role::Left,
            },
        ];
        assert_eq!(label(false, &observations), "Running RMK firmware · 1.2.0");
        assert_eq!(
            label(true, &observations),
            "Running factory firmware · 2.0.0"
        );
        assert_eq!(label(false, &[]), "Running RMK firmware · version unknown");
    }
}

//! Live peripheral status is independent of the firmware journey's eligibility.
use crate::{device::Device, recovery_journey::State, runtime_recovery::Role};
pub(crate) type UsbKey = Vec<(u64, u64, u64, String)>;

fn device(entry: &(u64, u64, u64, String)) -> Device {
    Device {
        location: entry.0,
        vendor: entry.1,
        product: entry.2,
        name: entry.3.clone(),
    }
}

pub(crate) fn factory_left(devices: &UsbKey) -> bool {
    devices
        .iter()
        .filter(|entry| device(entry).factory_left())
        .count()
        == 1
}

pub(crate) fn left_usb(devices: &UsbKey) -> bool {
    devices
        .iter()
        .filter(|entry| {
            let d = device(entry);
            d.factory_left() || d.rmk_left()
        })
        .count()
        == 1
}

/// Matches battery::read's preference: one normal left, otherwise one dongle.
/// Other peripherals entering recovery do not replace this producer.
pub(crate) fn battery_source(devices: &UsbKey) -> UsbKey {
    let left: UsbKey = devices
        .iter()
        .filter(|entry| device(entry).rmk_left())
        .cloned()
        .collect();
    if !left.is_empty() {
        return if left.len() == 1 { left } else { Vec::new() };
    }
    let dongles: UsbKey = devices
        .iter()
        .filter(|(_, v, p, name)| {
            (*v, *p, name.as_str()) == (0x4c4b, 0x4644, "NocFree RMK Receiver")
        })
        .cloned()
        .collect();
    if dongles.len() == 1 {
        dongles
    } else {
        Vec::new()
    }
}

pub(crate) fn right_usb(devices: &UsbKey) -> UsbKey {
    devices
        .iter()
        .filter(|entry| device(entry).role() == Some(crate::device::Role::Right))
        .cloned()
        .collect()
}

pub(crate) fn battery_available(devices: &UsbKey, recovery: &State) -> bool {
    let source = battery_source(devices);
    let Some((_, _, product, _)) = source.first() else {
        return false;
    };
    let recovering = match recovery {
        State::Identify(role) | State::Guiding(role, _) => Some(*role),
        _ => None,
    };
    !matches!(
        (product, recovering),
        (0x4643, Some(Role::Left)) | (0x4644, Some(Role::Receiver))
    )
}

/// One derived snapshot drives all status-bar visuals. No view mutates connection state.
pub(crate) struct Observation<'a> {
    pub devices: &'a UsbKey,
    pub recovery_locations: [Option<u64>; 3],
    pub levels: crate::battery::Levels,
    pub bluetooth_connected: bool,
    pub dongle_connected: bool,
    pub dongle_link_connected: bool,
    pub right_link_connected: bool,
}
pub(crate) struct Status {
    pub connection: crate::status_strip::Connection,
    pub left: crate::status_strip::Peripheral,
    pub right: crate::status_strip::Peripheral,
    pub dongle_recovery: bool,
}
impl Observation<'_> {
    pub(crate) fn derive(self) -> Status {
        use crate::status_strip::{Connection, Peripheral};
        let recovery = self.recovery_locations.map(|location| {
            location.is_some_and(|location| {
                self.devices
                    .iter()
                    .any(|(l, v, p, _)| *l == location && *v == 0x239a && *p == 0x0029)
            })
        });
        let left_usb = left_usb(self.devices) || recovery[0];
        let connection = if left_usb {
            Connection::Usb
        } else if self.bluetooth_connected {
            Connection::Bluetooth
        } else if self.dongle_connected && self.dongle_link_connected {
            Connection::Dongle
        } else {
            Connection::Disconnected
        };
        let left_connected = connection != Connection::Disconnected;
        let right_usb = !right_usb(self.devices).is_empty() || recovery[1];
        let levels = self.levels.visible(
            left_connected || left_usb,
            right_usb || left_connected && self.right_link_connected,
        );
        Status {
            connection,
            left: Peripheral {
                level: levels.left,
                usb_connected: left_usb,
                recovery: recovery[0],
            },
            right: Peripheral {
                level: levels.right,
                usb_connected: right_usb,
                recovery: recovery[1],
            },
            dongle_recovery: recovery[2],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        device::Snapshot,
        home::{Home, UpdateAssessment},
    };
    fn right_recovery() -> (Home, UsbKey) {
        let devices = vec![
            Device {
                location: 1,
                vendor: 0x4c4b,
                product: 0x4643,
                name: "NocFree RMK".into(),
            },
            Device {
                location: 2,
                vendor: 0x239a,
                product: 0x0029,
                name: "NocFree &".into(),
            },
        ];
        let key = devices
            .iter()
            .map(|d| (d.location, d.vendor, d.product, d.name.clone()))
            .collect();
        (
            Home::observe(
                Ok(Snapshot {
                    devices,
                    mounts: vec![],
                }),
                UpdateAssessment::Unknown,
            ),
            key,
        )
    }
    #[test]
    fn one_snapshot_derives_recovery_connection_and_cached_battery() {
        let (_, devices) = right_recovery();
        let observe = |devices, locations| {
            Observation {
                devices,
                recovery_locations: locations,
                levels: crate::battery::Levels {
                    left: Some(100),
                    right: Some(75),
                },
                bluetooth_connected: false,
                dongle_connected: false,
                dongle_link_connected: false,
                right_link_connected: false,
            }
            .derive()
        };
        let status = observe(&devices, [None, Some(2), None]);
        assert_eq!(status.connection, crate::status_strip::Connection::Usb);
        assert_eq!(status.left.level, Some(100));
        assert_eq!(status.right.level, Some(75));
        assert!(status.right.recovery);
        let empty = vec![];
        let disconnected = observe(&empty, [None, Some(2), None]);
        assert_eq!(
            disconnected.connection,
            crate::status_strip::Connection::Disconnected
        );
        assert!(disconnected.left.level.is_none());
        assert!(disconnected.right.level.is_none());
        assert!(!disconnected.right.recovery);
    }
    #[test]
    fn right_recovery_keeps_left_usb_status() {
        let (home, devices) = right_recovery();
        assert_eq!(home, Home::Connect); // Journey gates remain conservative.
        assert!(left_usb(&devices));
    }
    #[test]
    fn right_recovery_keeps_left_battery_polling() {
        let (_, devices) = right_recovery();
        assert!(battery_available(&devices, &State::Identify(Role::Right)));
        assert!(!battery_available(&devices, &State::Identify(Role::Left)));
    }

    #[test]
    fn only_battery_producer_changes_invalidate_its_connection() {
        let (_, recovery) = right_recovery();
        let mut normal = recovery.clone();
        normal[1] = (2, 0x4c4b, 0x4671, "NocFree RMK Right".into());
        assert_eq!(battery_source(&normal), battery_source(&recovery));
        assert_ne!(right_usb(&normal), right_usb(&recovery));
        assert!(battery_source(&recovery[1..].to_vec()).is_empty());
        let mut impostor = recovery;
        impostor[0].3 = "NocFree RMK Receiver".into();
        assert!(!left_usb(&impostor));
        assert!(battery_source(&impostor).is_empty());
    }
}

//! Live peripheral status is independent of the firmware journey's eligibility.
use crate::{device::Device, recovery_journey::State, runtime_recovery::Role};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Wired,
    Bluetooth,
    Dongle,
}
impl Mode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Wired => "Wired mode",
            Self::Bluetooth => "Bluetooth mode",
            Self::Dongle => "Dongle mode",
        }
    }
}
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

pub(crate) fn battery_available(devices: &UsbKey, recovery: &State, bluetooth: bool) -> bool {
    let source = battery_source(devices);
    let Some((_, _, product, _)) = source.first() else {
        return bluetooth && cfg!(target_os = "macos");
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

pub(crate) fn battery_via_bluetooth(devices: &UsbKey, bluetooth: bool) -> bool {
    cfg!(target_os = "macos") && battery_source(devices).is_empty() && bluetooth
}

/// One derived snapshot drives all status-bar visuals. No view mutates connection state.
pub(crate) struct Observation<'a> {
    pub devices: &'a UsbKey,
    pub recovery_locations: [Option<u64>; 3],
    pub levels: crate::battery::Levels,
    pub left_mode: Option<Mode>,
    pub bluetooth_connected: bool,
    pub dongle_connected: bool,
    pub right_link_connected: bool,
    pub right_link_known: bool,
    pub telemetry: Option<crate::battery::Telemetry>,
    pub links_fresh: bool,
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
        // Cable presence proves power, never the selected typing route.
        let telemetry = self.telemetry.filter(|_| self.links_fresh);
        let bt = telemetry
            .filter(|t| t.known & 4 != 0)
            .map(|t| t.has(3))
            .unwrap_or(
                self.bluetooth_connected
                    && (!self.links_fresh
                        || !matches!(self.left_mode, Some(Mode::Wired | Mode::Dongle))),
            );
        let dongle =
            self.dongle_connected && telemetry.is_some_and(|t| t.known & 4 != 0 && t.has(4));
        let wired = telemetry.is_some_and(|t| t.known & 4 != 0 && t.has(2));
        let routes = u8::from(bt) + u8::from(dongle) + u8::from(wired);
        let connection = if recovery[0] || routes > 1 {
            Connection::Unknown
        } else if wired {
            Connection::Usb
        } else if bt {
            Connection::Bluetooth
        } else if dongle {
            Connection::Dongle
        } else if telemetry.is_some_and(|t| t.known & 4 != 0) {
            Connection::Disconnected
        } else {
            Connection::Unknown
        };
        let left_connected = matches!(
            connection,
            Connection::Usb | Connection::Bluetooth | Connection::Dongle
        );
        let right_usb = !right_usb(self.devices).is_empty() || recovery[1];
        let levels = self.levels.visible(
            left_connected
                || left_usb
                || self.links_fresh && !battery_source(self.devices).is_empty()
                || telemetry.is_some_and(|t| t.known & 1 != 0 && t.has(0)),
            right_usb
                || telemetry.is_some_and(|t| t.known & 2 != 0 && t.has(1))
                || self.links_fresh && self.right_link_connected,
        );
        Status {
            connection,
            left: Peripheral {
                level: levels.left,
                mode: self.left_mode.filter(|_| self.links_fresh),
                usb_connected: left_usb || telemetry.is_some_and(|t| t.known & 1 != 0 && t.has(0)),
                link_connected: None,
                recovery: recovery[0],
            },
            right: Peripheral {
                level: levels.right,
                mode: None,
                usb_connected: right_usb || telemetry.is_some_and(|t| t.known & 2 != 0 && t.has(1)),
                link_connected: (self.links_fresh && self.right_link_known)
                    .then_some(self.right_link_connected),
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
    #[test]
    fn confirmed_dongle_route_does_not_invent_switch_position() {
        let devices = vec![];
        let status = Observation {
            devices: &devices,
            recovery_locations: [None; 3],
            levels: crate::battery::Levels {
                left: Some(100),
                right: Some(99),
            },
            left_mode: None,
            bluetooth_connected: false,
            dongle_connected: true,
            right_link_connected: true,
            right_link_known: true,
            telemetry: Some(crate::battery::Telemetry {
                flags: 16,
                known: 4,
            }),
            links_fresh: true,
        }
        .derive();
        assert_eq!(status.connection, crate::status_strip::Connection::Dongle);
        assert_eq!(status.left.mode, None);
        assert_eq!(status.left.level, Some(100));
    }
    #[test]
    fn live_route_flags_override_stale_os_link_without_turning_power_into_typing() {
        use crate::status_strip::Connection;
        let (_, devices) = right_recovery();
        let derive = |flags, known, fresh| {
            Observation {
                devices: &devices,
                recovery_locations: [None; 3],
                levels: crate::battery::Levels {
                    left: Some(0),
                    right: Some(75),
                },
                left_mode: Some(Mode::Dongle),
                bluetooth_connected: true,
                dongle_connected: true,
                right_link_connected: true,
                right_link_known: true,
                telemetry: Some(crate::battery::Telemetry { flags, known }),
                links_fresh: fresh,
            }
            .derive()
        };
        assert_eq!(
            derive(1 | 16, 1 | 4 | 8, true).connection,
            Connection::Dongle
        );
        assert_eq!(
            derive(1, 1 | 4 | 8, true).connection,
            Connection::Disconnected
        );
        assert_eq!(derive(1, 1, true).connection, Connection::Unknown);
        assert_eq!(
            derive(1 | 4 | 8, 1 | 4, true).connection,
            Connection::Unknown
        );
        let stale = derive(1 | 16, 1 | 4 | 8, false);
        assert_eq!(stale.connection, Connection::Bluetooth);
        assert_eq!(stale.left.mode, None);
        assert_eq!(stale.right.level, None);
        assert_eq!(stale.right.link_connected, None);
        assert_eq!(derive(1 | 16, 1 | 4 | 8, true).left.level, Some(0));
    }
    #[test]
    fn factory_usb_without_known_switch_is_power_only() {
        let devices = vec![(1, 0x2886, 0x8029, "NocFree & ANSI".into())];
        let observed = Observation {
            devices: &devices,
            recovery_locations: [None; 3],
            levels: crate::battery::Levels::default(),
            left_mode: None,
            bluetooth_connected: false,
            dongle_connected: false,
            right_link_connected: false,
            right_link_known: false,
            telemetry: None,
            links_fresh: false,
        }
        .derive();
        assert_eq!(
            observed.connection,
            crate::status_strip::Connection::Unknown
        );
        assert!(observed.left.usb_connected);
    }
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
                left_mode: None,
                levels: crate::battery::Levels {
                    left: Some(100),
                    right: Some(75),
                },
                bluetooth_connected: false,
                dongle_connected: false,
                right_link_connected: false,
                right_link_known: true,
                telemetry: None,
                links_fresh: true,
            }
            .derive()
        };
        let status = observe(&devices, [None, Some(2), None]);
        assert_eq!(status.connection, crate::status_strip::Connection::Unknown);
        assert_eq!(status.left.level, Some(100));
        assert_eq!(status.right.level, Some(75));
        assert!(status.right.recovery);
        let empty = vec![];
        let disconnected = observe(&empty, [None, Some(2), None]);
        assert_eq!(
            disconnected.connection,
            crate::status_strip::Connection::Unknown
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
        assert!(battery_available(
            &devices,
            &State::Identify(Role::Right),
            false
        ));
        assert!(!battery_available(
            &devices,
            &State::Identify(Role::Left),
            false
        ));
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

    #[test]
    fn dongle_battery_source_precedes_bluetooth() {
        let dongle = vec![(1, 0x4c4b, 0x4644, "NocFree RMK Receiver".into())];
        assert!(!battery_via_bluetooth(&dongle, true));
        assert!(battery_via_bluetooth(&vec![], true));
    }
}

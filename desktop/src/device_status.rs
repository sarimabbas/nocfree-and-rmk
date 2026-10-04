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

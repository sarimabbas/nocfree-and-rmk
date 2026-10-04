//! Pure setup assessment. Unseen parts never make an entire keyboard current.
use crate::{
    device::Device,
    device_status::UsbKey,
    firmware_version::Observation,
    navigation::{Page, Readiness},
    runtime_recovery::Role,
};

pub(crate) fn assess(
    page: Page,
    devices: &UsbKey,
    versions: &[Observation],
    latest: Option<&str>,
    fresh: bool,
) -> Readiness {
    if !fresh || devices.is_empty() {
        return Readiness::Unknown;
    }
    let devices: Vec<_> = devices
        .iter()
        .map(|(location, vendor, product, name)| Device {
            location: *location,
            vendor: *vendor,
            product: *product,
            name: name.clone(),
        })
        .collect();
    if devices.iter().any(Device::bootloader) {
        return Readiness::Unknown;
    }
    if page == Page::Restore {
        if devices.iter().filter(|d| d.factory_left()).count() == 2
            && devices.iter().filter(|d| d.factory_right()).count() == 1
            && !devices.iter().any(|d| {
                d.rmk_left()
                    || d.rmk_receiver()
                    || (d.vendor == 0x4c4b
                        && d.product == u64::from(Role::Right.product())
                        && d.name == Role::Right.name())
            })
        {
            return Readiness::AlreadyFactory;
        }
        return Readiness::Needed;
    }
    if page != Page::Firmware {
        return Readiness::Unknown;
    }
    let Some(latest) = latest else {
        return Readiness::Unknown;
    };
    for role in [Role::Left, Role::Right, Role::Receiver] {
        let matching: Vec<_> = devices
            .iter()
            .filter(|d| {
                d.vendor == 0x4c4b
                    && d.product == u64::from(role.product())
                    && d.name == role.name()
            })
            .collect();
        if matching.len() != 1 {
            return Readiness::Needed;
        }
        let matching_versions: Vec<_> = versions
            .iter()
            .filter(|v| !v.factory && v.role == role && v.location == matching[0].location)
            .collect();
        if matching_versions.len() != 1 || matching_versions[0].version != latest {
            return Readiness::Needed;
        }
    }
    Readiness::AlreadyLatest
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rmk() -> (UsbKey, Vec<Observation>) {
        let mut key = vec![];
        let mut versions = vec![];
        for (i, role) in [Role::Left, Role::Right, Role::Receiver]
            .into_iter()
            .enumerate()
        {
            key.push((
                i as u64,
                0x4c4b,
                u64::from(role.product()),
                role.name().into(),
            ));
            versions.push(Observation {
                location: i as u64,
                factory: false,
                role,
                version: "1.2.3".into(),
            });
        }
        (key, versions)
    }
    #[test]
    fn whole_release_requires_every_part_and_matching_live_versions() {
        let (mut key, mut versions) = rmk();
        assert_eq!(
            assess(Page::Firmware, &key, &versions, Some("1.2.3"), true),
            Readiness::AlreadyLatest
        );
        assert_eq!(
            assess(Page::Firmware, &key, &versions, Some("1.2.3"), false),
            Readiness::Unknown
        );
        versions[1].version = "1.2.2".into();
        assert_eq!(
            assess(Page::Firmware, &key, &versions, Some("1.2.3"), true),
            Readiness::Needed
        );
        versions[1].version = "1.2.3".into();
        versions[1].location = 100;
        assert_eq!(
            assess(Page::Firmware, &key, &versions, Some("1.2.3"), true),
            Readiness::Needed
        );
        key.pop();
        assert_eq!(
            assess(Page::Firmware, &key, &versions, Some("1.2.3"), true),
            Readiness::Needed
        );
    }
    #[test]
    fn factory_is_version_independent_but_mixed_or_missing_parts_are_not_complete() {
        let mut key = vec![
            (10, 0x2886, 0x8029, "NocFree & ANSI".into()),
            (11, 0x2886, 0x8029, "NocFree & ANSI".into()),
            (12, 0x239a, 0x80d8, "NocFree nRF52833 Right".into()),
        ];
        assert_eq!(
            assess(Page::Restore, &key, &[], None, true),
            Readiness::AlreadyFactory
        );
        key.pop();
        assert_eq!(
            assess(Page::Restore, &key, &[], None, true),
            Readiness::Needed
        );
        key.extend(rmk().0);
        assert_eq!(
            assess(Page::Restore, &key, &[], None, true),
            Readiness::Needed
        );
    }
}

//! Read-only macOS discovery. Connection identifiers never belong in UI text.
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum Role {
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub(crate) location: u64,
    pub(crate) vendor: u64,
    pub(crate) product: u64,
    pub(crate) name: String,
}
/// Factory USB names include the layout, but do not distinguish left from dongle.
/// Keep the observed VID/PID and product family together; layout is not role evidence.
pub(crate) fn factory_keyboard_identity(vendor: u64, product: u64, name: &str) -> bool {
    if (vendor, product) != (0x2886, 0x8029) {
        return false;
    }
    let layout = name
        .strip_prefix("NocFree & ")
        .or_else(|| name.strip_prefix("NocFree _ "));
    layout.is_some_and(|layout| {
        !layout.is_empty()
            && layout.len() <= 16
            && layout.bytes().all(|byte| byte.is_ascii_uppercase())
    })
}

impl Device {
    pub(crate) fn bootloader(&self) -> bool {
        self.vendor == 0x239a && self.product == 0x0029
    }
    pub(crate) fn factory_left(&self) -> bool {
        factory_keyboard_identity(self.vendor, self.product, &self.name)
    }
    pub(crate) fn rmk_left(&self) -> bool {
        (self.vendor, self.product, self.name.as_str()) == (0x4c4b, 0x4643, "NocFree RMK")
    }
    pub(crate) fn rmk_receiver(&self) -> bool {
        (self.vendor, self.product, self.name.as_str()) == (0x4c4b, 0x4644, "NocFree RMK Receiver")
    }
    /// Exact factory identity recorded before and after the original right restore.
    pub(crate) fn factory_right(&self) -> bool {
        (self.vendor, self.product, self.name.as_str())
            == (0x239a, 0x80d8, "NocFree nRF52833 Right")
    }
    pub(crate) fn role(&self) -> Option<Role> {
        if self.factory_right() {
            return Some(Role::Right);
        }
        match (self.vendor, self.product, self.name.as_str()) {
            (0x4c4b, 0x4643, "NocFree RMK") => Some(Role::Left),
            (0x4c4b, 0x4651, "NocFree Input Probe Right Mac") => Some(Role::Right),
            (0x4c4b, 0x4671, "NocFree RMK Right") => Some(Role::Right),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootMount {
    pub(crate) path: PathBuf,
    pub(crate) info: String,
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub(crate) devices: Vec<Device>,
    pub(crate) mounts: Vec<BootMount>,
}

/// Host-connected Bluetooth identity only; paired-but-disconnected entries do not count.
/// None means the host query failed, never proof that Bluetooth is disconnected.
pub(crate) fn bluetooth_links() -> Option<(bool, bool)> {
    let output = Command::new("/usr/sbin/system_profiler")
        .args(["SPBluetoothDataType", "-json", "-timeout", "3"])
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.len() > 1024 * 1024 {
        return None;
    }
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    report["SPBluetoothDataType"].as_array()?;
    if connected_count(&report, |name| name == "NocFree RMK") > 1
        || connected_count(&report, factory_bluetooth_name) > 1
    {
        return None;
    }
    Some((
        connected_bluetooth_report(&report),
        connected_factory_bluetooth_report(&report),
    ))
}
fn connected_count(report: &serde_json::Value, accepts: impl Fn(&str) -> bool) -> usize {
    report["SPBluetoothDataType"]
        .as_array()
        .map_or(0, |controllers| {
            controllers
                .iter()
                .filter_map(|controller| controller["device_connected"].as_array())
                .flatten()
                .filter_map(|device| device.as_object())
                .flat_map(|names| names.keys())
                .filter(|name| accepts(name))
                .count()
        })
}
fn factory_bluetooth_name(name: &str) -> bool {
    matches!(
        name,
        "NocFree & BLE1"
            | "NocFree & BLE2"
            | "NocFree & BLE3"
            | "NocFree _ BLE1"
            | "NocFree _ BLE2"
            | "NocFree _ BLE3"
    )
}
fn connected_bluetooth_report(report: &serde_json::Value) -> bool {
    connected_count(report, |name| name == "NocFree RMK") == 1
}
fn connected_factory_bluetooth_report(report: &serde_json::Value) -> bool {
    connected_count(report, factory_bluetooth_name) == 1
}

pub fn discover() -> Result<Snapshot, String> {
    if !cfg!(target_os = "macos") {
        return Err("Device discovery currently supports macOS only.".into());
    }
    let output = Command::new("/usr/sbin/ioreg")
        .args(["-p", "IOUSB", "-l", "-w0", "-a"])
        .output()
        .map_err(|_| "Could not run macOS USB discovery.".to_string())?;
    if !output.status.success() {
        return Err("macOS USB discovery failed.".into());
    }
    let tree = plist::Value::from_reader(std::io::Cursor::new(output.stdout))
        .map_err(|_| "Could not read macOS USB discovery results.".to_string())?;
    let mut devices = Vec::new();
    collect_devices(&tree, &mut devices);
    devices.sort_by_key(|d| (d.location, d.vendor, d.product));
    devices.dedup();
    let mut mounts = Vec::new();
    let volumes = fs::read_dir("/Volumes").map_err(|error| {
        if permission_denied(&error) {
            recovery_read_error(&error)
        } else {
            "Could not inspect mounted volumes.".into()
        }
    })?;
    for entry in volumes {
        let path = entry
            .map_err(|_| "Mounted volumes changed during discovery.".to_string())?
            .path();
        let info_path = path.join("INFO_UF2.TXT");
        if !metadata_readable(
            fs::metadata(&info_path).map(|_| ()),
            devices.iter().any(Device::bootloader),
        )? {
            continue;
        }
        let info = String::from_utf8(read_bounded(&info_path, 8192)?).map_err(|_| {
            "Could not read bootloader metadata; reconnect the selected component.".to_string()
        })?;
        if info.lines().any(|l| l.trim() == "Model: NocFree &") {
            mounts.push(BootMount { path, info });
        }
    }
    Ok(Snapshot { devices, mounts })
}

pub(crate) fn read_bounded(path: &Path, maximum: usize) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|error| recovery_read_error(&error))?;
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| recovery_read_error(&error))?;
    if bytes.len() > maximum {
        return Err("Recovery file exceeds the expected readback size.".into());
    }
    Ok(bytes)
}

fn metadata_readable(result: std::io::Result<()>, recovery_present: bool) -> Result<bool, String> {
    match result {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) if permission_denied(&error) => {
            if recovery_present {
                Err(recovery_read_error(&error))
            } else {
                Ok(false)
            }
        }
        Err(_) => Err("Could not inspect the recovery drive. Reconnect it, then try again.".into()),
    }
}

fn permission_denied(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::PermissionDenied
        || matches!(error.raw_os_error(), Some(1 | 13))
}
fn recovery_read_error(error: &std::io::Error) -> String {
    if permission_denied(error) {
        "Allow NocFree RMK Companion to access Removable Volumes in System Settings → Privacy & Security → Files and Folders, then try again.".into()
    } else {
        "Recovery files could not be read. Reconnect the drive, then try again.".into()
    }
}

fn collect_devices(value: &plist::Value, result: &mut Vec<Device>) {
    match value {
        plist::Value::Dictionary(dict) => {
            let number = |key: &str| dict.get(key).and_then(plist::Value::as_unsigned_integer);
            if let (Some(location), Some(vendor), Some(product)) = (
                number("locationID"),
                number("idVendor"),
                number("idProduct"),
            ) && matches!(vendor, 0x2886 | 0x4c4b | 0x239a)
            {
                result.push(Device {
                    location,
                    vendor,
                    product,
                    name: dict
                        .get("USB Product Name")
                        .and_then(plist::Value::as_string)
                        .unwrap_or("")
                        .to_owned(),
                });
            }
            for child in dict.values() {
                collect_devices(child, result);
            }
        }
        plist::Value::Array(values) => {
            for child in values {
                collect_devices(child, result);
            }
        }
        _ => {}
    }
}

/// Container validation for archiving a readback, NOT firmware installation policy.
/// This intentionally does not grant restore eligibility or recognize a half.
pub(crate) fn inspect_archive(data: &[u8]) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    const COUNT: usize = (0x6d000 - 0x1000) / 256;
    if data.len() != COUNT * 512 {
        return Err("Readback coverage is incomplete; nothing was saved.".into());
    }
    let mut indexes = vec![false; COUNT];
    let mut addresses = vec![false; COUNT];
    for block in data.as_chunks::<512>().0.iter() {
        let word =
            |offset: usize| u32::from_le_bytes(block[offset..offset + 4].try_into().unwrap());
        let address = word(12) as usize;
        let index = word(20) as usize;
        if word(0) != 0x0a324655
            || word(4) != 0x9e5d5157
            || word(508) != 0x0ab16f30
            || word(8) != 0x2000
            || word(16) != 256
            || word(24) != COUNT as u32
            || word(28) != 0x239a0029
            || index >= COUNT
            || indexes[index]
            || !(0x1000..0x6d000).contains(&address)
            || !address.is_multiple_of(256)
        {
            return Err("Readback has an unexpected UF2 container; nothing was saved.".into());
        }
        let slot = (address - 0x1000) / 256;
        if addresses[slot] {
            return Err("Readback repeats a flash address; nothing was saved.".into());
        }
        indexes[index] = true;
        addresses[slot] = true;
    }
    Ok(format!("{:x}", Sha256::digest(data)))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[test]
    fn missing_metadata_is_skipped_but_denied_recovery_metadata_fails() {
        assert!(
            !metadata_readable(
                Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
                true
            )
            .unwrap()
        );
        let denied =
            metadata_readable(Err(std::io::Error::from_raw_os_error(1)), true).unwrap_err();
        assert!(denied.contains("Removable Volumes"));
        // An unrelated protected volume cannot break ordinary keyboard discovery.
        assert!(!metadata_readable(Err(std::io::Error::from_raw_os_error(1)), false).unwrap());
        assert!(metadata_readable(Ok(()), true).unwrap());
    }
    #[test]
    fn permission_denial_is_actionable_and_not_a_disconnection() {
        for error in [
            std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            std::io::Error::from_raw_os_error(1),
            std::io::Error::from_raw_os_error(13),
        ] {
            assert!(recovery_read_error(&error).contains("Removable Volumes"));
            assert!(recovery_read_error(&error).contains("Files and Folders"));
        }
        assert!(
            !recovery_read_error(&std::io::Error::from(std::io::ErrorKind::NotFound))
                .contains("Removable Volumes")
        );
    }
    pub(crate) fn archive() -> Vec<u8> {
        let count = (0x6d000 - 0x1000) / 256;
        let mut data = vec![0; count * 512];
        for (index, block) in data.as_chunks_mut::<512>().0.iter_mut().enumerate() {
            for (offset, value) in [
                (0, 0x0a324655),
                (4, 0x9e5d5157),
                (8, 0x2000),
                (12, 0x1000 + index as u32 * 256),
                (16, 256),
                (20, index as u32),
                (24, count as u32),
                (28, 0x239a0029),
                (508, 0x0ab16f30),
            ] {
                block[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        data
    }
    #[test]
    fn readback_rejects_duplicate_addresses_and_missing_bytes() {
        let valid = archive();
        assert!(inspect_archive(&valid).is_ok());
        assert!(inspect_archive(&valid[..valid.len() - 1]).is_err());
        let mut duplicate = valid;
        duplicate[512 + 12..512 + 16].copy_from_slice(&0x1000u32.to_le_bytes());
        assert!(inspect_archive(&duplicate).is_err());
    }
    #[test]
    fn metadata_alone_does_not_identify_role() {
        assert_eq!(
            Device {
                location: 1,
                vendor: 0x239a,
                product: 0x29,
                name: "NocFree &".into()
            }
            .role(),
            None
        );
    }
    #[test]
    fn right_management_identity_cannot_identify_left_or_legacy_stage() {
        let right = Device {
            location: 7,
            vendor: 0x4c4b,
            product: 0x4671,
            name: "NocFree RMK Right".into(),
        };
        assert_eq!(right.role(), Some(Role::Right));
        assert!(!right.rmk_left());
        for (product, name) in [
            (0x4661, "NocFree RMK Right"),
            (0x4671, "NocFree RMK"),
            (0x4644, "NocFree RMK Receiver"),
        ] {
            assert_eq!(
                Device {
                    product,
                    name: name.into(),
                    ..right.clone()
                }
                .role(),
                None
            );
        }
    }
    #[test]
    fn factory_layout_is_not_a_role_and_unrelated_devices_are_excluded() {
        for prefix in ["NocFree & ", "NocFree _ "] {
            for layout in ["ANSI", "ISO", "JP", "KR", "JIS"] {
                let device = Device {
                    location: 1,
                    vendor: 0x2886,
                    product: 0x8029,
                    name: format!("{prefix}{layout}"),
                };
                assert!(device.factory_left());
                assert_eq!(device.role(), None);
            }
        }
        for (vendor, product, name) in [
            (0x2886, 0x8029, "NocFree & "),
            (0x2886, 0x8029, "NocFree & ANSI unrelated"),
            (0x2886, 0x8029, "NocFree & BLE1"),
            (0x2886, 0x8029, "Unrelated ISO"),
            (0x2886, 0x8030, "NocFree & ISO"),
            (0x239a, 0x8029, "NocFree & ISO"),
        ] {
            assert!(!factory_keyboard_identity(vendor, product, name));
        }
    }

    #[test]
    fn full_rmk_left_identity_excludes_receiver_and_near_matches() {
        let left = Device {
            location: 7,
            vendor: 0x4c4b,
            product: 0x4643,
            name: "NocFree RMK".into(),
        };
        assert_eq!(left.role(), Some(Role::Left));
        assert!(left.rmk_left());
        assert!(!left.factory_left());
        for (vendor, product, name) in [
            (0x4c4b, 0x4643, "NocFree AND RMK Receiver"),
            (0x4c4b, 0x4643, "NocFree RMK Left"),
            (0x4c4b, 0x4651, "NocFree RMK"),
            (0x2886, 0x4643, "NocFree RMK"),
        ] {
            let other = Device {
                vendor,
                product,
                name: name.into(),
                ..left.clone()
            };
            assert_eq!(other.role(), None);
            assert!(!other.rmk_left());
            assert!(!other.factory_left());
        }
    }
}

#[cfg(test)]
mod bluetooth_tests {
    #[test]
    fn pairing_alone_does_not_establish_a_connection() {
        use serde_json::json;
        assert!(super::connected_bluetooth_report(
            &json!({"SPBluetoothDataType":[{"device_connected":[{"NocFree RMK":{}}]}]})
        ));
        assert!(!super::connected_bluetooth_report(
            &json!({"SPBluetoothDataType":[{"device_not_connected":[{"NocFree RMK":{}}]}]})
        ));
        assert!(!super::connected_bluetooth_report(
            &json!({"SPBluetoothDataType":[{"device_connected":[{"Other":{}}]}]})
        ));
        assert!(!super::connected_bluetooth_report(
            &json!({"SPBluetoothDataType":[{"device_connected":[{"NocFree RMK":{}},{"NocFree RMK":{}}]}]})
        ));
    }
}

#[cfg(test)]
mod factory_bluetooth_tests {
    #[test]
    fn duplicate_connected_profiles_across_controllers_are_ambiguous() {
        let report = serde_json::json!({"SPBluetoothDataType":[{"device_connected":[{"NocFree & BLE1":{}}]},{"device_connected":[{"NocFree & BLE2":{}}]}]});
        assert_eq!(
            super::connected_count(&report, super::factory_bluetooth_name),
            2
        );
        assert!(!super::connected_factory_bluetooth_report(&report));
        let report = serde_json::json!({"SPBluetoothDataType":[{"device_connected":[{"NocFree RMK":{}}]},{"device_connected":[{"NocFree RMK":{}}]}]});
        assert!(!super::connected_bluetooth_report(&report));
    }
    #[test]
    fn stock_profiles_are_distinct_from_rmk_and_paired_only_entries() {
        let report = serde_json::json!({"SPBluetoothDataType":[{"device_connected":[{"NocFree & BLE2":{}}],"device_not_connected":[{"NocFree RMK":{}}]}]});
        assert!(super::connected_factory_bluetooth_report(&report));
        assert!(!super::connected_bluetooth_report(&report));
        let report = serde_json::json!({"SPBluetoothDataType":[{"device_not_connected":[{"NocFree & BLE2":{}}]}]});
        assert!(!super::connected_factory_bluetooth_report(&report));
    }
}

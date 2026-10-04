//! Read-only archives keep normal-device correlation separate from unverified mounted-drive adoption.
use crate::device::{self, BootMount, Role as KeyboardRole, Snapshot};
use crate::runtime_recovery::Role;
impl From<KeyboardRole> for Role {
    fn from(role: KeyboardRole) -> Self {
        match role {
            KeyboardRole::Left => Self::Left,
            KeyboardRole::Right => Self::Right,
        }
    }
}
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub struct View {
    pub title: String,
    pub instruction: String,
    pub can_save: bool,
    pub return_complete: bool,
    pub needs_power_on_ack: bool,
    pub error: Option<String>,
    pub backup_path: Option<PathBuf>,
}
use crate::return_flow::{Observation as ReturnObservation, Phase as ReturnPhase, ReturnFlow};
#[derive(Default)]
pub struct Session {
    role: Option<Role>,
    archive_only: bool,
    recovery_role: Option<crate::runtime_recovery::Role>,
    location: Option<u64>,
    ready: Option<BootMount>,
    normal_present: bool,
    rmk_left: bool,
    rmk_receiver: bool,
    legacy_left_start: bool,
    factory_right: bool,
    connection_present: bool,
    problem: Option<String>,
    status: String,
    backup_path: Option<PathBuf>,
    return_flow: ReturnFlow,
    archived_location: Option<u64>,
}
impl Session {
    pub fn new() -> Self {
        Self::default()
    }
    /// Adopt the USB location of a freshly validated, role-specific local USB endpoint.
    /// The caller must arm that endpoint first; product strings alone are not evidence.
    pub fn bind_recovery(&mut self, location: u64) {
        self.location = Some(location);
        self.rmk_left = self.role == Some(Role::Left);
        self.rmk_receiver = self.role == Some(Role::Receiver);
        self.factory_right = false;
    }
    pub(crate) fn selected_component(&self) -> Option<Role> {
        self.role
    }
    pub fn select(&mut self, role: impl Into<Role>) {
        *self = Self {
            role: Some(role.into()),
            status: "Waiting for normal-mode identification.".into(),
            ..Self::default()
        };
    }
    pub fn select_recovery_role(&mut self, role: Role) {
        self.select(role);
        self.recovery_role = Some(role);
    }
    pub(crate) fn recovery_binding(
        &self,
    ) -> Result<(crate::runtime_recovery::Role, u64, BootMount), String> {
        if self.archive_only {
            return Err(
                "This read-only archive does not establish firmware installation eligibility."
                    .into(),
            );
        }
        if self.problem.is_some() {
            return Err("Recovery connection is unavailable.".into());
        }
        Ok((
            self.recovery_role
                .ok_or("Recovery role was not explicitly selected.")?,
            self.location.ok_or("Recovery connection is missing.")?,
            self.ready.clone().ok_or("Recovery drive is missing.")?,
        ))
    }
    /// A deliberately selected part may archive a unique, validated drive already
    /// mounted at launch. Its role stays unverified and cannot authorize installation.
    pub(crate) fn adopt_archive_drive(&mut self, snapshot: Snapshot) -> Result<bool, String> {
        if self.role.is_none() || self.backup_path.is_some() || self.location.is_some() {
            return Ok(false);
        }
        let bootloaders: Vec<_> = snapshot.devices.iter().filter(|d| d.bootloader()).collect();
        if bootloaders.is_empty() && snapshot.mounts.is_empty() {
            return Ok(false);
        }
        if bootloaders.len() != 1 || snapshot.mounts.len() > 1 {
            return Err("Connect only the part you want to back up in recovery mode.".into());
        }
        if snapshot.mounts.is_empty() {
            return Ok(false);
        }
        validate_metadata(&snapshot.mounts[0].info)
            .map_err(|_| "This recovery drive is unfamiliar. Nothing was saved.".to_owned())?;
        self.location = Some(bootloaders[0].location);
        self.archive_only = true;
        self.recovery_role = None;
        self.observe(Ok(snapshot));
        if let Some(error) = self.view().error {
            return Err(error);
        }
        Ok(self.view().can_save)
    }
    /// Retry the physical return guide without losing a completed host archive.
    /// A retained port is correlation for this live guide, not fresh role evidence.
    pub fn retry(&mut self) {
        if self.backup_path.is_some() {
            self.location = self.archived_location;
            self.ready = None;
            self.normal_present = false;
            self.connection_present = false;
            self.problem = None;
            self.return_flow.restart();
            self.status = "Checking the saved component before restarting its return steps.".into();
        } else if let Some(role) = self.role {
            self.select(role);
        }
    }
    pub fn observe(&mut self, observation: Result<Snapshot, String>) {
        self.observe_at(observation, Instant::now());
    }
    fn observe_at(&mut self, observation: Result<Snapshot, String>, now: Instant) {
        let fresh = observation.is_ok();
        self.observe_snapshot(observation);
        if fresh && self.problem.is_none() {
            self.advance_return(now);
        } else if self.return_flow.phase().is_some() {
            // Never let time spent without a trustworthy observation satisfy a wait.
            self.return_flow.restart();
        }
    }
    fn observe_snapshot(&mut self, observation: Result<Snapshot, String>) {
        self.ready = None;
        self.normal_present = false;
        self.connection_present = false;
        let snapshot = match observation {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.location = None;
                self.problem = Some(error.clone());
                self.status = error;
                return;
            }
        };
        let Some(role) = self.role else {
            self.status = "Choose the component you connected.".into();
            return;
        };
        let bootloaders: Vec<_> = snapshot.devices.iter().filter(|d| d.bootloader()).collect();
        if bootloaders.len() > 1 || snapshot.mounts.len() > 1 {
            self.location = None;
            self.problem =
                Some("Leave only the half we are checking connected, then try again.".into());
            self.status = "Multiple bootloaders are connected. Disconnect them, then reconnect only the selected component in normal mode.".into();
            return;
        }
        if self.location.is_none() {
            let normal: Vec<_> = snapshot
                .devices
                .iter()
                .filter(|d| normal_matches(d, role))
                .collect();
            if normal.len() == 1 && bootloaders.is_empty() && snapshot.mounts.is_empty() {
                self.location = Some(normal[0].location);
                self.rmk_left = normal[0].rmk_left();
                self.rmk_receiver = normal[0].rmk_receiver();
                self.factory_right = normal[0].factory_right();
                self.normal_present = true;
                self.connection_present = true;
                self.problem = None;
                self.status =
                    "Selected component observed in normal mode. Keep the same USB connection."
                        .into();
            } else {
                if role == Role::Right
                    && snapshot
                        .devices
                        .iter()
                        .any(|d| d.vendor == 0x239a && d.product == 0x80d8)
                {
                    self.problem = Some("This right half’s USB identity is unfamiliar. Reconnect the selected component.".into());
                }
                self.status = "Waiting for the selected component in normal mode; a bootloader drive alone cannot identify a half.".into();
            }
            return;
        }
        let location = self.location.unwrap();
        self.connection_present = snapshot.devices.iter().any(|d| d.location == location);
        if let Some(bootloader) = bootloaders.first() {
            if bootloader.location != location {
                self.location = None;
                self.connection_present = false;
                self.problem = Some("The keyboard returned on a different USB connection. Reconnect it where it was, then try again.".into());
                self.status = "Bootloader appeared on a different connection. Reconnect the selected component in normal mode.".into();
                return;
            }
            let Some(mount) = snapshot.mounts.first() else {
                self.status =
                    "Bootloader detected. Waiting for its recovery drive to mount.".into();
                return;
            };
            if let Err(error) = validate_metadata(&mount.info) {
                self.problem = Some(
                    "This keyboard has an unfamiliar recovery system. Saving is unavailable."
                        .into(),
                );
                self.status = error;
                return;
            }
            self.problem = None;
            self.ready = Some(mount.clone());
            self.status =
                "One correlated recovery drive observed. Ready to save a private readback.".into();
        } else if let Some(normal) = snapshot
            .devices
            .iter()
            .find(|d| d.location == location && normal_matches(d, role))
        {
            self.rmk_left = normal.rmk_left();
            self.rmk_receiver = normal.rmk_receiver();
            self.factory_right = normal.factory_right();
            self.normal_present = true;
            self.problem = None;
            self.status =
                "Component is in normal mode. Follow the recovery procedure shown above.".into();
        } else if snapshot.devices.iter().any(|d| d.location == location) {
            self.location = None;
            self.connection_present = false;
            self.problem = Some(
                "A different device is connected. Reconnect the keyboard, then try again.".into(),
            );
            self.status = "An unexpected device appeared on this connection. Identify the selected component again.".into();
        } else {
            self.status =
                "USB disappearance observed. Waiting for the same connection to return.".into();
        }
    }
    fn advance_return(&mut self, now: Instant) {
        let fresh_return = (self.role == Some(Role::Left) && !self.legacy_left_start
            || self.role == Some(Role::Receiver))
            && self.normal_present
            && self.backup_path.is_some()
            && self.location.is_some()
            && self.location == self.archived_location;
        if matches!(self.return_flow.phase(), Some(ReturnPhase::Reconnect))
            && self.connection_present
            && !self.normal_present
            && !fresh_return
        {
            self.status =
                "It stayed in recovery. Unplug the cable and try the return steps again.".into();
        }
        self.return_flow.observe(ReturnObservation {
            now,
            connected: self.connection_present,
            normal: self.normal_present,
            fresh_return,
            needs_power_on: self.role == Some(Role::Right) || self.legacy_left_start,
        });
    }
    pub fn confirm_power_on(&mut self) {
        self.confirm_power_on_at(Instant::now());
    }
    fn confirm_power_on_at(&mut self, now: Instant) {
        if matches!(self.return_flow.phase(), Some(ReturnPhase::PowerOn))
            && !self.connection_present
            && self.problem.is_none()
        {
            self.return_flow.confirm(now);
        }
    }
    fn return_instruction(&self, now: Instant) -> Option<(String, String)> {
        let side = if self.role == Some(Role::Right) {
            "right"
        } else {
            "left"
        };
        let remaining = |since: Instant, seconds: u64| {
            Duration::from_secs(seconds)
                .saturating_sub(now.saturating_duration_since(since))
                .as_millis()
                .div_ceil(1000)
        };
        self.return_flow.phase().map(|phase| match phase {
            ReturnPhase::Disconnect if self.role == Some(Role::Receiver) => (
                "Unplug the USB dongle".into(),
                "Unplug it from your Mac.".into(),
            ),
            ReturnPhase::Disconnect if self.role == Some(Role::Right) => (
                "Turn the right half OFF".into(),
                if self.status.starts_with("It stayed in recovery") {
                    "It stayed in recovery. Then unplug its USB cable.".into()
                } else {
                    "Then unplug its USB cable.".into()
                },
            ),
            ReturnPhase::Disconnect if self.rmk_left => (
                "Unplug the left half".into(),
                "Set its switch to WIRED, then unplug its USB cable.".into(),
            ),
            ReturnPhase::Disconnect => (
                "Unplug the left half".into(),
                if self.status.starts_with("It stayed in recovery") {
                    "It stayed in recovery. Leave its switch in WIRED.".into()
                } else {
                    "Leave its switch in WIRED.".into()
                },
            ),
            ReturnPhase::OffWait { since } => (
                "Keep it unplugged".into(),
                format!("Wait {} more seconds.", remaining(since, 5)),
            ),
            ReturnPhase::PowerOn if self.rmk_left => (
                "Switch the left half to Bluetooth".into(),
                "Leave USB unplugged. Confirm once its switch is in Bluetooth.".into(),
            ),
            ReturnPhase::PowerOn => (
                "Switch the right half ON".into(),
                "Leave USB unplugged. Confirm once its switch is ON.".into(),
            ),
            ReturnPhase::StartWait { since } => (
                "Let it start".into(),
                format!(
                    "Keep USB unplugged for {} more seconds.",
                    remaining(since, 10)
                ),
            ),
            ReturnPhase::Reconnect if self.role == Some(Role::Receiver) => (
                "Reconnect the USB dongle".into(),
                "Plug it back into your Mac.".into(),
            ),
            ReturnPhase::Reconnect => (
                format!("Reconnect the {side} half"),
                "Plug its USB cable back into your Mac.".into(),
            ),
            ReturnPhase::Complete => (
                "Firmware copy saved".into(),
                "The keyboard is back in normal mode.".into(),
            ),
        })
    }
    /// Factory identity for the selected role; shared left/receiver identity still relies on physical selection.
    pub fn factory_role(&self) -> Option<KeyboardRole> {
        if !self.identified_normal() {
            return None;
        }
        match self.role {
            Some(Role::Left) if !self.rmk_left => Some(KeyboardRole::Left),
            Some(Role::Right) if self.factory_right => Some(KeyboardRole::Right),
            Some(Role::Receiver) if !self.rmk_receiver => Some(KeyboardRole::Left),
            _ => None,
        }
    }
    pub(crate) fn identified_normal(&self) -> bool {
        self.location.is_some() && self.normal_present && self.problem.is_none()
    }
    pub fn view(&self) -> View {
        let identified = self.location.is_some();
        let waiting_drive = identified && self.connection_present && !self.normal_present;
        let instruction = match self.role {
            None => "Connect your keyboard with a USB cable.".into(),
            Some(Role::Left) if !identified => "Connect the left half by USB. Leave the dongle disconnected.".into(),
            Some(Role::Right) if !identified => "Plug in the right half.".into(),
            Some(Role::Receiver) if !identified => "Plug in the USB dongle. Leave the keyboard halves disconnected from USB.".into(),
            _ if waiting_drive => "Keep the cable connected.".into(),
            _ if !self.connection_present => "Reconnect using the same USB port.".into(),
            Some(Role::Left) if self.rmk_left => "Use the recovery procedure for your installed firmware, keeping the same USB port.".into(),
            Some(Role::Left) => "Leave USB connected and the switch in WIRED. Hold Fn + 5 for five seconds, then release.".into(),
            Some(Role::Right) if self.factory_right => "Leave USB connected. Hold Fn + 0 for five seconds, then release.".into(),
            Some(Role::Right) => "Leave USB connected. Hold Fn, tap the main-row 0 key, then release Fn.".into(),
            Some(Role::Receiver)=>"Keep the dongle connected. Companion will guide you into recovery.".into(),
        };
        let return_instruction = self.return_instruction(Instant::now());
        View {
            title: if let Some((title, _)) = &return_instruction {
                title.clone()
            } else if waiting_drive {
                "Preparing your firmware copy".into()
            } else if identified && !self.connection_present {
                "Waiting for your keyboard".into()
            } else if identified {
                match self.role {
                    Some(Role::Left) if self.rmk_left => "Open recovery on the left half".into(),
                    Some(Role::Left) => "Hold Fn + 5".into(),
                    Some(Role::Right) if self.factory_right => "Hold Fn + 0".into(),
                    Some(Role::Right) => "Hold Fn and tap 0".into(),
                    Some(Role::Receiver) => "Open recovery on the dongle".into(),
                    None => "Connect your keyboard".into(),
                }
            } else {
                match self.role {
                    None => "Connect your keyboard".into(),
                    Some(Role::Left) => "Connect the left half".into(),
                    Some(Role::Right) => "Connect the right half".into(),
                    Some(Role::Receiver) => "Connect the USB dongle".into(),
                }
            },
            instruction: return_instruction
                .map(|(_, instruction)| instruction)
                .unwrap_or(instruction),
            return_complete: matches!(self.return_flow.phase(), Some(ReturnPhase::Complete))
                && self.normal_present
                && self.problem.is_none(),
            needs_power_on_ack: matches!(self.return_flow.phase(), Some(ReturnPhase::PowerOn))
                && !self.connection_present
                && self.problem.is_none(),
            error: self.problem.clone(),
            can_save: self.ready.is_some() && self.backup_path.is_none(),
            backup_path: self.backup_path.clone(),
        }
    }
    pub fn save_backup(&mut self) -> Result<PathBuf, String> {
        let home = std::env::var_os("HOME")
            .ok_or("Could not locate your private application support folder.")?;
        let root = PathBuf::from(home).join("Library/Application Support/NocFree Companion");
        self.save_with(&root, device::discover, |path| {
            device::read_bounded(path, 1728 * 512)
        })
    }
    fn save_with(
        &mut self,
        root: &Path,
        mut discover: impl FnMut() -> Result<Snapshot, String>,
        mut read: impl FnMut(&Path) -> Result<Vec<u8>, String>,
    ) -> Result<PathBuf, String> {
        let result: Result<PathBuf, String> = (|| {
            if !self.view().can_save {
                return Err(
                    "Identify the selected component and its recovery drive before saving.".into(),
                );
            }
            let location = self.location;
            let original = self.ready.clone().unwrap();
            self.observe(Ok(discover()?));
            if self.location != location || self.ready.as_ref() != Some(&original) {
                return Err(
                    "Connection changed before backup. Identify the component again.".into(),
                );
            }
            let data = read(&original.path.join("CURRENT.UF2"))?;
            let hash = device::inspect_archive(&data)?;
            // A complete read is not sufficient: the same correlated device and mount must still be present.
            self.observe(Ok(discover()?));
            if self.location != location || self.ready.as_ref() != Some(&original) {
                return Err(
                    "Recovery drive changed during backup. No complete backup was saved.".into(),
                );
            }
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| "Could not timestamp the local backup.")?
                .as_nanos();
            private_directory(root)?;
            let folder = root.join(format!("readback-{timestamp}"));
            create_private_directory(&folder)?;
            atomic_file(&folder, "CURRENT.UF2", &data)?;
            atomic_file(&folder, "INFO_UF2.TXT", original.info.as_bytes())?;
            let journal = serde_json::json!({"schema":1,"planned_role":self.role.map(|role|match role {Role::Left=>"Left",Role::Right=>"Right",Role::Receiver=>"Receiver"}),"role_verified":false,"saved_at_unix_ns":timestamp.to_string(),"readback_sha256":hash,"coverage_start":4096,"coverage_end_exclusive":446464,"blocks":1728,"status":"readback archived; restore eligibility unproven","restart":"fresh normal-mode identification required"});
            atomic_file(
                &folder,
                "session.json",
                &serde_json::to_vec_pretty(&journal)
                    .map_err(|_| "Could not encode the local journal.")?,
            )?;
            self.backup_path = Some(folder.clone());
            self.legacy_left_start = self.rmk_left
                && data.as_chunks::<512>().0.iter().any(|block| {
                    u32::from_le_bytes(block[12..16].try_into().unwrap()) == 0x1200
                        && u32::from_le_bytes(block[32..36].try_into().unwrap()) == 0x87eeb07c
                });
            self.archived_location = location;
            self.return_flow.restart();
            self.status = "Private readback saved and hashed. No firmware was written.".into();
            Ok(folder)
        })();
        if let Err(error) = &result {
            self.ready = None;
            self.location = None;
            self.normal_present = false;
            self.connection_present = false;
            self.problem = Some(error.clone());
            self.status = error.clone();
        }
        result
    }
}
pub(crate) fn validate_metadata(info: &str) -> Result<(), String> {
    let lines: Vec<_> = info.lines().map(str::trim).collect();
    if !lines.contains(&"UF2 Bootloader 0.9.2-39-g0147d71")
        || !lines.contains(&"Model: NocFree &")
        || !lines.contains(&"Board-ID: NocFree &")
    {
        return Err("This bootloader version/model has not been inspected for this prototype. Saving is unavailable.".into());
    }
    Ok(())
}
fn private_directory(path: &Path) -> Result<(), String> {
    if path.exists() {
        if fs::symlink_metadata(path)
            .map_err(|_| "Could not inspect backup folder.")?
            .file_type()
            .is_symlink()
        {
            return Err("Backup folder must not be a symbolic link.".into());
        }
    } else {
        fs::create_dir_all(path).map_err(|_| "Could not create your private backup folder.")?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| "Could not restrict backup folder permissions.")?;
    }
    Ok(())
}
fn create_private_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .mode(0o700)
            .create(path)
            .map_err(|_| "Could not create a unique private backup folder.".to_string())
    }
    #[cfg(not(unix))]
    {
        fs::create_dir(path).map_err(|_| "Could not create a unique backup folder.".to_string())
    }
}
fn atomic_file(folder: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    let temporary = folder.join(format!(".{name}.partial"));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|_| "Could not create a private backup file.")?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(
            |_| "Could not finish saving backup files; the folder may contain incomplete files.",
        )?;
    fs::rename(temporary, folder.join(name)).map_err(|_| "Could not finalize backup file.")?;
    fs::File::open(folder)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| "Could not flush the backup directory.")?;
    Ok(())
}

fn normal_matches(device: &device::Device, role: Role) -> bool {
    match role {
        Role::Left => device.role() == Some(KeyboardRole::Left),
        Role::Right => device.role() == Some(KeyboardRole::Right),
        Role::Receiver => device.rmk_receiver() || device.factory_left(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn modern_left_returns_in_wired_while_legacy_marker_keeps_old_start_guide() {
        let now = std::time::Instant::now();
        for legacy in [false, true] {
            let mut session = super::Session {
                role: Some(Role::Left),
                rmk_left: true,
                legacy_left_start: legacy,
                return_flow: ReturnFlow::at(super::ReturnPhase::OffWait { since: now }),
                ..Default::default()
            };
            session.advance_return(now + std::time::Duration::from_secs(5));
            assert_eq!(
                matches!(
                    session.return_flow.phase(),
                    Some(super::ReturnPhase::PowerOn)
                ),
                legacy
            );
            assert_eq!(
                matches!(
                    session.return_flow.phase(),
                    Some(super::ReturnPhase::Reconnect)
                ),
                !legacy
            );
        }
    }
    use super::*;
    use crate::device::Device;
    fn normal() -> Snapshot {
        Snapshot {
            devices: vec![Device {
                location: 7,
                vendor: 0x2886,
                product: 0x8029,
                name: "NocFree _ ANSI".into(),
            }],
            mounts: vec![],
        }
    }
    fn boot(mounted: bool) -> Snapshot {
        Snapshot {
            devices: vec![Device {
                location: 7,
                vendor: 0x239a,
                product: 0x29,
                name: "NocFree &".into(),
            }],
            mounts: if mounted {
                vec![BootMount {
                    path: PathBuf::from("/test/recovery"),
                    info: "UF2 Bootloader 0.9.2-39-g0147d71\r\nModel: NocFree &\r\nBoard-ID: NocFree &\r\n".into(),
                }]
            } else {
                vec![]
            },
        }
    }
    fn identified() -> Session {
        let mut session = Session::new();
        session.select(Role::Left);
        session.observe(Ok(normal()));
        session
    }
    #[test]
    fn modern_return_uses_same_port_normal_observation_not_a_polling_countdown() {
        let now = Instant::now();
        for elapsed in [
            Duration::from_millis(4900),
            Duration::from_secs(5),
            Duration::from_secs(6),
        ] {
            let mut session = identified();
            session.backup_path = Some(PathBuf::from("/saved"));
            session.archived_location = Some(7);
            session.return_flow.restart();
            session.observe_at(Ok(boot(true)), now);
            assert!(!session.view().return_complete);
            session.observe_at(Ok(Snapshot::default()), now + Duration::from_millis(100));
            session.observe_at(Ok(normal()), now + elapsed);
            assert!(
                session.view().return_complete,
                "normal return at {elapsed:?} must complete"
            );
        }
    }
    #[test]
    fn errors_are_not_normal_return_and_legacy_startup_keeps_its_guide() {
        let now = Instant::now();
        let mut session = identified();
        session.backup_path = Some(PathBuf::from("/saved"));
        session.archived_location = Some(7);
        session.return_flow.restart();
        session.observe_at(Err("Inventory failed".into()), now);
        assert!(!session.view().return_complete);
        let mut session = identified();
        session.backup_path = Some(PathBuf::from("/saved"));
        session.archived_location = Some(7);
        session.return_flow.restart();
        session.legacy_left_start = true;
        session.observe_at(Ok(Snapshot::default()), now);
        session.observe_at(Ok(normal()), now + Duration::from_secs(4));
        assert!(!session.view().return_complete);
    }
    #[test]
    fn already_returned_modern_left_completes_without_observed_disconnect_only_on_saved_port() {
        let now = Instant::now();
        let make = || {
            let mut session = identified();
            session.backup_path = Some(PathBuf::from("/saved"));
            session.archived_location = Some(7);
            session.return_flow.restart();
            session
        };
        let mut session = make();
        session.observe_at(Ok(normal()), now);
        assert!(session.view().return_complete);
        let mut session = make();
        let mut wrong = normal();
        wrong.devices[0].location = 8;
        session.observe_at(Ok(wrong), now);
        assert!(!session.view().return_complete);
        let mut session = make();
        let mut wrong = normal();
        wrong.devices[0].vendor = 0x4c4b;
        wrong.devices[0].product = 0x4671;
        wrong.devices[0].name = "NocFree RMK Right".into();
        session.observe_at(Ok(wrong), now);
        assert!(!session.view().return_complete);
        let mut session = make();
        session.observe_at(Ok(boot(true)), now);
        assert!(!session.view().return_complete);
    }
    #[test]
    fn receiver_runtime_binding_and_normal_observation_keep_firmware_identity() {
        let mut session = Session::new();
        session.select_recovery_role(Role::Receiver);
        session.bind_recovery(7);
        assert!(session.rmk_receiver);
        assert!(!session.rmk_left);
        let mut receiver = normal();
        receiver.devices[0].vendor = 0x4c4b;
        receiver.devices[0].product = 0x4644;
        receiver.devices[0].name = "NocFree RMK Receiver".into();
        session.observe(Ok(receiver));
        assert_eq!(session.factory_role(), None);
        session.observe(Ok(normal()));
        assert_eq!(session.factory_role(), Some(KeyboardRole::Left));
    }
    #[test]
    fn receiver_backup_preserves_component_label_and_returns_without_keyboard_steps() {
        let root = std::env::temp_dir().join(format!(
            "nocfree-dongle-backup-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut session = Session::new();
        session.select_recovery_role(Role::Receiver);
        assert!(session.adopt_archive_drive(boot(true)).unwrap());
        assert_eq!(session.selected_component(), Some(Role::Receiver));
        assert!(session.recovery_binding().is_err());
        let folder = session
            .save_with(&root, || Ok(boot(true)), |_| Ok(device::tests::archive()))
            .unwrap();
        let journal: serde_json::Value =
            serde_json::from_slice(&fs::read(folder.join("session.json")).unwrap()).unwrap();
        assert_eq!(journal["planned_role"], "Receiver");
        assert_eq!(journal["role_verified"], false);
        assert_eq!(session.view().title, "Unplug the USB dongle");
        assert!(!session.view().needs_power_on_ack);
        let mut receiver = normal();
        receiver.devices[0].vendor = 0x4c4b;
        receiver.devices[0].product = 0x4644;
        receiver.devices[0].name = "NocFree RMK Receiver".into();
        session.observe(Ok(receiver));
        assert!(session.view().return_complete);
        assert!(session.recovery_binding().is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn receiver_return_rejects_a_normal_left_on_the_saved_port() {
        let mut session = Session::new();
        session.select_recovery_role(Role::Receiver);
        assert!(session.adopt_archive_drive(boot(true)).unwrap());
        session.backup_path = Some(PathBuf::from("/saved"));
        session.archived_location = Some(7);
        session.return_flow.restart();
        let mut left = normal();
        left.devices[0].vendor = 0x4c4b;
        left.devices[0].product = 0x4643;
        left.devices[0].name = "NocFree RMK".into();
        session.observe(Ok(left));
        assert!(!session.view().return_complete);
        assert!(session.view().error.is_some());
    }
    #[test]
    fn already_mounted_backup_is_read_only_and_never_installation_evidence() {
        let mut session = Session::new();
        session.select_recovery_role(crate::runtime_recovery::Role::Left);
        assert!(session.adopt_archive_drive(boot(true)).unwrap());
        assert!(session.view().can_save);
        assert!(session.recovery_binding().is_err());
        assert!(!session.identified_normal());
        // Ordinary initial discovery remains insufficient for any write-capable recovery flow.
        let mut ordinary = Session::new();
        ordinary.select_recovery_role(crate::runtime_recovery::Role::Left);
        ordinary.observe(Ok(boot(true)));
        assert!(!ordinary.view().can_save);
    }
    #[test]
    fn permission_denied_during_fresh_backup_discovery_is_not_replaced_by_connection_error() {
        let mut session = Session::new();
        session.select(Role::Left);
        assert!(session.adopt_archive_drive(boot(true)).unwrap());
        let denied = "Allow NocFree RMK Companion to access Removable Volumes".to_owned();
        let error = session
            .save_with(
                Path::new("/unused-test-backup"),
                || Err(denied.clone()),
                |_| panic!("No read occurs after denied discovery"),
            )
            .unwrap_err();
        assert_eq!(error, denied);
        assert_eq!(session.view().error, Some(denied));
        assert!(!session.view().can_save);
    }
    #[test]
    fn mounted_backup_rejects_ambiguity_metadata_and_late_mount_has_no_fake_ready() {
        let mut session = Session::new();
        session.select(Role::Left);
        assert!(!session.adopt_archive_drive(boot(false)).unwrap());
        assert!(!session.view().can_save);
        let mut ambiguous = boot(true);
        ambiguous.devices.push(ambiguous.devices[0].clone());
        assert!(session.adopt_archive_drive(ambiguous).is_err());
        assert!(!session.view().can_save);
        let mut unknown = boot(true);
        unknown.mounts[0].info = "Unknown bootloader".into();
        assert!(session.adopt_archive_drive(unknown).is_err());
        assert!(!session.view().can_save);
        assert!(session.adopt_archive_drive(boot(true)).unwrap());
    }
    #[test]
    fn cable_still_connected_cannot_advance_and_late_mount_can() {
        let mut s = identified();
        s.observe(Ok(normal()));
        assert!(!s.view().can_save);
        s.observe(Ok(Snapshot::default()));
        assert!(!s.view().can_save);
        s.observe(Ok(boot(false)));
        assert!(!s.view().can_save);
        s.observe(Ok(boot(true)));
        assert!(s.view().can_save);
    }
    #[test]
    fn ambiguous_bootloaders_drop_binding() {
        let mut s = identified();
        let mut b = boot(true);
        let mut other = b.devices[0].clone();
        other.location = 8;
        b.devices.push(other);
        s.observe(Ok(b));
        assert!(!s.view().can_save);
        s.observe(Ok(boot(true)));
        assert!(!s.view().can_save);
    }
    #[test]
    fn unbound_restart_and_error_never_enable_save() {
        let mut s = Session::new();
        s.select(Role::Left);
        s.observe(Ok(boot(true)));
        assert!(!s.view().can_save);
        s.observe(Ok(normal()));
        s.observe(Ok(boot(true)));
        assert!(s.view().can_save);
        s.observe(Err("USB inventory unavailable".into()));
        assert!(!s.view().can_save);
        s.observe(Ok(boot(true)));
        assert!(!s.view().can_save);
        s.select(Role::Right);
        assert!(!s.view().can_save);
    }
    #[test]
    fn wrong_connection_and_metadata_rejected() {
        let mut s = identified();
        let mut b = boot(true);
        b.devices[0].location = 99;
        s.observe(Ok(b));
        assert!(!s.view().can_save);
        s.observe(Ok(normal()));
        let mut b = boot(true);
        b.mounts[0].info = "UF2 Bootloader unknown\nModel: NocFree &".into();
        s.observe(Ok(b));
        assert!(!s.view().can_save);
    }
    #[test]
    fn removal_during_backup_never_creates_output() {
        let mut s = identified();
        s.observe(Ok(boot(true)));
        let mut count = 0;
        let root =
            std::env::temp_dir().join(format!("nocfree-companion-removal-{}", std::process::id()));
        let outcome = s.save_with(
            &root,
            || {
                count += 1;
                Ok(if count == 1 {
                    boot(true)
                } else {
                    Snapshot::default()
                })
            },
            |_| Ok(crate::device::tests::archive()),
        );
        assert!(outcome.is_err());
        assert!(!root.exists());
        assert!(!s.view().can_save);
    }
    #[test]
    fn successful_archive_is_private_and_restart_needs_binding() {
        let mut s = identified();
        s.observe(Ok(boot(true)));
        let root = std::env::temp_dir().join(format!(
            "nocfree-companion-save-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = s
            .save_with(
                &root,
                || Ok(boot(true)),
                |_| Ok(crate::device::tests::archive()),
            )
            .unwrap();
        assert!(path.join("CURRENT.UF2").is_file());
        assert!(path.join("session.json").is_file());
        assert!(!s.view().can_save);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path.join("CURRENT.UF2"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        s.observe(Ok(normal()));
        s.observe(Ok(Snapshot::default()));
        assert!(!s.view().return_complete);
        s.observe(Err("USB discovery failed".into()));
        assert!(!s.view().return_complete);
        assert!(s.view().error.is_some());
        let mut restarted = Session::new();
        restarted.select(Role::Left);
        restarted.observe(Ok(boot(true)));
        assert!(!restarted.view().can_save);
        let now = Instant::now();
        s.retry();
        assert_eq!(s.view().backup_path.as_ref(), Some(&path));
        s.observe_at(Ok(boot(true)), now);
        assert!(!s.view().can_save);
        assert!(!s.view().return_complete);
        s.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(1));
        s.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(6));
        s.observe_at(Ok(normal()), now + Duration::from_secs(7));
        assert!(s.view().return_complete);
        assert!(!s.view().can_save);
        assert_eq!(s.view().backup_path.as_ref(), Some(&path));
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        let journal: serde_json::Value =
            serde_json::from_slice(&fs::read(path.join("session.json")).unwrap()).unwrap();
        assert_eq!(journal["role_verified"], false);
        assert_eq!(journal["planned_role"], "Left");
        assert!(journal.get("role").is_none());
        fs::remove_dir_all(root).unwrap();
    }
    fn saved_session(role: Role) -> Session {
        let mut s = identified();
        s.role = Some(role);
        s.backup_path = Some(PathBuf::from("/private/test-copy"));
        s.return_flow.restart();
        s
    }
    #[test]
    fn shared_recovery_is_adopted_only_before_saving_for_the_selected_half() {
        let mut recovered = Session::new();
        recovered.select(Role::Left);
        recovered.bind_recovery(7);
        recovered.observe(Ok(boot(true)));
        let mut journey = crate::journey::Journey::backup();
        assert!(journey.accept_recovery(recovered));
        assert_eq!(journey.state(), crate::journey::State::ReadyToSave);

        let mut wrong_role = Session::new();
        wrong_role.select(Role::Right);
        wrong_role.bind_recovery(7);
        wrong_role.observe(Ok(boot(true)));
        let mut fresh = crate::journey::Journey::backup();
        assert!(!fresh.accept_recovery(wrong_role));
        fresh.pause();
        assert!(!fresh.accept_recovery(identified()));
        assert_eq!(fresh.state(), crate::journey::State::Paused);
    }
    #[test]
    fn local_recovery_binding_rejects_wrong_port_or_unreviewed_board_metadata() {
        let mut session = Session::new();
        session.select(Role::Left);
        session.bind_recovery(8);
        session.observe(Ok(boot(true)));
        assert!(!session.view().can_save);
        assert!(session.view().error.is_some());
        session.select(Role::Left);
        session.bind_recovery(7);
        let mut unknown = boot(true);
        unknown.mounts[0].info = "UF2 Bootloader 0.9.2-39-g0147d71\nModel: NocFree &".into();
        session.observe(Ok(unknown));
        assert!(!session.view().can_save);
        assert!(session.view().error.is_some());
    }
    #[test]
    fn factory_right_identity_selects_its_hold_shortcut() {
        let mut snapshot = normal();
        snapshot.devices[0].vendor = 0x239a;
        snapshot.devices[0].product = 0x80d8;
        snapshot.devices[0].name = "NocFree nRF52833 Right".into();
        let mut session = Session::new();
        session.select(Role::Right);
        session.observe(Ok(snapshot));
        assert_eq!(session.factory_role(), Some(KeyboardRole::Right));
        assert!(session.view().instruction.contains("five seconds"));
        assert!(!session.view().can_save);
    }
    #[test]
    fn selected_backup_completes_without_automatically_visiting_another_half() {
        let now = Instant::now();
        let mut session = saved_session(Role::Left);
        session.observe_at(Ok(Snapshot::default()), now);
        session.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(5));
        session.observe_at(Ok(normal()), now + Duration::from_secs(6));
        let mut journey = crate::journey::Journey::from_saved_test_session(session);
        journey.observe(Ok(normal()));
        assert_eq!(journey.component(), Role::Left);
        assert!(journey.is_complete());
        journey.pause();
        journey.resume();
        assert!(journey.is_complete());
        assert_eq!(journey.archives(), &[PathBuf::from("/private/test-copy")]);
    }
    #[test]
    fn left_only_journey_completes_after_observed_normal_return() {
        let now = Instant::now();
        let mut session = saved_session(Role::Left);
        session.observe_at(Ok(Snapshot::default()), now);
        session.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(5));
        session.observe_at(Ok(normal()), now + Duration::from_secs(6));
        let mut journey = crate::journey::Journey::from_saved_test_session(session);
        journey.observe(Ok(normal()));
        assert!(journey.is_complete());
        assert_eq!(journey.role(), Role::Left);
        assert_eq!(journey.archives(), &[PathBuf::from("/private/test-copy")]);
    }
    #[test]
    fn left_return_requires_observed_absence_then_full_wait() {
        let now = Instant::now();
        let mut s = saved_session(Role::Left);
        s.observe_at(Ok(normal()), now);
        assert!(!s.view().return_complete);
        s.observe_at(Ok(Snapshot::default()), now);
        s.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(4));
        assert!(matches!(
            s.return_flow.phase(),
            Some(ReturnPhase::OffWait { .. })
        ));
        s.observe_at(Ok(normal()), now + Duration::from_secs(4));
        assert!(matches!(
            s.return_flow.phase(),
            Some(ReturnPhase::Disconnect)
        ));
        assert!(!s.view().return_complete);
        s.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(5));
        s.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(10));
        assert!(matches!(
            s.return_flow.phase(),
            Some(ReturnPhase::Reconnect)
        ));
        s.observe_at(Ok(boot(true)), now + Duration::from_secs(11));
        assert!(matches!(
            s.return_flow.phase(),
            Some(ReturnPhase::Disconnect)
        ));
        assert!(s.view().error.is_none());
        assert!(!s.view().can_save);
        assert!(s.view().instruction.contains("stayed in recovery"));
        s.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(12));
        s.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(17));
        s.observe_at(Ok(normal()), now + Duration::from_secs(18));
        assert!(s.view().return_complete);
        s.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(19));
        assert!(!s.view().return_complete);
    }
    #[test]
    fn right_start_wait_begins_only_after_explicit_power_ack() {
        let now = Instant::now();
        let mut s = saved_session(Role::Right);
        s.observe_at(Ok(Snapshot::default()), now);
        s.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(5));
        assert!(s.view().needs_power_on_ack);
        s.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(100));
        assert!(s.view().needs_power_on_ack);
        s.confirm_power_on_at(now + Duration::from_secs(100));
        s.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(109));
        assert!(matches!(
            s.return_flow.phase(),
            Some(ReturnPhase::StartWait { .. })
        ));
        s.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(110));
        assert!(matches!(
            s.return_flow.phase(),
            Some(ReturnPhase::Reconnect)
        ));
        let mut right = normal();
        right.devices[0].vendor = 0x4c4b;
        right.devices[0].product = 0x4651;
        right.devices[0].name = "NocFree Input Probe Right Mac".into();
        s.observe_at(Ok(right), now + Duration::from_secs(111));
        assert!(s.view().return_complete);
        s.observe_at(
            Err("USB discovery failed".into()),
            now + Duration::from_secs(112),
        );
        assert!(!s.view().return_complete);
        assert!(!s.view().needs_power_on_ack);
    }
    #[test]
    fn premature_right_usb_and_discovery_failure_invalidate_waits() {
        let now = Instant::now();
        let mut s = saved_session(Role::Right);
        s.observe_at(Ok(Snapshot::default()), now);
        s.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(5));
        s.confirm_power_on_at(now + Duration::from_secs(5));
        s.observe_at(Ok(boot(true)), now + Duration::from_secs(6));
        assert!(matches!(
            s.return_flow.phase(),
            Some(ReturnPhase::Disconnect)
        ));
        s.confirm_power_on_at(now + Duration::from_secs(7));
        assert!(matches!(
            s.return_flow.phase(),
            Some(ReturnPhase::Disconnect)
        ));
        s.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(8));
        s.observe_at(Err("USB unavailable".into()), now + Duration::from_secs(9));
        assert!(matches!(
            s.return_flow.phase(),
            Some(ReturnPhase::Disconnect)
        ));
        assert!(!s.view().return_complete);
    }
    #[test]
    fn rmk_identity_does_not_guess_a_shortcut_and_legacy_return_still_checks_observations() {
        let now = Instant::now();
        let mut rmk = normal();
        rmk.devices[0].vendor = 0x4c4b;
        rmk.devices[0].product = 0x4643;
        rmk.devices[0].name = "NocFree RMK".into();
        let mut s = Session::new();
        s.select(Role::Left);
        s.observe_at(Ok(rmk.clone()), now);
        assert!(s.view().instruction.contains("installed firmware"));
        assert!(!s.view().instruction.contains("Escape"));
        assert!(!s.view().instruction.contains("Fn + 5"));
        s.observe_at(Ok(boot(true)), now);
        assert!(s.view().can_save);
        // Same saved-archive return seam exercised by the existing factory tests.
        // Legacy marker classification comes from saved bytes, never the USB name.
        s.legacy_left_start = true;
        s.backup_path = Some(PathBuf::from("/private/rmk-fixture"));
        s.return_flow.restart();
        s.observe_at(Ok(Snapshot::default()), now);
        s.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(5));
        assert!(s.view().needs_power_on_ack);
        assert!(s.view().title.contains("Bluetooth"));
        s.confirm_power_on_at(now + Duration::from_secs(5));
        s.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(14));
        assert!(matches!(
            s.return_flow.phase(),
            Some(ReturnPhase::StartWait { .. })
        ));
        s.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(15));
        assert!(matches!(
            s.return_flow.phase(),
            Some(ReturnPhase::Reconnect)
        ));
        s.observe_at(Ok(rmk), now + Duration::from_secs(16));
        assert!(s.view().return_complete);
    }
}

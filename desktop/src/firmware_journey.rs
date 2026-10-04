//! A guarded, sequential application update. Recovery acquisition belongs to the
//! shared recovery worker; a copy is attempted once and reconciled by readback.
use crate::{
    device::{self, BootMount, Snapshot},
    release::{FirmwareRelease, ReleaseImage},
    runtime_recovery::Role,
    session::Session,
    update_image,
};
use sha2::{Digest, Sha256};
use statig::{
    Outcome,
    blocking::{IntoStateMachine, IntoStateMachineExt, State as StatigState, StateMachine},
};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const PLAN: [Role; 3] = [Role::Receiver, Role::Right, Role::Left];
const ARCHIVE_LIMIT: usize = 1728 * 512;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Recovery,
    Approval,
    Reconcile,
    Disconnect,
    OffWait(Instant),
    PowerOn,
    StartWait(Instant),
    Reconnect,
    Complete,
    Failed,
    Cancelled,
}
#[derive(Clone)]
pub struct View {
    pub title: String,
    pub instruction: String,
    pub role: Role,
    pub step: usize,
    pub needs_power_on_ack: bool,
    pub can_transfer: bool,
    pub needs_recovery: bool,
    pub verification: bool,
    pub complete: bool,
    pub error: Option<String>,
}
#[derive(Clone)]
struct Baseline {
    location: u64,
    mount: BootMount,
    folder: PathBuf,
    bytes: Vec<u8>,
}
struct FirmwareData {
    release: FirmwareRelease,
    index: usize,
    baseline: Option<Baseline>,
    error: Option<String>,
    attempted: bool,
    // This is failure history, not a second mutable current state.
    retry_phase: Option<Phase>,
}
impl FirmwareData {
    fn role(&self) -> Role {
        PLAN[self.index]
    }
}

enum FirmwareEvent<'a> {
    RecoverySaved(&'a Baseline, bool),
    TransferAttempted,
    TransferResult(&'a Result<(), String>),
    Verified,
    PowerOn(Instant),
    Observe(&'a Result<Snapshot, String>, Instant),
    Cancel,
}
impl IntoStateMachine for FirmwareData {
    type Event<'a> = FirmwareEvent<'a>;
    type Context<'a> = ();
    type State = Phase;
    type Superstate<'a> = ();
    fn initial() -> Phase {
        Phase::Recovery
    }
}
impl StatigState<FirmwareData> for Phase {
    fn call_handler(
        &mut self,
        data: &mut FirmwareData,
        event: &FirmwareEvent<'_>,
        _: &mut (),
    ) -> Outcome<Self> {
        use Outcome::{Handled, Transition};
        match event {
            FirmwareEvent::Cancel => {
                data.retry_phase = None;
                return Transition(Phase::Cancelled);
            }
            FirmwareEvent::RecoverySaved(baseline, installed) if *self == Phase::Recovery => {
                data.baseline = Some((*baseline).clone());
                data.error = None;
                return Transition(if *installed {
                    Phase::Disconnect
                } else {
                    Phase::Approval
                });
            }
            FirmwareEvent::TransferAttempted if *self == Phase::Approval && !data.attempted => {
                data.attempted = true;
                return Transition(Phase::Reconcile);
            }
            FirmwareEvent::TransferResult(result) if *self == Phase::Reconcile => {
                data.error = result.as_ref().err().cloned();
                return Handled;
            }
            FirmwareEvent::Verified if *self == Phase::Reconcile => {
                data.error = None;
                return Transition(Phase::Disconnect);
            }
            FirmwareEvent::PowerOn(now) if *self == Phase::PowerOn => {
                return Transition(Phase::StartWait(*now));
            }
            _ => {}
        }
        let FirmwareEvent::Observe(observation, now) = event else {
            return Handled;
        };
        if matches!(self, Phase::Complete | Phase::Cancelled) {
            return Handled;
        }
        let (snapshot, phase) = match observation {
            Err(error) => {
                if *self != Phase::Failed {
                    data.retry_phase = Some(match self {
                        Phase::OffWait(_)
                        | Phase::PowerOn
                        | Phase::StartWait(_)
                        | Phase::Reconnect => Phase::Disconnect,
                        phase => *phase,
                    });
                }
                data.error = Some(format!("Could not check the keyboard: {error}"));
                return Transition(Phase::Failed);
            }
            Ok(snapshot) => {
                let phase = if *self == Phase::Failed {
                    data.error = None;
                    data.retry_phase.take().unwrap_or(Phase::Recovery)
                } else {
                    *self
                };
                (snapshot, phase)
            }
        };
        let Some(baseline) = &data.baseline else {
            return if phase == *self {
                Handled
            } else {
                Transition(phase)
            };
        };
        let connected = snapshot
            .devices
            .iter()
            .any(|d| d.location == baseline.location);
        let next = match phase {
            Phase::Disconnect if !connected => Phase::OffWait(*now),
            Phase::OffWait(_) | Phase::PowerOn | Phase::StartWait(_) if connected => {
                Phase::Disconnect
            }
            Phase::OffWait(since)
                if now.saturating_duration_since(since) >= Duration::from_secs(5) =>
            {
                if data.role() == Role::Right {
                    Phase::PowerOn
                } else {
                    Phase::Reconnect
                }
            }
            Phase::StartWait(since)
                if now.saturating_duration_since(since) >= Duration::from_secs(10) =>
            {
                Phase::Reconnect
            }
            Phase::Reconnect if connected => {
                let normal: Vec<_> = snapshot
                    .devices
                    .iter()
                    .filter(|d| {
                        d.vendor == 0x4c4b
                            && d.product == data.role().product() as u64
                            && d.name == data.role().name()
                    })
                    .collect();
                if normal.len() == 1
                    && normal[0].location == baseline.location
                    && snapshot.mounts.is_empty()
                    && !snapshot.devices.iter().any(|d| d.bootloader())
                {
                    if data.index == PLAN.len() - 1 {
                        Phase::Complete
                    } else {
                        data.index += 1;
                        data.baseline = None;
                        data.attempted = false;
                        Phase::Recovery
                    }
                } else {
                    Phase::Disconnect
                }
            }
            phase => phase,
        };
        if next == *self {
            Handled
        } else {
            Transition(next)
        }
    }
}

pub struct FirmwareJourney {
    machine: StateMachine<FirmwareData>,
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn flash_bytes(archive: &[u8]) -> Result<Vec<u8>, String> {
    device::inspect_archive(archive)?;
    let mut bytes = vec![0; 0x6d000 - 0x1000];
    for block in archive.as_chunks::<512>().0 {
        let address = u32::from_le_bytes(block[12..16].try_into().unwrap()) as usize;
        bytes[address - 0x1000..address - 0x1000 + 256].copy_from_slice(&block[32..288]);
    }
    Ok(bytes)
}
fn target(image: &ReleaseImage) -> Result<(usize, Vec<u8>), String> {
    let proof = update_image::validate_for(image.proof.policy(), &image.uf2, &image.binary)
        .map_err(|e| e.to_string())?;
    if proof.sha256() != image.proof.sha256()
        || proof.binary_sha256() != image.proof.binary_sha256()
        || proof.sha256() != image.metadata.uf2_sha256
        || proof.binary_sha256() != image.metadata.binary_sha256
        || proof.role() != image.proof.role()
    {
        return Err("Bundled firmware changed; installation is unavailable.".into());
    }
    let mut bytes = image.binary.clone();
    bytes.resize((proof.end_exclusive() - proof.start()) as usize, 0xff);
    Ok(((proof.start() - 0x1000) as usize, bytes))
}
fn exact_candidate(archive: &[u8], image: &ReleaseImage) -> Result<bool, String> {
    let bytes = flash_bytes(archive)?;
    let (start, target) = target(image)?;
    Ok(bytes[start..start + target.len()] == target)
}
fn verify(archive: &[u8], baseline: &[u8], image: &ReleaseImage) -> Result<usize, String> {
    let actual = flash_bytes(archive)?;
    let mut expected = flash_bytes(baseline)?;
    let (start, target) = target(image)?;
    expected[start..start + target.len()].copy_from_slice(&target);
    // Configuration may migrate, but the entire application, padding and untouched
    // flash through 0x65000 must remain exactly as planned.
    let boundary = 0x65000 - 0x1000;
    if actual[..boundary] != expected[..boundary] {
        return Err(
            "Firmware readback differs from the planned image. Keep the recovery drive connected."
                .into(),
        );
    }
    Ok(actual[boundary..]
        .iter()
        .zip(&expected[boundary..])
        .filter(|(a, b)| a != b)
        .count())
}
fn durable(folder: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(folder.join(name))
        .map_err(|_| "A saved operation already exists or could not be created.")?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Could not save the operation record.")?;
    fs::File::open(folder)
        .and_then(|f| f.sync_all())
        .map_err(|_| "Could not make the operation record durable.".to_owned())
}
fn correlated(snapshot: &Snapshot, location: u64, mount: &BootMount) -> bool {
    let boots: Vec<_> = snapshot.devices.iter().filter(|d| d.bootloader()).collect();
    boots.len() == 1
        && boots[0].location == location
        && snapshot.mounts.as_slice() == [mount.clone()]
}
// An app restart does not erase uncertainty. A pending journal for this part
// must reconcile against its original backup and the same pinned target.
fn reconcile_prior(
    folder: &Path,
    role: Role,
    location: u64,
    actual: &[u8],
    image: &ReleaseImage,
) -> Result<(), String> {
    let root = folder.parent().ok_or("Backup folder is unavailable.")?;
    for entry in fs::read_dir(root).map_err(|_| "Could not check previous installations.")? {
        let entry = entry.map_err(|_| "Could not check previous installations.")?;
        if !entry
            .file_type()
            .map_err(|_| "Could not check installation record.")?
            .is_dir()
        {
            continue;
        }
        let prior = entry.path();
        if prior == folder || !prior.join("install-intent.json").exists() {
            continue;
        }
        let record = device::read_bounded(&prior.join("install-intent.json"), 8192)?;
        let record: serde_json::Value = serde_json::from_slice(&record)
            .map_err(|_| "An earlier installation record is unreadable.")?;
        if record["schema"].as_u64() != Some(1)
            || !matches!(record["role"].as_str(), Some("Left" | "Right" | "Receiver"))
        {
            return Err("An earlier installation record is incomplete.".into());
        }
        if prior.join("install-verified.json").exists() {
            let bytes = device::read_bounded(&prior.join("install-verified.json"), 8192)?;
            let completed: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|_| "An installation verification record is incomplete.")?;
            if completed["schema"].as_u64() != Some(1)
                || !completed["readback_sha256"]
                    .as_str()
                    .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                return Err("An installation verification record is incomplete.".into());
            }
            continue;
        }
        if record["role"].as_str() != Some(format!("{role:?}").as_str()) {
            continue;
        }
        if record["location"].as_u64() != Some(location)
            || record["target_sha256"].as_str() != Some(image.metadata.uf2_sha256.as_str())
        {
            return Err("An earlier installation needs reconciliation on its original USB connection and release.".into());
        }
        let previous = device::read_bounded(&prior.join("CURRENT.UF2"), ARCHIVE_LIMIT)?;
        if record["backup_sha256"].as_str() != Some(hash(&previous).as_str()) {
            return Err("The earlier installation backup changed.".into());
        }
        let storage = verify(actual, &previous, image)?;
        durable(&prior,"install-verified.json",&serde_json::to_vec_pretty(&serde_json::json!({"schema":1,"readback_sha256":hash(actual),"settings_changed_bytes":storage,"reconciled_after_restart":true})).map_err(|_| "Could not encode reconciliation.")?)?;
    }
    Ok(())
}
impl FirmwareJourney {
    pub fn new(release: FirmwareRelease) -> Self {
        Self {
            machine: FirmwareData {
                release,
                index: 0,
                baseline: None,
                error: None,
                attempted: false,
                retry_phase: None,
            }
            .state_machine(),
        }
    }
    pub fn role(&self) -> Role {
        PLAN[self.machine.inner().index]
    }
    pub fn view(&self) -> View {
        let (title, instruction) = match *self.machine.state() {
            Phase::Recovery => (
                "Connect your keyboard",
                "Follow the recovery steps to save a copy before installing.",
            ),
            Phase::Approval => (
                "Ready to install",
                "Your firmware copy is saved. Install the update when you’re ready.",
            ),
            Phase::Reconcile => (
                "Check the installed firmware",
                "Open recovery again. We’ll check the saved bytes before continuing.",
            ),
            Phase::Disconnect if self.role() == Role::Right => (
                "Unplug the right half",
                "Turn its switch OFF, then unplug USB.",
            ),
            Phase::Disconnect if self.role() == Role::Left => (
                "Unplug the left half",
                "Set its switch to middle WIRED, then unplug USB.",
            ),
            Phase::Disconnect => ("Unplug the dongle", "Unplug the dongle for five seconds."),
            Phase::OffWait(_) => (
                "Keep it unplugged",
                "Wait five seconds before reconnecting.",
            ),
            Phase::PowerOn => (
                "Turn the right half ON",
                "Keep USB unplugged. Turn it ON, then continue.",
            ),
            Phase::StartWait(_) => ("Let it start", "Keep USB unplugged for ten seconds."),
            Phase::Reconnect => ("Reconnect USB", "Reconnect it to the same USB port."),
            Phase::Complete => (
                "You’re up to date",
                "All three parts are running the installed firmware.",
            ),
            Phase::Failed => (
                "Check your connection",
                "Check USB and recovery drive access. We’ll continue when the connection can be checked again.",
            ),
            Phase::Cancelled => (
                "Installation paused",
                "Your saved copies are kept. An attempted transfer must be checked before another installation.",
            ),
        };
        View {
            title: title.into(),
            instruction: instruction.into(),
            role: self.role(),
            step: self.machine.inner().index,
            needs_power_on_ack: *self.machine.state() == Phase::PowerOn,
            can_transfer: *self.machine.state() == Phase::Approval
                && !self.machine.inner().attempted,
            needs_recovery: matches!(*self.machine.state(), Phase::Recovery | Phase::Reconcile),
            verification: *self.machine.state() == Phase::Reconcile,
            complete: *self.machine.state() == Phase::Complete,
            error: self.machine.inner().error.clone(),
        }
    }
    pub fn accept_recovery(&mut self, mut session: Session) -> Result<(), String> {
        if *self.machine.state() != Phase::Recovery {
            return Err("This recovery result is no longer needed.".into());
        }
        let (role, location, mount) = session.recovery_binding()?;
        if role != self.role() {
            return Err("Recovery belongs to a different component.".into());
        }
        let folder = session.save_backup()?;
        let bytes = device::read_bounded(&folder.join("CURRENT.UF2"), ARCHIVE_LIMIT)?;
        device::inspect_archive(&bytes)?;
        reconcile_prior(
            &folder,
            role,
            location,
            &bytes,
            self.machine.inner().release.image(role),
        )?;
        let baseline = Baseline {
            location,
            mount,
            folder,
            bytes,
        };
        let installed = exact_candidate(&baseline.bytes, self.machine.inner().release.image(role))?;
        self.machine
            .handle(&FirmwareEvent::RecoverySaved(&baseline, installed));
        Ok(())
    }
    pub fn transfer(&mut self) -> Result<(), String> {
        if *self.machine.state() != Phase::Approval || self.machine.inner().attempted {
            return Err(
                "A transfer cannot be repeated. Check the installed firmware first.".into(),
            );
        }
        let baseline = self
            .machine
            .inner()
            .baseline
            .as_ref()
            .ok_or("A fresh backup is required.")?
            .clone();
        let image = self.machine.inner().release.image(self.role()).clone();
        target(&image)?;
        if !correlated(&device::discover()?, baseline.location, &baseline.mount) {
            return Err("Recovery connection changed. No firmware was copied.".into());
        }
        let fresh = device::read_bounded(&baseline.mount.path.join("CURRENT.UF2"), ARCHIVE_LIMIT)?;
        if fresh != baseline.bytes
            || device::read_bounded(&baseline.folder.join("CURRENT.UF2"), ARCHIVE_LIMIT)?
                != baseline.bytes
            || device::read_bounded(&baseline.folder.join("INFO_UF2.TXT"), 8192)?
                != baseline.mount.info.as_bytes()
            || device::read_bounded(&baseline.mount.path.join("INFO_UF2.TXT"), 8192)
                .map_err(|_| "Recovery metadata is unavailable.")?
                != baseline.mount.info.as_bytes()
            || !correlated(&device::discover()?, baseline.location, &baseline.mount)
        {
            return Err(
                "Firmware or connection changed after backup. No firmware was copied.".into(),
            );
        }
        let intent = serde_json::json!({"schema":1,"release":self.machine.inner().release.id(),"role":format!("{:?}",self.role()),"backup_sha256":hash(&fresh),"target_sha256":image.metadata.uf2_sha256,"location":baseline.location,"attempt":"one-shot; reconcile readback before continuing"});
        durable(
            &baseline.folder,
            "install-intent.json",
            &serde_json::to_vec_pretty(&intent)
                .map_err(|_| "Could not encode installation record.")?,
        )?;
        // Set the state before touching the drive: even a partial/failed copy cannot
        // return this live journey to an install button.
        self.machine.handle(&FirmwareEvent::TransferAttempted);
        let result = (|| {
            if !correlated(&device::discover()?, baseline.location, &baseline.mount)
                || device::read_bounded(&baseline.mount.path.join("INFO_UF2.TXT"), 8192)?
                    != baseline.mount.info.as_bytes()
            {
                return Err("Recovery connection changed after the operation was saved. Check its readback before continuing.".into());
            }
            let mut destination = fs::OpenOptions::new().write(true).create_new(true).open(baseline.mount.path.join("COMPANION.UF2")).map_err(|_| "The transfer could not start. Check the recovery drive; don’t repeat the copy.")?;
            destination
                .write_all(&image.uf2)
                .and_then(|_| destination.sync_all())
                .map_err(|_| {
                    "The transfer result is uncertain. Open recovery to check it.".to_owned()
                })
        })();
        self.machine.handle(&FirmwareEvent::TransferResult(&result));
        result
    }
    pub fn accept_verification(&mut self, mut session: Session) -> Result<(), String> {
        if *self.machine.state() != Phase::Reconcile {
            return Err("This verification result is no longer needed.".into());
        }
        let (role, location, mount) = session.recovery_binding()?;
        let baseline = self
            .machine
            .inner()
            .baseline
            .as_ref()
            .ok_or("Saved installation evidence is missing.")?;
        if role != self.role() || location != baseline.location || mount.info != baseline.mount.info
        {
            return Err("Verification belongs to a different component or USB connection.".into());
        }
        let folder = session.save_backup()?;
        let actual = device::read_bounded(&folder.join("CURRENT.UF2"), ARCHIVE_LIMIT)?;
        let changed_storage = verify(
            &actual,
            &baseline.bytes,
            self.machine.inner().release.image(role),
        )?;
        durable(&baseline.folder,"install-verified.json",&serde_json::to_vec_pretty(&serde_json::json!({"schema":1,"readback_sha256":hash(&actual),"settings_changed_bytes":changed_storage,"settings_range":"0x65000..0x6d000","readback":folder})).map_err(|_| "Could not encode verification record.")?)?;
        self.machine.handle(&FirmwareEvent::Verified);
        Ok(())
    }
    pub fn confirm_power_on(&mut self) {
        self.machine.handle(&FirmwareEvent::PowerOn(Instant::now()));
    }
    pub fn observe(&mut self, observation: Result<Snapshot, String>) {
        self.observe_at(observation, Instant::now());
    }
    fn observe_at(&mut self, observation: Result<Snapshot, String>, now: Instant) {
        self.machine
            .handle(&FirmwareEvent::Observe(&observation, now));
    }
    pub fn cancel(&mut self) {
        self.machine.handle(&FirmwareEvent::Cancel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::Device;
    fn archive_with(image: &ReleaseImage) -> Vec<u8> {
        let mut archive = device::tests::archive();
        let (start, target) = target(image).unwrap();
        for (i, byte) in target.iter().enumerate() {
            let offset = start + i;
            archive[(offset / 256) * 512 + 32 + offset % 256] = *byte;
        }
        archive
    }
    fn model(role: Role) -> FirmwareJourney {
        let mut journey = FirmwareJourney::new(crate::release::fixture());
        unsafe {
            journey.machine.inner_mut().index = PLAN.iter().position(|r| *r == role).unwrap();
            journey.machine.inner_mut().baseline = Some(Baseline {
                location: 10,
                mount: BootMount {
                    path: PathBuf::new(),
                    info: String::new(),
                },
                folder: PathBuf::new(),
                bytes: device::tests::archive(),
            });
            *journey.machine.state_mut() = Phase::Disconnect;
        }
        journey
    }
    fn normal(role: Role, location: u64) -> Snapshot {
        Snapshot {
            devices: vec![Device {
                location,
                vendor: 0x4c4b,
                product: role.product() as u64,
                name: role.name().into(),
            }],
            mounts: vec![],
        }
    }
    #[test]
    fn explicit_events_keep_transfer_one_shot_and_reject_late_completion() {
        let mut journey = FirmwareJourney::new(crate::release::fixture());
        let baseline = Baseline {
            location: 10,
            mount: BootMount {
                path: PathBuf::new(),
                info: String::new(),
            },
            folder: PathBuf::new(),
            bytes: device::tests::archive(),
        };
        journey.machine.handle(&FirmwareEvent::Verified);
        journey.machine.handle(&FirmwareEvent::TransferAttempted);
        assert_eq!(*journey.machine.state(), Phase::Recovery);
        assert!(!journey.machine.inner().attempted);
        journey
            .machine
            .handle(&FirmwareEvent::RecoverySaved(&baseline, false));
        assert_eq!(*journey.machine.state(), Phase::Approval);
        journey.machine.handle(&FirmwareEvent::TransferAttempted);
        assert_eq!(*journey.machine.state(), Phase::Reconcile);
        assert!(journey.machine.inner().attempted);
        journey.machine.handle(&FirmwareEvent::TransferAttempted);
        journey.observe(Err("inventory denied".into()));
        assert_eq!(*journey.machine.state(), Phase::Failed);
        journey.observe(Ok(Snapshot {
            devices: vec![],
            mounts: vec![],
        }));
        assert_eq!(*journey.machine.state(), Phase::Reconcile);
        assert!(journey.machine.inner().attempted);
        assert_eq!(
            journey.machine.inner().baseline.as_ref().unwrap().bytes,
            baseline.bytes
        );
        journey.cancel();
        journey.machine.handle(&FirmwareEvent::Verified);
        journey
            .machine
            .handle(&FirmwareEvent::TransferResult(&Ok(())));
        assert_eq!(*journey.machine.state(), Phase::Cancelled);
        assert!(journey.machine.inner().attempted);
    }
    #[test]
    fn readback_requires_target_padding_and_untouched_gap_but_reports_settings_separately() {
        let release = crate::release::fixture();
        for role in PLAN {
            let image = release.image(role);
            let baseline = device::tests::archive();
            let mut actual = archive_with(image);
            assert_eq!(verify(&actual, &baseline, image).unwrap(), 0);
            let gap = image.proof.end_exclusive() as usize - 0x1000;
            actual[(gap / 256) * 512 + 32 + gap % 256] ^= 1;
            assert!(verify(&actual, &baseline, image).is_err());
            actual = archive_with(image);
            let storage = 0x65000 - 0x1000;
            actual[(storage / 256) * 512 + 32] = 1;
            assert_eq!(verify(&actual, &baseline, image).unwrap(), 1);
            // Last page padding is just as important as the executable bytes.
            let padding = image.proof.binary_size();
            if padding < (image.proof.end_exclusive() - image.proof.start()) as usize {
                let offset = (image.proof.start() as usize - 0x1000) + padding;
                actual[(offset / 256) * 512 + 32 + offset % 256] ^= 1;
                assert!(verify(&actual, &baseline, image).is_err());
            }
        }
    }
    #[test]
    fn cancelled_and_attempted_journeys_cannot_transfer_or_accept_late_results() {
        let mut journey = model(Role::Right);
        unsafe {
            *journey.machine.state_mut() = Phase::Approval;
        }
        unsafe {
            journey.machine.inner_mut().attempted = true;
        }
        assert!(journey.transfer().is_err());
        journey.cancel();
        assert!(journey.accept_recovery(Session::new()).is_err());
        assert!(journey.accept_verification(Session::new()).is_err());
        journey.observe(Ok(normal(Role::Right, 10)));
        assert_eq!(*journey.machine.state(), Phase::Cancelled);
    }
    #[test]
    fn recovery_role_is_explicit_and_wrong_role_is_rejected_before_io() {
        let mut journey = FirmwareJourney::new(crate::release::fixture());
        let mut session = Session::new();
        session.select_recovery_role(Role::Left);
        session.bind_recovery(10);
        session.observe(Ok(Snapshot {
            devices: vec![Device {
                location: 10,
                vendor: 0x239a,
                product: 0x0029,
                name: "UF2".into(),
            }],
            mounts: vec![BootMount {
                path: PathBuf::new(),
                info: "UF2 Bootloader 0.9.2-39-g0147d71\nModel: NocFree &\nBoard-ID: NocFree &"
                    .into(),
            }],
        }));
        assert!(
            journey
                .accept_recovery(session)
                .unwrap_err()
                .contains("different component")
        );
    }
    #[test]
    fn right_restart_requires_off_wait_ack_start_wait_and_unique_same_port_normal() {
        let mut journey = model(Role::Right);
        let now = Instant::now();
        journey.observe_at(Ok(Snapshot::default()), now);
        journey.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(5));
        assert_eq!(*journey.machine.state(), Phase::PowerOn);
        assert!(journey.view().needs_power_on_ack);
        unsafe {
            *journey.machine.state_mut() = Phase::StartWait(now + Duration::from_secs(5));
        }
        journey.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(14));
        assert!(matches!(*journey.machine.state(), Phase::StartWait(_)));
        journey.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(15));
        assert_eq!(*journey.machine.state(), Phase::Reconnect);
        journey.observe_at(Ok(normal(Role::Right, 11)), now + Duration::from_secs(16));
        assert_eq!(*journey.machine.state(), Phase::Reconnect);
        journey.observe_at(Ok(normal(Role::Left, 10)), now + Duration::from_secs(17));
        assert_eq!(*journey.machine.state(), Phase::Disconnect);
        unsafe {
            *journey.machine.state_mut() = Phase::Reconnect;
        }
        journey.observe_at(Ok(normal(Role::Right, 10)), now + Duration::from_secs(18));
        assert_eq!(journey.role(), Role::Left);
        assert_eq!(*journey.machine.state(), Phase::Recovery);
    }
    #[test]
    fn failed_discovery_cannot_satisfy_power_off_interval() {
        let mut journey = model(Role::Left);
        let now = Instant::now();
        journey.observe_at(Ok(Snapshot::default()), now);
        journey.observe_at(Err("unavailable".into()), now + Duration::from_secs(5));
        assert_eq!(*journey.machine.state(), Phase::Failed);
        assert_eq!(journey.machine.inner().retry_phase, Some(Phase::Disconnect));
        assert!(
            journey
                .view()
                .error
                .as_deref()
                .unwrap()
                .contains("unavailable")
        );
        journey.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(20));
        assert!(matches!(*journey.machine.state(), Phase::OffWait(_)));
    }
    #[test]
    fn factory_plan_keeps_left_until_dongle_entry_is_finished() {
        assert_eq!(PLAN, [Role::Receiver, Role::Right, Role::Left]);
    }
    #[test]
    fn interrupted_transfer_requires_exact_original_backup_and_cannot_be_blindly_retried() {
        let root = std::env::temp_dir().join(format!(
            "nocfree-install-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let prior = root.join("readback-prior");
        let current = root.join("readback-current");
        fs::create_dir(&prior).unwrap();
        fs::create_dir(&current).unwrap();
        let release = crate::release::fixture();
        let image = release.image(Role::Left);
        let baseline = device::tests::archive();
        fs::write(prior.join("CURRENT.UF2"), &baseline).unwrap();
        let intent = serde_json::json!({"schema":1,"role":"Left","location":10,"target_sha256":image.metadata.uf2_sha256,"backup_sha256":hash(&baseline)});
        durable(
            &prior,
            "install-intent.json",
            &serde_json::to_vec(&intent).unwrap(),
        )
        .unwrap();
        // No exact target yet: the saved one-shot intent remains unresolved.
        assert!(reconcile_prior(&current, Role::Left, 10, &baseline, image).is_err());
        assert!(!prior.join("install-verified.json").exists());
        let actual = archive_with(image);
        assert!(reconcile_prior(&current, Role::Left, 11, &actual, image).is_err());
        fs::write(prior.join("CURRENT.UF2"), &actual).unwrap();
        assert!(
            reconcile_prior(&current, Role::Left, 10, &actual, image)
                .unwrap_err()
                .contains("backup changed")
        );
        fs::write(prior.join("CURRENT.UF2"), &baseline).unwrap();
        reconcile_prior(&current, Role::Left, 10, &actual, image).unwrap();
        assert!(prior.join("install-verified.json").exists());
        fs::write(prior.join("install-verified.json"), b"{partial").unwrap();
        assert!(reconcile_prior(&current, Role::Left, 10, &actual, image).is_err());
        assert!(durable(&prior, "install-intent.json", b"retry").is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn discovery_failure_is_visible_during_recovery_and_verification_without_retrying_copy() {
        let mut journey = FirmwareJourney::new(crate::release::fixture());
        journey.observe(Err("Recovery drive access was denied.".into()));
        assert_eq!(*journey.machine.state(), Phase::Failed);
        assert!(journey.view().error.unwrap().contains("denied"));
        assert!(!journey.view().can_transfer);
        journey.observe(Ok(Snapshot::default()));
        assert_eq!(*journey.machine.state(), Phase::Recovery);
        assert!(journey.view().needs_recovery);

        let mut journey = model(Role::Left);
        unsafe {
            *journey.machine.state_mut() = Phase::Reconcile;
        }
        unsafe {
            journey.machine.inner_mut().attempted = true;
        }
        let original_bytes = journey
            .machine
            .inner()
            .baseline
            .as_ref()
            .unwrap()
            .bytes
            .clone();
        let original_mount = journey
            .machine
            .inner()
            .baseline
            .as_ref()
            .unwrap()
            .mount
            .clone();
        let original_folder = journey
            .machine
            .inner()
            .baseline
            .as_ref()
            .unwrap()
            .folder
            .clone();
        journey.observe(Err("USB inventory unavailable.".into()));
        assert_eq!(*journey.machine.state(), Phase::Failed);
        assert_eq!(journey.machine.inner().retry_phase, Some(Phase::Reconcile));
        assert!(journey.transfer().is_err());
        journey.observe(Err("Recovery drive access was denied.".into()));
        assert_eq!(journey.machine.inner().retry_phase, Some(Phase::Reconcile));
        journey.observe(Ok(normal(Role::Left, 10)));
        assert_eq!(*journey.machine.state(), Phase::Reconcile);
        assert!(journey.machine.inner().attempted);
        assert!(journey.transfer().is_err());
        assert!(journey.view().verification);
        assert_eq!(
            journey.machine.inner().baseline.as_ref().unwrap().bytes,
            original_bytes
        );
        assert_eq!(
            journey.machine.inner().baseline.as_ref().unwrap().mount,
            original_mount
        );
        assert_eq!(
            journey.machine.inner().baseline.as_ref().unwrap().folder,
            original_folder
        );
    }

    #[test]
    fn returning_discovery_failure_requires_new_verified_off_interval() {
        let now = Instant::now();
        for phase in [
            Phase::Disconnect,
            Phase::OffWait(now),
            Phase::PowerOn,
            Phase::StartWait(now),
            Phase::Reconnect,
        ] {
            let mut journey = model(Role::Right);
            unsafe {
                journey.machine.inner_mut().attempted = true;
            }
            unsafe {
                *journey.machine.state_mut() = phase;
            }
            journey.observe_at(Err("USB inventory unavailable.".into()), now);
            assert_eq!(*journey.machine.state(), Phase::Failed);
            assert_eq!(journey.machine.inner().retry_phase, Some(Phase::Disconnect));
            // An already attached board cannot bypass the restart dance.
            journey.observe_at(Ok(normal(Role::Right, 10)), now + Duration::from_secs(60));
            assert_eq!(*journey.machine.state(), Phase::Disconnect);
            assert_eq!(journey.role(), Role::Right);
            assert!(journey.machine.inner().attempted);
            journey.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(61));
            assert_eq!(
                *journey.machine.state(),
                Phase::OffWait(now + Duration::from_secs(61))
            );
            assert!(journey.transfer().is_err());
        }
    }
}

//! A guarded, sequential application update. Recovery acquisition belongs to the
//! shared recovery worker; a copy is attempted once and reconciled by readback.
use crate::{
    device::{self, BootMount, Snapshot},
    factory_release::{FactoryImage, FactoryRelease},
    release::{FirmwareRelease, ReleaseImage},
    runtime_recovery::Role,
    scope::Scope,
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
#[derive(Clone)]
enum TargetRelease {
    Rmk(FirmwareRelease),
    Factory(FactoryRelease),
}
#[derive(Clone)]
enum TargetImage {
    Rmk(Box<ReleaseImage>),
    Factory(FactoryImage),
}
impl TargetRelease {
    fn plan(&self) -> [Role; 3] {
        match self {
            Self::Rmk(_) => PLAN,
            Self::Factory(_) => [Role::Left, Role::Right, Role::Receiver],
        }
    }
    fn id(&self) -> String {
        match self {
            Self::Rmk(r) => r.id().into(),
            Self::Factory(r) => r.id(),
        }
    }
    fn image(&self, role: Role) -> Result<TargetImage, String> {
        match self {
            Self::Rmk(r) => Ok(TargetImage::Rmk(Box::new(r.image(role).clone()))),
            Self::Factory(r) => Ok(TargetImage::Factory(r.image(role)?.clone())),
        }
    }
    fn factory(&self) -> bool {
        matches!(self, Self::Factory(_))
    }
}
fn scoped_plan(release: &TargetRelease, scope: Scope) -> Vec<Role> {
    release
        .plan()
        .into_iter()
        .filter(|role| scope.roles().contains(role))
        .collect()
}
impl TargetImage {
    fn sha(&self) -> &str {
        match self {
            Self::Rmk(i) => &i.metadata.uf2_sha256,
            Self::Factory(i) => &i.sha256,
        }
    }
    fn uf2(&self) -> &[u8] {
        match self {
            Self::Rmk(i) => &i.uf2,
            Self::Factory(i) => &i.uf2,
        }
    }
    fn target(&self) -> Result<(usize, Vec<u8>), String> {
        match self {
            Self::Rmk(i) => target(i),
            Self::Factory(i) => {
                i.checked()?;
                Ok((0, i.bytes.clone()))
            }
        }
    }
    fn exact(&self, archive: &[u8]) -> Result<bool, String> {
        match self {
            Self::Rmk(i) => exact_candidate(archive, i),
            Self::Factory(_) => {
                let (start, p) = self.target()?;
                let actual = flash_bytes(archive)?;
                Ok(actual[start..start + p.len()] == p)
            }
        }
    }
    fn verify(&self, archive: &[u8], baseline: &[u8]) -> Result<usize, String> {
        match self {
            Self::Rmk(i) => verify(archive, baseline, i),
            Self::Factory(_) => {
                if !self.exact(archive)? {
                    return Err("Factory readback differs from the complete planned restore, including S140 and saved settings.".into());
                }
                let old = flash_bytes(baseline)?;
                let actual = flash_bytes(archive)?;
                Ok(actual[0x64000..]
                    .iter()
                    .zip(&old[0x64000..])
                    .filter(|(a, b)| a != b)
                    .count())
            }
        }
    }
}
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
    pub needs_wired_ack: bool,
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
    release: TargetRelease,
    plan: Vec<Role>,
    index: usize,
    baseline: Option<Baseline>,
    error: Option<String>,
    attempted: bool,
    already_current: bool,
    install_authorized: bool,
    verified_locations: Vec<(Role, u64)>,
    // This is failure history, not a second mutable current state.
    retry_phase: Option<Phase>,
    wired_ack: bool,
    wired_ack_available: bool,
    latest: Option<(Snapshot, Instant, Option<crate::device_status::Mode>)>,
    absent_since: Option<Instant>,
}
impl FirmwareData {
    fn role(&self) -> Role {
        self.plan[self.index]
    }
}

enum FirmwareEvent<'a> {
    AuthorizeInstall,
    RecoverySaved(&'a Baseline, bool),
    TransferAttempted,
    TransferResult(&'a Result<(), String>),
    Verified,
    PowerOn(Instant),
    Observe(
        &'a Result<Snapshot, String>,
        Instant,
        Option<crate::device_status::Mode>,
    ),
    ConfirmWired,
    Advance(
        &'a Result<Snapshot, String>,
        Instant,
        Option<crate::device_status::Mode>,
    ),
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
            FirmwareEvent::ConfirmWired
                if data.wired_ack_available
                    && data.release.factory()
                    && data.role() == Role::Left
                    && matches!(
                        self,
                        Phase::Disconnect | Phase::OffWait(_) | Phase::Reconnect
                    ) =>
            {
                data.wired_ack = true;
                return Handled;
            }
            FirmwareEvent::AuthorizeInstall => {
                if !matches!(self, Phase::Cancelled | Phase::Complete) {
                    data.install_authorized = true;
                }
                return Handled;
            }
            FirmwareEvent::Cancel => {
                data.retry_phase = None;
                return Transition(Phase::Cancelled);
            }
            FirmwareEvent::RecoverySaved(baseline, installed) if *self == Phase::Recovery => {
                data.baseline = Some((*baseline).clone());
                data.latest = None;
                data.absent_since = None;
                data.error = None;
                data.already_current = *installed;
                if *installed {
                    data.verified_locations
                        .push((data.role(), baseline.location));
                }
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
                data.latest = None;
                data.absent_since = None;
                if let Some(baseline) = &data.baseline {
                    data.verified_locations
                        .push((data.role(), baseline.location));
                }
                data.error = None;
                return Transition(Phase::Disconnect);
            }
            FirmwareEvent::PowerOn(now) if *self == Phase::PowerOn => {
                return Transition(Phase::StartWait(*now));
            }
            _ => {}
        }
        let (FirmwareEvent::Observe(observation, now, mode)
        | FirmwareEvent::Advance(observation, now, mode)) = event
        else {
            return Handled;
        };
        if matches!(self, Phase::Complete | Phase::Cancelled) {
            return Handled;
        }
        if matches!(event, FirmwareEvent::Observe(..)) {
            data.latest = observation
                .as_ref()
                .ok()
                .map(|snapshot| (snapshot.clone(), *now, *mode));
        }
        let (snapshot, phase) = match observation {
            Err(error) => {
                data.wired_ack = false;
                data.wired_ack_available = false;
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
                    if matches!(event, FirmwareEvent::Advance(..)) {
                        data.error = None;
                        data.retry_phase.take().unwrap_or(Phase::Recovery)
                    } else {
                        data.retry_phase.unwrap_or(Phase::Recovery)
                    }
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
        if matches!(event, FirmwareEvent::Observe(..)) {
            if connected {
                data.absent_since = None;
            } else {
                data.absent_since.get_or_insert(*now);
            }
        }
        // Verification already proved the installed image. A fresh normal
        // descriptor on its bound port proves it returned; an unplug can occur
        // entirely between discovery polls, so do not require observing absence.
        let returned = matches!(
            phase,
            Phase::Disconnect
                | Phase::OffWait(_)
                | Phase::PowerOn
                | Phase::StartWait(_)
                | Phase::Reconnect
        ) && normal_return(
            snapshot,
            data.role(),
            baseline.location,
            data.release.factory(),
        );
        data.wired_ack_available = returned && data.role() == Role::Left && data.release.factory();
        if !returned {
            data.wired_ack = false;
        }
        let return_allowed = data.role() != Role::Left
            || if data.release.factory() {
                data.wired_ack
            } else {
                *mode == Some(crate::device_status::Mode::Wired)
            };
        if matches!(event, FirmwareEvent::Observe(..))
            && matches!(
                phase,
                Phase::Disconnect
                    | Phase::OffWait(_)
                    | Phase::PowerOn
                    | Phase::StartWait(_)
                    | Phase::Reconnect
            )
        {
            return Handled;
        }
        if returned && return_allowed {
            return if data.index == data.plan.len() - 1 {
                Transition(Phase::Complete)
            } else {
                data.index += 1;
                data.baseline = None;
                data.attempted = false;
                data.already_current = false;
                data.wired_ack = false;
                data.wired_ack_available = false;
                data.latest = None;
                data.absent_since = None;
                Transition(Phase::Recovery)
            };
        }
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
            Phase::Reconnect if connected && !returned => Phase::Disconnect,
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
fn normal_return(snapshot: &Snapshot, role: Role, location: u64, factory: bool) -> bool {
    let mut normal = snapshot.devices.iter().filter(|d| {
        if factory {
            d.location == location
                && match role {
                    Role::Right => d.factory_right(),
                    Role::Left | Role::Receiver => d.factory_left(),
                }
        } else {
            d.vendor == 0x4c4b && d.product == role.product() as u64 && d.name == role.name()
        }
    });
    normal.next().is_some_and(|d| d.location == location)
        && normal.next().is_none()
        && snapshot.mounts.is_empty()
        && !snapshot.devices.iter().any(|d| d.bootloader())
}
fn rmk_intent(record: &serde_json::Value) -> bool {
    record["kind"].as_str() == Some("RMK")
        || record["release"]
            .as_str()
            .is_some_and(|r| r.starts_with("nocfree-"))
}
fn sibling_folder(root: &Path, value: &serde_json::Value) -> Result<PathBuf, String> {
    let p = PathBuf::from(
        value
            .as_str()
            .ok_or("Factory verification path is missing.")?,
    );
    if p.parent() != Some(root)
        || !fs::symlink_metadata(&p)
            .map_err(|_| "Factory verification folder is unavailable.")?
            .is_dir()
        || fs::symlink_metadata(&p)
            .map_err(|_| "Factory verification folder is unavailable.")?
            .file_type()
            .is_symlink()
    {
        return Err("Factory verification must remain in the private backup folder.".into());
    }
    Ok(p)
}
fn superseded(prior: &Path, raw_intent: &[u8], role: Role) -> Result<bool, String> {
    let marker_path = prior.join("install-superseded.json");
    if !marker_path.exists() {
        return Ok(false);
    }
    let root = prior.parent().ok_or("Backup folder is unavailable.")?;
    let marker: serde_json::Value =
        serde_json::from_slice(&device::read_bounded(&marker_path, 8192)?)
            .map_err(|_| "Factory supersession record is unreadable.")?;
    let old: serde_json::Value = serde_json::from_slice(raw_intent)
        .map_err(|_| "Original operation record is unreadable.")?;
    if marker["schema"].as_u64() != Some(1)
        || !rmk_intent(&old)
        || marker["old_intent_sha256"].as_str() != Some(hash(raw_intent).as_str())
        || marker["role"] != old["role"]
        || marker["location"] != old["location"]
    {
        return Err("Factory supersession does not bind the original RMK attempt.".into());
    }
    let folder = sibling_folder(root, &marker["factory_verification"])?;
    let proof: serde_json::Value = serde_json::from_slice(&device::read_bounded(
        &folder.join("factory-verified.json"),
        8192,
    )?)
    .map_err(|_| "Factory verification record is unreadable.")?;
    if proof["schema"].as_u64() != Some(1)
        || proof["role"] != marker["role"]
        || proof["location"] != marker["location"]
        || proof["target_sha256"] != marker["factory_target_sha256"]
        || proof["readback_sha256"] != marker["factory_readback_sha256"]
    {
        return Err("Factory verification does not bind the supersession record.".into());
    }
    let readback = sibling_folder(root, &proof["readback"])?;
    let actual = device::read_bounded(&readback.join("CURRENT.UF2"), ARCHIVE_LIMIT)?;
    let image = FactoryRelease::verified_archive_image(role, &actual)?;
    if proof["readback_sha256"].as_str() != Some(hash(&actual).as_str())
        || proof["target_sha256"].as_str() != Some(image.sha256.as_str())
    {
        return Err(
            "Verified factory readback changed; the old RMK attempt remains uncertain.".into(),
        );
    }
    Ok(true)
}
fn record_factory_supersession(
    folder: &Path,
    readback: &Path,
    role: Role,
    location: u64,
    actual: &[u8],
    image: &TargetImage,
) -> Result<(), String> {
    if !matches!(image, TargetImage::Factory(_)) || !image.exact(actual)? {
        return Err("Exact complete factory verification is required before supersession.".into());
    }
    let root = folder.parent().ok_or("Backup folder is unavailable.")?;
    let proof = serde_json::json!({"schema":1,"role":format!("{role:?}"),"location":location,"target_sha256":image.sha(),"readback_sha256":hash(actual),"readback":readback,"classification":"factory readback exact; preceding RMK result remains uncertain"});
    let proof_path = folder.join("factory-verified.json");
    if proof_path.exists() {
        let old: serde_json::Value =
            serde_json::from_slice(&device::read_bounded(&proof_path, 8192)?)
                .map_err(|_| "Factory verification record is unreadable.")?;
        if old != proof {
            return Err("Factory verification record differs from this observation.".into());
        }
    } else {
        durable(
            folder,
            "factory-verified.json",
            &serde_json::to_vec_pretty(&proof)
                .map_err(|_| "Could not encode factory verification.")?,
        )?;
    }
    for entry in fs::read_dir(root).map_err(|_| "Could not check earlier RMK attempts.")? {
        let entry = entry.map_err(|_| "Could not read earlier RMK attempt.")?;
        if !entry
            .file_type()
            .map_err(|_| "Could not inspect earlier RMK attempt.")?
            .is_dir()
            || entry.path() == folder
        {
            continue;
        }
        let prior = entry.path();
        let intent = prior.join("install-intent.json");
        if !intent.exists() || prior.join("install-verified.json").exists() {
            continue;
        }
        let raw = device::read_bounded(&intent, 8192)?;
        let old: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|_| "Earlier RMK intent is unreadable.")?;
        if old["schema"].as_u64() != Some(1)
            || !rmk_intent(&old)
            || old["role"].as_str() != Some(format!("{role:?}").as_str())
            || old["location"].as_u64() != Some(location)
        {
            continue;
        }
        if superseded(&prior, &raw, role)? {
            continue;
        }
        let marker = serde_json::json!({"schema":1,"old_intent_sha256":hash(&raw),"role":old["role"],"location":old["location"],"factory_verification":folder,"factory_target_sha256":image.sha(),"factory_readback_sha256":hash(actual)});
        durable(
            &prior,
            "install-superseded.json",
            &serde_json::to_vec_pretty(&marker).map_err(|_| "Could not encode supersession.")?,
        )?;
    }
    Ok(())
}
fn already_factory(role: Role, archive: &[u8], stock_origin: Option<Role>) -> Result<bool, String> {
    device::inspect_archive(archive)?;
    let actual = FactoryRelease::archive_role(archive);
    if actual.is_some_and(|r| r != role) {
        return Err("The recovery drive belongs to another part. Connect the selected part before continuing.".into());
    }
    // This is only a no-write decision. Capturing an unknown factory original
    // separately requires the correlated factory session and a complete archive.
    Ok(stock_origin == Some(role) || actual == Some(role))
}
// An app restart does not erase uncertainty. A pending journal for this part
// must reconcile against its original backup and the same pinned target.
fn reconcile_target(
    folder: &Path,
    role: Role,
    location: u64,
    actual: &[u8],
    image: &TargetImage,
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
        if matches!(image, TargetImage::Factory(_)) && rmk_intent(&record) {
            // Explicit undo uses a new fresh full backup and its own one-shot
            // intent. Keep the old RMK attempt uncertain; never replay or claim
            // it verified merely because a different restore was requested.
            image.target()?;
            flash_bytes(actual)?;
            continue;
        }
        if superseded(
            &prior,
            &device::read_bounded(&prior.join("install-intent.json"), 8192)?,
            role,
        )? {
            continue;
        }
        if record["location"].as_u64() != Some(location)
            || record["target_sha256"].as_str() != Some(image.sha())
        {
            return Err("An earlier installation needs reconciliation on its original USB connection and release.".into());
        }
        let previous = device::read_bounded(&prior.join("CURRENT.UF2"), ARCHIVE_LIMIT)?;
        if record["backup_sha256"].as_str() != Some(hash(&previous).as_str()) {
            return Err("The earlier installation backup changed.".into());
        }
        let storage = image.verify(actual, &previous)?;
        durable(&prior,"install-verified.json",&serde_json::to_vec_pretty(&serde_json::json!({"schema":1,"readback_sha256":hash(actual),"settings_changed_bytes":storage,"reconciled_after_restart":true})).map_err(|_| "Could not encode reconciliation.")?)?;
    }
    Ok(())
}
impl FirmwareJourney {
    pub fn new(release: FirmwareRelease) -> Self {
        Self::new_scoped(release, Scope::Whole)
    }
    pub fn new_scoped(release: FirmwareRelease, scope: Scope) -> Self {
        let plan = scoped_plan(&TargetRelease::Rmk(release.clone()), scope);
        Self {
            machine: FirmwareData {
                release: TargetRelease::Rmk(release),
                plan,
                index: 0,
                baseline: None,
                error: None,
                attempted: false,
                already_current: false,
                install_authorized: false,
                verified_locations: vec![],
                retry_phase: None,
                wired_ack: false,
                wired_ack_available: false,
                latest: None,
                absent_since: None,
            }
            .state_machine(),
        }
    }
    pub fn factory(release: FactoryRelease) -> Result<Self, String> {
        Self::factory_scoped(release, Scope::Whole)
    }
    pub fn factory_scoped(release: FactoryRelease, scope: Scope) -> Result<Self, String> {
        let plan = scoped_plan(&TargetRelease::Factory(release.clone()), scope);
        if !plan.iter().all(|role| release.has(*role)) {
            return Err("Add factory firmware for the selected parts first.".into());
        }
        Ok(Self {
            machine: FirmwareData {
                release: TargetRelease::Factory(release),
                plan,
                index: 0,
                baseline: None,
                error: None,
                attempted: false,
                already_current: false,
                install_authorized: false,
                verified_locations: vec![],
                retry_phase: None,
                wired_ack: false,
                wired_ack_available: false,
                latest: None,
                absent_since: None,
            }
            .state_machine(),
        })
    }
    pub fn plan(&self) -> &[Role] {
        &self.machine.inner().plan
    }
    pub fn is_factory(&self) -> bool {
        self.machine.inner().release.factory()
    }
    pub fn verified_locations(&self) -> Vec<(Role, u64)> {
        self.machine.inner().verified_locations.clone()
    }
    /// Overall install approval authorizes subsequent guarded per-part copies.
    /// Readback, fresh backup, bound port and durable one-shot intent still apply.
    pub fn authorize_install(&mut self) {
        self.machine.handle(&FirmwareEvent::AuthorizeInstall);
    }
    pub fn transfer_if_ready(&mut self) -> Result<bool, String> {
        if !self.machine.inner().install_authorized || !self.view().can_transfer {
            return Ok(false);
        }
        self.transfer()?;
        Ok(true)
    }
    pub fn role(&self) -> Role {
        self.machine.inner().role()
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
                "Start the right half",
                "If the recovery drive is open, turn right OFF and unplug USB. Keep it OFF until prompted to turn it ON.",
            ),
            Phase::Disconnect if self.role() == Role::Left => (
                "Start the left half",
                "Move left to middle WIRED. If the recovery drive is open, unplug USB; otherwise keep it connected.",
            ),
            Phase::Disconnect => (
                "Start the dongle",
                "If the recovery drive is open, unplug the dongle for five seconds. Otherwise keep it connected.",
            ),
            Phase::OffWait(_) if self.role() == Role::Right => (
                "Keep the right half OFF",
                "Keep the right switch OFF and USB unplugged for five seconds. We’ll prompt you to turn it ON next.",
            ),
            Phase::OffWait(_) => ("Keep it unplugged", "Keep USB unplugged for five seconds."),
            Phase::PowerOn => (
                "Turn the right half ON",
                "Keep USB unplugged. Turn it ON, then continue.",
            ),
            Phase::StartWait(_) => ("Let it start", "Keep USB unplugged for ten seconds."),
            Phase::Reconnect => ("Reconnect USB", "Reconnect it to the same USB port."),
            Phase::Complete if self.is_factory() => (
                "Factory firmware restored",
                "All three parts are running factory firmware.",
            ),
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
            title: if self.machine.inner().already_current
                && matches!(
                    self.machine.state(),
                    Phase::Disconnect
                        | Phase::OffWait(_)
                        | Phase::PowerOn
                        | Phase::StartWait(_)
                        | Phase::Reconnect
                ) {
                if self.is_factory() {
                    "Already on factory firmware"
                } else {
                    "Already latest version"
                }
                .into()
            } else {
                title.into()
            },
            instruction: instruction.into(),
            role: self.role(),
            step: self.machine.inner().index,
            needs_wired_ack: self.machine.inner().wired_ack_available
                && self.is_factory()
                && self.role() == Role::Left
                && !self.machine.inner().wired_ack
                && matches!(
                    *self.machine.state(),
                    Phase::Disconnect | Phase::OffWait(_) | Phase::Reconnect
                ),
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
        if self.is_factory() && !self.machine.inner().install_authorized {
            return Err("Choose Restore factory before starting its guided steps.".into());
        }
        if role != self.role() {
            return Err("Recovery belongs to a different component.".into());
        }
        let stock_origin = session.factory_recovery_role();
        let folder = session.save_backup()?;
        let bytes = device::read_bounded(&folder.join("CURRENT.UF2"), ARCHIVE_LIMIT)?;
        device::inspect_archive(&bytes)?;
        let stock_noop = self.is_factory() && already_factory(role, &bytes, stock_origin)?;
        if FactoryRelease::archive_role(&bytes).is_some_and(|actual| actual != role) {
            return Err("The recovery drive contains another part's factory firmware. Connect the selected part before continuing.".into());
        }
        if !stock_noop {
            reconcile_target(
                &folder,
                role,
                location,
                &bytes,
                &self.machine.inner().release.image(role)?,
            )?;
        }
        let baseline = Baseline {
            location,
            mount,
            folder,
            bytes,
        };
        if !self.is_factory() {
            let mut originals = FactoryRelease::discover()?;
            if stock_origin == Some(role) {
                originals.capture_original(role, &baseline.bytes)?;
            } else {
                originals.retain_original(role, &baseline.bytes)?;
            }
        }
        let exact_target = self
            .machine
            .inner()
            .release
            .image(role)?
            .exact(&baseline.bytes)?;
        let installed = stock_noop || exact_target;
        if exact_target && self.is_factory() {
            record_factory_supersession(
                &baseline.folder,
                &baseline.folder,
                role,
                location,
                &baseline.bytes,
                &self.machine.inner().release.image(role)?,
            )?;
        }
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
        let image = self.machine.inner().release.image(self.role())?;
        image.target()?;
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
        let intent = serde_json::json!({"schema":1,"release":self.machine.inner().release.id(),"kind":if self.is_factory(){"factory"}else{"RMK"},"role":format!("{:?}",self.role()),"backup_sha256":hash(&fresh),"target_sha256":image.sha(),"location":baseline.location,"attempt":"one-shot; reconcile readback before continuing"});
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
            let written = destination.write_all(image.uf2());
            let flushed = written.as_ref().ok().map(|_| destination.sync_all());
            record_transfer_outcome(&baseline.folder, written, flushed)
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
        if role != self.role()
            || location != baseline.location
            || !same_recovery_identity(&mount.info, &baseline.mount.info)
        {
            return Err("Verification belongs to a different component or USB connection.".into());
        }
        let folder = session.save_backup()?;
        let actual = device::read_bounded(&folder.join("CURRENT.UF2"), ARCHIVE_LIMIT)?;
        let changed_storage = self
            .machine
            .inner()
            .release
            .image(role)?
            .verify(&actual, &baseline.bytes)?;
        durable(&baseline.folder,"install-verified.json",&serde_json::to_vec_pretty(&serde_json::json!({"schema":1,"readback_sha256":hash(&actual),"settings_changed_bytes":changed_storage,"settings_range":"0x65000..0x6d000","settings_action":if self.is_factory(){"restore original factory settings exactly"}else{"RMK schema migration permitted"},"readback":folder})).map_err(|_| "Could not encode verification record.")?)?;
        if self.is_factory() {
            record_factory_supersession(
                &baseline.folder,
                &folder,
                role,
                location,
                &actual,
                &self.machine.inner().release.image(role)?,
            )?;
        }
        self.machine.handle(&FirmwareEvent::Verified);
        Ok(())
    }
    pub fn can_next(&self) -> bool {
        self.can_next_at(Instant::now())
    }
    fn can_next_at(&self, now: Instant) -> bool {
        let Some((snapshot, seen, mode)) = &self.machine.inner().latest else {
            return false;
        };
        if now.saturating_duration_since(*seen) > Duration::from_secs(5) {
            return false;
        }
        let Some(baseline) = &self.machine.inner().baseline else {
            return false;
        };
        let normal = normal_return(snapshot, self.role(), baseline.location, self.is_factory());
        let returned = normal
            && (self.role() != Role::Left
                || self.is_factory()
                || *mode == Some(crate::device_status::Mode::Wired));
        let connected = snapshot
            .devices
            .iter()
            .any(|d| d.location == baseline.location);
        let phase = if *self.machine.state() == Phase::Failed {
            self.machine.inner().retry_phase.unwrap_or(Phase::Recovery)
        } else {
            *self.machine.state()
        };
        match phase {
            Phase::Disconnect => returned || !connected,
            Phase::OffWait(since) => {
                returned
                    || connected
                    || self.machine.inner().absent_since.is_some_and(|absent| {
                        now.saturating_duration_since(absent.max(since)) >= Duration::from_secs(5)
                    })
            }
            Phase::PowerOn => returned || !connected,
            Phase::StartWait(since) => {
                returned
                    || connected
                    || self.machine.inner().absent_since.is_some_and(|absent| {
                        now.saturating_duration_since(absent.max(since)) >= Duration::from_secs(10)
                    })
            }
            Phase::Reconnect => returned || connected && !normal,
            _ => false,
        }
    }
    #[allow(clippy::should_implement_trait)] // Explicit journey button, not an iterator.
    pub fn next(&mut self) -> bool {
        self.next_at(Instant::now())
    }
    fn next_at(&mut self, now: Instant) -> bool {
        if !self.can_next_at(now) {
            return false;
        }
        let (snapshot, _, mode) = self.machine.inner().latest.clone().unwrap();
        if self.view().needs_wired_ack {
            self.confirm_wired();
        }
        if *self.machine.state() == Phase::PowerOn
            && !snapshot.devices.iter().any(|d| {
                self.machine
                    .inner()
                    .baseline
                    .as_ref()
                    .is_some_and(|b| d.location == b.location)
            })
        {
            self.machine.handle(&FirmwareEvent::PowerOn(now));
        } else {
            self.machine
                .handle(&FirmwareEvent::Advance(&Ok(snapshot), now, mode));
        }
        true
    }
    pub fn confirm_wired(&mut self) {
        self.machine.handle(&FirmwareEvent::ConfirmWired);
    }
    pub fn confirm_power_on(&mut self) {
        self.machine.handle(&FirmwareEvent::PowerOn(Instant::now()));
    }
    pub fn observe(&mut self, observation: Result<Snapshot, String>) {
        self.observe_at(observation, Instant::now());
    }
    pub(crate) fn observe_with_mode(
        &mut self,
        observation: Result<Snapshot, String>,
        mode: Option<crate::device_status::Mode>,
    ) {
        self.observe_mode_at(observation, mode, Instant::now());
    }
    fn observe_at(&mut self, observation: Result<Snapshot, String>, now: Instant) {
        self.observe_mode_at(observation, None, now);
    }
    fn observe_mode_at(
        &mut self,
        observation: Result<Snapshot, String>,
        mode: Option<crate::device_status::Mode>,
        now: Instant,
    ) {
        self.machine
            .handle(&FirmwareEvent::Observe(&observation, now, mode));
    }
    pub fn cancel(&mut self) {
        self.machine.handle(&FirmwareEvent::Cancel);
    }
}

fn record_transfer_outcome(
    folder: &Path,
    written: std::io::Result<()>,
    flushed: Option<std::io::Result<()>>,
) -> Result<(), String> {
    let error = |result: &std::io::Result<()>| {
        result.as_ref().err().map(|error| {
        serde_json::json!({"kind":format!("{:?}", error.kind()),"os_code":error.raw_os_error(),"message":error.to_string()})
    })
    };
    let report = serde_json::json!({
        "schema":1,
        "write_complete":written.is_ok(),
        "write_error":error(&written),
        "flush_error":flushed.as_ref().and_then(error),
        "status":"readback verification required; not installation success",
    });
    durable(
        folder,
        "transfer-outcome.json",
        &serde_json::to_vec_pretty(&report).map_err(
            |_| "Could not record the transfer result. Verify its readback before continuing.",
        )?,
    )?;
    written.map_err(|_| "The firmware write was interrupted. Click Next to check its readback before continuing.".to_owned())
    // A UF2 target can reset before fsync acknowledges. Even a successful
    // flush cannot prove installation: both outcomes remain in Reconcile and
    // require exact readback. Never retry the copy here.
}

#[cfg(test)]
mod transfer_outcome_tests {
    use super::*;
    #[test]
    fn flush_failure_proceeds_only_to_readback_but_write_failure_stops() {
        let root =
            std::env::temp_dir().join(format!("nocfree-transfer-outcome-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        assert!(
            record_transfer_outcome(
                &root,
                Ok(()),
                Some(Err(std::io::Error::from_raw_os_error(22)))
            )
            .is_ok()
        );
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("transfer-outcome.json")).unwrap()).unwrap();
        assert_eq!(report["write_complete"], true);
        assert_eq!(report["flush_error"]["os_code"], 22);
        let failed = root.join("failed-write");
        fs::create_dir(&failed).unwrap();
        assert!(
            record_transfer_outcome(
                &failed,
                Err(std::io::Error::from(std::io::ErrorKind::WriteZero)),
                None
            )
            .is_err()
        );
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(failed.join("transfer-outcome.json")).unwrap())
                .unwrap();
        assert_eq!(report["write_complete"], false);
        assert!(report["flush_error"].is_null());
        fs::remove_dir_all(root).unwrap();
    }
}

// Restoring factory bytes can restore S140 too. INFO_UF2 reports its presence,
// so that one field is firmware state rather than bootloader/device identity.
// Preserve every other metadata line; image verification still checks S140 bytes.
fn same_recovery_identity(actual: &str, saved: &str) -> bool {
    if actual == saved {
        return true;
    }
    let softdevice = |line: &&str| line.starts_with("SoftDevice:");
    actual.lines().filter(softdevice).count() == 1
        && saved.lines().filter(softdevice).count() == 1
        && actual
            .lines()
            .filter(|line| !line.starts_with("SoftDevice:"))
            .eq(saved
                .lines()
                .filter(|line| !line.starts_with("SoftDevice:")))
}

#[cfg(test)]
mod recovery_identity_tests {
    use super::same_recovery_identity;
    #[test]
    fn factory_softdevice_restoration_keeps_identity_but_other_changes_do_not() {
        let saved = "UF2 Bootloader 0.9.2\nModel: NocFree &\nBoard-ID: NocFree &\nDate: Dec 26 2025\nSoftDevice: not found\n";
        let restored = saved.replace("not found", "S140 7.3.0");
        assert!(same_recovery_identity(&restored, saved));
        assert!(!same_recovery_identity(
            &restored.replace("0.9.2", "0.9.3"),
            saved
        ));
        assert!(!same_recovery_identity(
            &restored.replace("Board-ID: NocFree &", "Board-ID: different"),
            saved
        ));
        assert!(!same_recovery_identity(
            &restored.replace("SoftDevice: S140 7.3.0\n", ""),
            saved
        ));
        assert!(!same_recovery_identity(
            &(restored + "SoftDevice: not found\n"),
            saved
        ));
    }
}

#[cfg(test)]
fn reconcile_prior(
    folder: &Path,
    role: Role,
    location: u64,
    actual: &[u8],
    image: &ReleaseImage,
) -> Result<(), String> {
    reconcile_target(
        folder,
        role,
        location,
        actual,
        &TargetImage::Rmk(Box::new(image.clone())),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::Device;
    impl FirmwareJourney {
        fn observe_advance_at(&mut self, snapshot: Result<Snapshot, String>, now: Instant) {
            self.observe_at(snapshot, now);
            self.next_at(now);
        }
    }
    fn settle(journey: &mut FirmwareJourney, snapshot: Result<Snapshot, String>) {
        let now = Instant::now();
        journey.observe_mode_at(
            snapshot.clone(),
            Some(crate::device_status::Mode::Wired),
            now,
        );
        assert!(!journey.view().complete);
        journey.observe_mode_at(
            snapshot,
            Some(crate::device_status::Mode::Wired),
            now + Duration::from_secs(5),
        );
        journey.next_at(now + Duration::from_secs(5));
    }
    #[test]
    fn left_return_requires_live_wired_and_explicit_next() {
        use crate::device_status::Mode;
        let mut journey = model(Role::Left);
        let now = Instant::now();
        for mode in [None, Some(Mode::Dongle), Some(Mode::Bluetooth)] {
            journey.observe_mode_at(Ok(normal(Role::Left, 10)), mode, now);
            assert!(!journey.can_next_at(now));
            assert!(!journey.next_at(now));
            assert!(!journey.view().complete);
        }
        journey.observe_mode_at(Ok(normal(Role::Left, 10)), Some(Mode::Wired), now);
        assert!(journey.can_next_at(now));
        assert!(!journey.view().complete);
        journey.observe_mode_at(Ok(normal(Role::Left, 10)), Some(Mode::Dongle), now);
        assert!(!journey.next_at(now));
        journey.observe_mode_at(Ok(normal(Role::Left, 10)), Some(Mode::Wired), now);
        assert!(!journey.next_at(now + Duration::from_secs(6)));
        journey.observe_mode_at(
            Ok(normal(Role::Left, 10)),
            Some(Mode::Wired),
            now + Duration::from_secs(7),
        );
        assert!(journey.next_at(now + Duration::from_secs(7)));
        assert!(journey.view().complete);
    }
    #[test]
    fn factory_left_return_waits_for_explicit_next_after_normal_usb_return() {
        let mut journey = model(Role::Left);
        unsafe {
            journey.machine.inner_mut().release = TargetRelease::Factory(
                FactoryRelease::at(std::env::temp_dir().join("nocfree-wired-ack-no-sources"))
                    .unwrap(),
            );
        }
        let now = Instant::now();
        let stock = || {
            Ok(Snapshot {
                devices: vec![Device {
                    location: 10,
                    vendor: 0x2886,
                    product: 0x8029,
                    name: "NocFree & ANSI".into(),
                }],
                mounts: vec![],
            })
        };
        journey.observe_at(stock(), now);
        assert!(journey.view().needs_wired_ack);
        assert!(journey.can_next_at(now));
        journey.observe_at(stock(), now + Duration::from_secs(20));
        assert!(!journey.view().complete);
        assert!(!journey.next_at(now + Duration::from_secs(26)));
        journey.observe_at(stock(), now + Duration::from_secs(27));
        assert!(journey.next_at(now + Duration::from_secs(27)));
        assert!(journey.view().complete);
    }
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
        journey.observe_advance_at(Ok(Snapshot::default()), now);
        journey.observe_advance_at(Ok(Snapshot::default()), now + Duration::from_secs(5));
        assert_eq!(*journey.machine.state(), Phase::PowerOn);
        assert!(journey.view().needs_power_on_ack);
        unsafe {
            *journey.machine.state_mut() = Phase::StartWait(now + Duration::from_secs(5));
        }
        journey.observe_advance_at(Ok(Snapshot::default()), now + Duration::from_secs(14));
        assert!(matches!(*journey.machine.state(), Phase::StartWait(_)));
        journey.observe_advance_at(Ok(Snapshot::default()), now + Duration::from_secs(15));
        assert_eq!(*journey.machine.state(), Phase::Reconnect);
        journey.observe_advance_at(Ok(normal(Role::Right, 11)), now + Duration::from_secs(16));
        assert_eq!(*journey.machine.state(), Phase::Reconnect);
        journey.observe_advance_at(Ok(normal(Role::Left, 10)), now + Duration::from_secs(17));
        assert_eq!(*journey.machine.state(), Phase::Disconnect);
        unsafe {
            *journey.machine.state_mut() = Phase::Reconnect;
        }
        journey.observe_mode_at(
            Ok(normal(Role::Right, 10)),
            None,
            now + Duration::from_secs(18),
        );
        journey.observe_mode_at(
            Ok(normal(Role::Right, 10)),
            None,
            now + Duration::from_secs(23),
        );
        journey.next_at(now + Duration::from_secs(23));
        assert_eq!(journey.role(), Role::Left);
        assert_eq!(*journey.machine.state(), Phase::Recovery);
    }
    #[test]
    fn failed_discovery_cannot_satisfy_power_off_interval() {
        let mut journey = model(Role::Left);
        let now = Instant::now();
        journey.observe_advance_at(Ok(Snapshot::default()), now);
        journey.observe_advance_at(Err("unavailable".into()), now + Duration::from_secs(5));
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
        journey.observe_advance_at(Ok(Snapshot::default()), now + Duration::from_secs(20));
        assert!(matches!(*journey.machine.state(), Phase::OffWait(_)));
    }
    #[test]
    fn verified_fresh_normal_return_can_finish_between_discovery_polls() {
        for role in PLAN {
            for phase in [
                Phase::Disconnect,
                Phase::OffWait(Instant::now()),
                Phase::PowerOn,
                Phase::StartWait(Instant::now()),
                Phase::Reconnect,
            ] {
                let mut journey = model(role);
                unsafe {
                    *journey.machine.state_mut() = phase;
                }
                settle(&mut journey, Ok(normal(role, 10)));
                assert!(journey.view().complete || journey.view().needs_recovery);
                assert_ne!(*journey.machine.state(), phase);
            }
        }
        let mut journey = model(Role::Right);
        let mut ambiguous = normal(Role::Right, 10);
        ambiguous.devices.extend(normal(Role::Right, 11).devices);
        journey.observe(Ok(ambiguous));
        assert_eq!(*journey.machine.state(), Phase::Disconnect);
        journey.observe(Ok(normal(Role::Right, 11)));
        assert_eq!(*journey.machine.state(), Phase::Disconnect);
        journey.next();
        assert!(matches!(journey.machine.state(), Phase::OffWait(_)));
        assert_eq!(journey.role(), Role::Right);
    }
    #[test]
    fn unknown_stock_version_is_noop_only_with_correlated_stock_provenance() {
        let mut session = Session::new();
        session.select_recovery_role(Role::Right);
        session.observe(Ok(Snapshot {
            devices: vec![Device {
                location: 10,
                vendor: 0x239a,
                product: 0x80d8,
                name: "NocFree nRF52833 Right".into(),
            }],
            mounts: vec![],
        }));
        session.observe(Ok(Snapshot {
            devices: vec![Device {
                location: 10,
                vendor: 0x239a,
                product: 0x0029,
                name: "NocFree &".into(),
            }],
            mounts: vec![BootMount {
                path: PathBuf::new(),
                info: "UF2 Bootloader 0.9.2-39-g0147d71\nModel: NocFree &\nBoard-ID: NocFree &"
                    .into(),
            }],
        }));
        assert_eq!(session.factory_recovery_role(), Some(Role::Right));
        let unknown = device::tests::archive();
        assert!(FactoryRelease::archive_role(&unknown).is_none());
        assert!(already_factory(Role::Right, &unknown, session.factory_recovery_role()).unwrap());
        assert!(
            !already_factory(
                Role::Right,
                &unknown,
                Session::new().factory_recovery_role()
            )
            .unwrap()
        );
        assert!(!already_factory(Role::Right, &unknown, Some(Role::Left)).unwrap());
    }
    #[test]
    fn latest_rmk_noop_has_explicit_title_and_resets_for_next_part() {
        let mut journey = model(Role::Right);
        unsafe {
            *journey.machine.state_mut() = Phase::Recovery;
        }
        let release = crate::release::fixture();
        let baseline = Baseline {
            location: 10,
            mount: BootMount {
                path: PathBuf::new(),
                info: String::new(),
            },
            folder: PathBuf::new(),
            bytes: archive_with(release.image(Role::Right)),
        };
        assert!(
            TargetImage::Rmk(Box::new(release.image(Role::Right).clone()))
                .exact(&baseline.bytes)
                .unwrap()
        );
        journey
            .machine
            .handle(&FirmwareEvent::RecoverySaved(&baseline, true));
        assert_eq!(journey.view().title, "Already latest version");
        assert!(!journey.view().can_transfer);
        settle(&mut journey, Ok(normal(Role::Right, 10)));
        assert_eq!(journey.role(), Role::Left);
        assert!(!journey.machine.inner().already_current);
        assert_eq!(journey.view().title, "Connect your keyboard");
    }
    #[test]
    fn automatic_transfer_requires_overall_approval_and_a_ready_phase() {
        let mut journey = FirmwareJourney::new(crate::release::fixture());
        assert!(!journey.transfer_if_ready().unwrap());
        journey.authorize_install();
        assert!(!journey.transfer_if_ready().unwrap());
        journey.cancel();
        assert!(!journey.transfer_if_ready().unwrap());
    }
    #[test]
    fn factory_shared_left_and_dongle_identity_requires_the_bound_port() {
        let mut stock = Snapshot::default();
        for location in [10, 11] {
            stock.devices.push(Device {
                location,
                vendor: 0x2886,
                product: 0x8029,
                name: "NocFree & ANSI".into(),
            });
        }
        assert!(normal_return(&stock, Role::Left, 10, true));
        assert!(normal_return(&stock, Role::Receiver, 11, true));
        assert!(!normal_return(&stock, Role::Receiver, 12, true));
        assert!(!normal_return(&stock, Role::Right, 10, true));
        assert!(!normal_return(&stock, Role::Left, 10, false));
        stock.devices.push(stock.devices[0].clone());
        assert!(!normal_return(&stock, Role::Left, 10, true));
    }
    #[test]
    #[ignore = "requires private original factory archives; offline only"]
    fn factory_target_roundtrip_verifies_softdevice_application_and_settings_exactly() {
        let root = std::env::temp_dir().join(format!(
            "factory-target-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut release = FactoryRelease::at(root.clone()).unwrap();
        let evidence = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".evidence");
        // Imports only local original bytes. No hardware is queried or written.
        // A separate temporary copy is used for target validation below.
        for (role, folder) in [
            (Role::Left, "factory-left"),
            (Role::Right, "factory-right"),
            (Role::Receiver, "receiver-backup"),
        ] {
            let source = evidence.join(folder).join("CURRENT.UF2");
            release.import(role, &source).unwrap();
            let archive = fs::read(source).unwrap();
            let image = TargetImage::Factory(release.image(role).unwrap().clone());
            let prior = root.join(format!("{role:?}-prior"));
            let current = root.join(format!("{role:?}-current"));
            fs::create_dir(&prior).unwrap();
            fs::create_dir(&current).unwrap();
            fs::write(prior.join("CURRENT.UF2"), &archive).unwrap();
            let intent = serde_json::json!({"schema":1,"kind":"RMK","role":format!("{role:?}"),"location":10,"target_sha256":"different-unfinished-RMK-target","backup_sha256":hash(&archive)});
            fs::write(
                prior.join("install-intent.json"),
                serde_json::to_vec(&intent).unwrap(),
            )
            .unwrap();
            assert!(
                reconcile_target(&current, role, 10, &device::tests::archive(), &image).is_ok()
            );
            assert!(!prior.join("install-verified.json").exists());
            assert_eq!(
                fs::read(prior.join("install-intent.json")).unwrap(),
                serde_json::to_vec(&intent).unwrap()
            );
            let next = root.join(format!("{role:?}-next-RMK"));
            fs::create_dir(&next).unwrap();
            let rmk = TargetImage::Rmk(Box::new(crate::release::fixture().image(role).clone()));
            assert!(reconcile_target(&next, role, 10, &archive, &rmk).is_err());
            fs::write(current.join("CURRENT.UF2"), &archive).unwrap();
            record_factory_supersession(&current, &current, role, 10, &archive, &image).unwrap();
            assert!(reconcile_target(&next, role, 10, &archive, &rmk).is_ok());
            assert!(!prior.join("install-verified.json").exists());
            assert_eq!(
                fs::read(prior.join("install-intent.json")).unwrap(),
                serde_json::to_vec(&intent).unwrap()
            );
            let mut changed = archive.clone();
            changed[32 + 100] ^= 1;
            fs::write(current.join("CURRENT.UF2"), changed).unwrap();
            assert!(reconcile_target(&next, role, 10, &archive, &rmk).is_err());
            fs::write(current.join("CURRENT.UF2"), &archive).unwrap();
            let marker = prior.join("install-superseded.json");
            let saved = fs::read(&marker).unwrap();
            let mut wrong: serde_json::Value = serde_json::from_slice(&saved).unwrap();
            wrong["old_intent_sha256"] = serde_json::json!("wrong");
            fs::write(&marker, serde_json::to_vec(&wrong).unwrap()).unwrap();
            assert!(reconcile_target(&next, role, 10, &archive, &rmk).is_err());
            fs::write(marker, saved).unwrap();
            assert!(already_factory(role, &archive, None).unwrap());
            for other in [Role::Left, Role::Right, Role::Receiver] {
                if other != role {
                    assert!(already_factory(other, &archive, Some(other)).is_err());
                }
            }
            assert!(image.exact(&archive).unwrap());
            assert_eq!(image.verify(&archive, &archive).unwrap(), 0);
            for address in [0x1000, 0x27000, 0x65000] {
                let mut wrong = archive.clone();
                for b in wrong.as_chunks_mut::<512>().0 {
                    if u32::from_le_bytes(b[12..16].try_into().unwrap()) == address {
                        b[32 + 100] ^= 1;
                        break;
                    }
                }
                assert!(image.verify(&wrong, &archive).is_err());
            }
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "requires private original and official ANSI 2.4.5 fixtures; offline only"]
    fn factory_version_difference_is_noop_without_selected_target_verification() {
        let root = std::env::temp_dir().join(format!(
            "factory-version-noop-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let evidence = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".evidence");
        let original = evidence.join("factory-left/CURRENT.UF2");
        let mut release = FactoryRelease::at(root.clone()).unwrap();
        release.import(Role::Left, &original).unwrap();
        release
            .import(
                Role::Left,
                &evidence.join("factory-version-skip/left-official.uf2"),
            )
            .unwrap();
        let archive = fs::read(original).unwrap();
        let target = TargetImage::Factory(release.image(Role::Left).unwrap().clone());
        assert!(!target.exact(&archive).unwrap());
        assert!(already_factory(Role::Left, &archive, Some(Role::Left)).unwrap());
        let mut journey = FirmwareJourney {
            machine: FirmwareData {
                release: TargetRelease::Factory(release),
                plan: vec![Role::Left],
                index: 0,
                baseline: None,
                error: None,
                attempted: false,
                already_current: false,
                install_authorized: true,
                verified_locations: vec![],
                retry_phase: None,
                wired_ack: false,
                wired_ack_available: false,
                latest: None,
                absent_since: None,
            }
            .state_machine(),
        };
        let baseline = Baseline {
            location: 10,
            mount: BootMount {
                path: PathBuf::new(),
                info: String::new(),
            },
            folder: root.clone(),
            bytes: archive,
        };
        journey
            .machine
            .handle(&FirmwareEvent::RecoverySaved(&baseline, true));
        assert_eq!(journey.view().title, "Already on factory firmware");
        assert!(!journey.view().can_transfer);
        assert!(!root.join("factory-verified.json").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn checkbox_pairs_preserve_transfer_order_and_exclude_unselected_parts() {
        for omitted in PLAN {
            let selected: Vec<_> = PLAN.into_iter().filter(|role| *role != omitted).collect();
            let scope = Scope::from_roles(selected.clone()).unwrap();
            let journey = FirmwareJourney::new_scoped(crate::release::fixture(), scope);
            assert_eq!(journey.plan(), selected);
            assert!(!journey.plan().contains(&omitted));
            assert_eq!(journey.role(), selected[0]);
            let factory_order: Vec<_> = [Role::Left, Role::Right, Role::Receiver]
                .into_iter()
                .filter(|role| *role != omitted)
                .collect();
            assert_eq!(
                scoped_plan(
                    &TargetRelease::Factory(
                        FactoryRelease::at(std::env::temp_dir().join(format!(
                            "nocfree-checkbox-plan-uncreated-{}",
                            std::process::id()
                        )))
                        .unwrap()
                    ),
                    scope
                ),
                factory_order
            );
        }
    }
    #[test]
    fn part_scope_returns_only_selected_part_and_cannot_advance_others() {
        for role in PLAN {
            let mut journey =
                FirmwareJourney::new_scoped(crate::release::fixture(), Scope::Part(role));
            assert_eq!(journey.plan(), &[role]);
            assert_eq!(journey.role(), role);
            let baseline = Baseline {
                location: 10,
                mount: BootMount {
                    path: PathBuf::new(),
                    info: String::new(),
                },
                folder: PathBuf::new(),
                bytes: vec![],
            };
            journey
                .machine
                .handle(&FirmwareEvent::RecoverySaved(&baseline, true));
            let wrong = PLAN.into_iter().find(|other| *other != role).unwrap();
            let snapshot = |r: Role| {
                Ok(Snapshot {
                    devices: vec![Device {
                        location: 10,
                        vendor: 0x4c4b,
                        product: u64::from(r.product()),
                        name: r.name().into(),
                    }],
                    mounts: vec![],
                })
            };
            journey.observe(snapshot(wrong));
            assert!(!journey.view().complete);
            assert_eq!(journey.role(), role);
            settle(&mut journey, snapshot(role));
            assert!(journey.view().complete);
            assert_eq!(journey.role(), role);
        }
    }
    #[test]
    #[ignore = "uses the owner's private factory original fixture"]
    fn factory_part_accepts_partial_validated_source() {
        let root =
            std::env::temp_dir().join(format!("nocfree-factory-part-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut source = FactoryRelease::at(root.clone()).unwrap();
        let original = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".evidence/factory-left/CURRENT.UF2");
        source.import(Role::Left, &original).unwrap();
        assert!(!source.is_complete());
        let journey =
            FirmwareJourney::factory_scoped(source.clone(), Scope::Part(Role::Left)).unwrap();
        assert_eq!(journey.plan(), &[Role::Left]);
        assert!(FirmwareJourney::factory_scoped(source.clone(), Scope::Part(Role::Right)).is_err());
        assert!(FirmwareJourney::factory(source).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn restore_and_install_have_explicit_distinct_role_order() {
        assert_eq!(PLAN, [Role::Receiver, Role::Right, Role::Left]);
        let source = FactoryRelease::at(
            std::env::temp_dir().join(format!("empty-factory-plan-{}", std::process::id())),
        )
        .unwrap();
        assert_eq!(
            TargetRelease::Factory(source).plan(),
            [Role::Left, Role::Right, Role::Receiver]
        );
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
            journey.observe_advance_at(Err("USB inventory unavailable.".into()), now);
            assert_eq!(*journey.machine.state(), Phase::Failed);
            assert_eq!(journey.machine.inner().retry_phase, Some(Phase::Disconnect));
            // Errors cannot supply evidence of a power-off interval. A new
            // successful observation starts its own interval.
            journey.observe_advance_at(Ok(Snapshot::default()), now + Duration::from_secs(61));
            assert_eq!(journey.role(), Role::Right);
            assert!(journey.machine.inner().attempted);
            assert_eq!(
                *journey.machine.state(),
                Phase::OffWait(now + Duration::from_secs(61))
            );
            assert!(journey.transfer().is_err());
        }
    }
}

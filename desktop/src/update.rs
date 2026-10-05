//! Host-only update checkpointing. This module cannot write to a keyboard.
//!
//! `Prepared` means a candidate and supplied backup bytes are bound to a host checkpoint. It
//! does not establish device eligibility, recovery, or permission to flash.
//! A future observation adapter and reviewed transfer service must supply those
//! gates. Journals record intent and never replay an interrupted transfer.
use crate::update_image::ValidatedImage;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Binding {
    session: String,
    device_observation_sha256: String,
    image_sha256: String,
    backup_sha256: String,
}
impl Binding {
    /// Bind a reviewed candidate and supplied backup bytes. Observation digest is
    /// a correlation token from a future adapter, not a hardware attestation.
    pub fn new(
        image: &ValidatedImage,
        reviewed_image_sha256: &str,
        session: &str,
        device_observation_sha256: &str,
        backup: &[u8],
    ) -> Result<Self, String> {
        let image_sha256 = image.sha256().to_string();
        if image_sha256 != reviewed_image_sha256
            || !valid_digest(&image_sha256)
            || !valid_digest(device_observation_sha256)
            || backup.is_empty()
            || session.is_empty()
            || session.len() > 128
            || !session
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        {
            return Err(
                "The checkpoint binding is incomplete or does not match the reviewed image.".into(),
            );
        }
        Ok(Self {
            session: session.into(),
            device_observation_sha256: device_observation_sha256.into(),
            image_sha256,
            backup_sha256: digest(backup),
        })
    }
    pub fn image_sha256(&self) -> &str {
        &self.image_sha256
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    Prepared,
    AwaitingApproval,
    TransferStarted,
    NeedsReconciliation,
    Verified,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Record {
    schema: u8,
    sequence: u32,
    binding: Binding,
    state: State,
}

// Only private disk records can reconstruct a binding. Public callers must
// pass an opaque validated image through Binding::new.
impl<'de> Deserialize<'de> for Record {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct DiskBinding {
            session: String,
            device_observation_sha256: String,
            image_sha256: String,
            backup_sha256: String,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct DiskRecord {
            schema: u8,
            sequence: u32,
            binding: DiskBinding,
            state: State,
        }
        let disk = DiskRecord::deserialize(deserializer)?;
        Ok(Self {
            schema: disk.schema,
            sequence: disk.sequence,
            state: disk.state,
            binding: Binding {
                session: disk.binding.session,
                device_observation_sha256: disk.binding.device_observation_sha256,
                image_sha256: disk.binding.image_sha256,
                backup_sha256: disk.binding.backup_sha256,
            },
        })
    }
}

/// Durable append-only host journal. The supplied directory must be private
/// application host storage, never a mounted keyboard drive. This component
/// writes only its own numbered JSON records and contains no firmware backend.
pub struct Journal {
    directory: PathBuf,
    record: Record,
}
impl Journal {
    /// `create_dir` and `create_new` fail closed on an existing checkpoint.
    pub fn create(binding: Binding) -> Result<Self, String> {
        let directory = host_directory(&binding)?;
        Self::create_at(&directory, binding)
    }
    fn create_at(directory: &Path, binding: Binding) -> Result<Self, String> {
        fs::create_dir(directory).map_err(|_| {
            "A checkpoint already exists or host storage is unavailable.".to_string()
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
                .map_err(|_| "Could not protect the host checkpoint.".to_string())?;
        }
        let record = Record {
            schema: 1,
            sequence: 0,
            binding,
            state: State::Prepared,
        };
        append(directory, &record)?;
        sync_directory(
            directory
                .parent()
                .ok_or("Checkpoint requires a parent directory.")?,
        )?;
        Ok(Self {
            directory: directory.into(),
            record,
        })
    }
    /// Reopening transfer intent records reconciliation immediately. Neither
    /// resumption nor a second transfer is available through this API.
    pub fn open(expected: &Binding) -> Result<Self, String> {
        Self::open_at(&host_directory(expected)?, expected)
    }
    fn open_at(directory: &Path, expected: &Binding) -> Result<Self, String> {
        let record = read_latest(directory)?;
        if &record.binding != expected {
            return Err(
                "The checkpoint belongs to a different session, device, backup, or image.".into(),
            );
        }
        let mut journal = Self {
            directory: directory.into(),
            record,
        };
        if journal.state() == State::TransferStarted {
            journal.advance(State::NeedsReconciliation)?;
        }
        Ok(journal)
    }
    pub fn state(&self) -> State {
        self.record.state
    }
    pub fn await_approval(&mut self) -> Result<(), String> {
        self.advance(State::AwaitingApproval)
    }
    /// Records scoped owner intent only. Does not confer device eligibility or
    /// perform a transfer. Must be persisted before any future backend starts.
    pub fn record_transfer_intent(&mut self, approved: &Binding) -> Result<(), String> {
        if approved != &self.record.binding {
            return Err("Approval no longer matches the checkpoint.".into());
        }
        self.advance(State::TransferStarted)
    }
    pub fn require_reconciliation(&mut self) -> Result<(), String> {
        self.advance(State::NeedsReconciliation)
    }
    // Verified has no public transition until an actual device readback adapter
    // exists. A caller-provided boolean or checksum must not impersonate it.
    fn advance(&mut self, state: State) -> Result<(), String> {
        if !allowed(self.record.state, state) {
            return Err("The checkpoint cannot make this transition.".into());
        }
        if read_latest(&self.directory)? != self.record {
            return Err("The checkpoint changed; reload it before continuing.".into());
        }
        let next = Record {
            sequence: self
                .record
                .sequence
                .checked_add(1)
                .ok_or("Checkpoint sequence exhausted.")?,
            state,
            ..self.record.clone()
        };
        append(&self.directory, &next)?;
        self.record = next;
        Ok(())
    }
}
fn allowed(from: State, to: State) -> bool {
    matches!(
        (from, to),
        (State::Prepared, State::AwaitingApproval)
            | (State::AwaitingApproval, State::TransferStarted)
            | (State::TransferStarted, State::NeedsReconciliation)
    )
}
fn append(directory: &Path, record: &Record) -> Result<(), String> {
    check_directory(directory)?;
    let bytes = serde_json::to_vec(record).map_err(|_| "Could not encode checkpoint.")?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    crate::host_storage::private_file_options(&mut options);
    let mut file = options
        .open(directory.join(format!("{:08}.json", record.sequence)))
        .map_err(|_| "Checkpoint conflicts with another operation.")?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Checkpoint persistence failed; reconciliation is required.")?;
    sync_directory(directory)
}
fn sync_directory(directory: &Path) -> Result<(), String> {
    crate::host_storage::sync_directory(directory)
}
fn read_latest(directory: &Path) -> Result<Record, String> {
    check_directory(directory)?;
    let mut files = fs::read_dir(directory)
        .map_err(|_| "Could not read checkpoint.")?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Checkpoint directory changed.")?;
    files.sort();
    if files.is_empty() || files.len() > 5 {
        return Err("Checkpoint history is invalid.".into());
    }
    let mut previous: Option<Record> = None;
    for (sequence, path) in files.into_iter().enumerate() {
        if path.file_name().and_then(|name| name.to_str())
            != Some(format!("{sequence:08}.json").as_str())
            || !fs::symlink_metadata(&path)
                .map_err(|_| "Checkpoint file is unavailable.")?
                .file_type()
                .is_file()
        {
            return Err("Unexpected checkpoint file.".into());
        }
        let file = fs::File::open(&path).map_err(|_| "Could not read checkpoint file.")?;
        let mut bytes = Vec::new();
        file.take(4097)
            .read_to_end(&mut bytes)
            .map_err(|_| "Could not read checkpoint file.")?;
        if bytes.len() > 4096 {
            return Err("Checkpoint file is too large.".into());
        }
        let record: Record = serde_json::from_slice(&bytes)
            .map_err(|_| "Checkpoint is incomplete; manual reconciliation is required.")?;
        if record.schema != 1
            || record.sequence as usize != sequence
            || !valid_digest(&record.binding.image_sha256)
            || !valid_digest(&record.binding.device_observation_sha256)
            || !valid_digest(&record.binding.backup_sha256)
            || record.binding.session.is_empty()
            || record.binding.session.len() > 128
            || !record
                .binding
                .session
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        {
            return Err("Checkpoint schema or binding is invalid.".into());
        }
        match &previous {
            None if record.state != State::Prepared => {
                return Err("Checkpoint does not start with preparation.".into());
            }
            Some(last) if last.binding != record.binding || !allowed(last.state, record.state) => {
                return Err("Checkpoint history is inconsistent.".into());
            }
            _ => {}
        }
        previous = Some(record);
    }
    previous.ok_or("Checkpoint is empty.".into())
}
fn check_directory(path: &Path) -> Result<(), String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| "Checkpoint directory is unavailable.")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Checkpoint directory must be private host storage.".into());
    }
    Ok(())
}
fn host_directory(binding: &Binding) -> Result<PathBuf, String> {
    // No caller-provided destination path or mounted-device backend.
    let application = crate::host_storage::application_root()?;
    let root = application.join("update-checkpoints");
    ensure_private_directory(&root)?;
    Ok(root.join(&binding.session))
}
fn ensure_private_directory(path: &Path) -> Result<(), String> {
    let created = match fs::create_dir(path) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(_) => return Err("Private host storage is unavailable.".into()),
    };
    check_directory(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| "Could not protect private host storage.")?;
    }
    if created {
        sync_directory(
            path.parent()
                .ok_or("Private storage requires a parent directory.")?,
        )?;
    }
    Ok(())
}
fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Temporary(PathBuf);
    impl Temporary {
        fn new() -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let suffix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            Self(std::env::temp_dir().join(format!(
                "nocfree-journal-{}-{suffix}-{counter}",
                std::process::id()
            )))
        }
    }
    impl Drop for Temporary {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn binding() -> Binding {
        Binding {
            session: "synthetic-session".into(),
            device_observation_sha256: digest(b"synthetic-observation"),
            image_sha256: digest(b"synthetic-image"),
            backup_sha256: digest(b"synthetic-backup"),
        }
    }
    #[test]
    fn interrupted_intent_reconciles_and_never_replays() {
        let temp = Temporary::new();
        let binding = binding();
        let mut journal = Journal::create_at(&temp.0, binding.clone()).unwrap();
        journal.await_approval().unwrap();
        journal.record_transfer_intent(&binding).unwrap();
        drop(journal);
        let mut resumed = Journal::open_at(&temp.0, &binding).unwrap();
        assert_eq!(resumed.state(), State::NeedsReconciliation);
        assert!(resumed.record_transfer_intent(&binding).is_err());
        assert_eq!(
            Journal::open_at(&temp.0, &binding).unwrap().state(),
            State::NeedsReconciliation
        );
    }
    #[test]
    fn approval_is_bound_and_transitions_are_finite() {
        let temp = Temporary::new();
        let binding = binding();
        let mut journal = Journal::create_at(&temp.0, binding.clone()).unwrap();
        assert!(journal.record_transfer_intent(&binding).is_err());
        journal.await_approval().unwrap();
        let mut changed = binding.clone();
        changed.image_sha256 = digest(b"changed");
        assert!(journal.record_transfer_intent(&changed).is_err());
        assert!(Journal::open_at(&temp.0, &changed).is_err());
        journal.record_transfer_intent(&binding).unwrap();
        assert!(journal.record_transfer_intent(&binding).is_err());
    }
    #[test]
    fn conflicting_and_partial_histories_fail_closed() {
        let temp = Temporary::new();
        let binding = binding();
        let mut journal = Journal::create_at(&temp.0, binding.clone()).unwrap();
        assert!(Journal::create_at(&temp.0, binding.clone()).is_err());
        let mut concurrent = Journal::open_at(&temp.0, &binding).unwrap();
        journal.await_approval().unwrap();
        assert!(concurrent.await_approval().is_err());
        fs::write(temp.0.join("00000002.json"), b"{").unwrap();
        assert!(Journal::open_at(&temp.0, &binding).is_err());
    }
    #[test]
    fn unexpected_files_and_gaps_fail_closed() {
        let temp = Temporary::new();
        let binding = binding();
        Journal::create_at(&temp.0, binding.clone()).unwrap();
        fs::write(temp.0.join("00000002.json"), b"{}").unwrap();
        assert!(Journal::open_at(&temp.0, &binding).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn symlink_journal_root_is_rejected() {
        let temp = Temporary::new();
        let redirected = Temporary::new();
        let binding = binding();
        Journal::create_at(&temp.0, binding.clone()).unwrap();
        std::os::unix::fs::symlink(&temp.0, &redirected.0).unwrap();
        assert!(Journal::open_at(&redirected.0, &binding).is_err());
        fs::remove_file(&redirected.0).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn application_directory_symlink_cannot_redirect_storage() {
        let real = Temporary::new();
        let link = Temporary::new();
        fs::create_dir(&real.0).unwrap();
        std::os::unix::fs::symlink(&real.0, &link.0).unwrap();
        assert!(ensure_private_directory(&link.0).is_err());
        assert_eq!(fs::read_dir(&real.0).unwrap().count(), 0);
        fs::remove_file(&link.0).unwrap();
    }
}

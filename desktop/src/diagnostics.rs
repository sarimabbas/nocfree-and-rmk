//! Bounded local diagnostics. Callers provide fixed labels, never device data or errors.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_BYTES: u64 = 256 * 1024;
const FILES: usize = 4;
const MARKER: &str = "running";
// ditto normally includes AppleDouble metadata. Export only the curated file bytes.
#[cfg(target_os = "macos")]
const ARCHIVE_OPTIONS: [&str; 6] = [
    "-c",
    "-k",
    "--keepParent",
    "--norsrc",
    "--noextattr",
    "--noqtn",
];
static LOGGER: OnceLock<Mutex<Logger>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    App,
    Journey,
    Operation,
    Device,
    Health,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    Startup,
    PreviousUncleanExit,
    Panic,
    Heartbeat,
    Started,
    Completed,
    Failed,
    Cancelled,
    StateChanged,
    ConnectionChanged,
    Waiting,
    Ready,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Component {
    Left,
    Right,
    Dongle,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionMode {
    Wired,
    Bluetooth,
    Dongle,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    page: String,
    stage: String,
    busy: bool,
    usb_parts: u8,
    next_ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    component: Option<Component>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<ConnectionMode>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Record {
    time_unix_seconds: u64,
    app_version: String,
    category: Category,
    event: Event,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot: Option<Snapshot>,
}
struct Logger {
    root: PathBuf,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn private_directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|_| "Could not create the diagnostics folder.".to_owned())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| "Could not protect the diagnostics folder.".to_owned())?;
    }
    Ok(())
}
fn private_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|_| "Could not save diagnostics.".to_owned())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Could not finish saving diagnostics.".to_owned())
}
impl Logger {
    fn path(&self, index: usize) -> PathBuf {
        self.root.join(format!("companion-{index}.jsonl"))
    }
    fn write(&self, record: Record) -> Result<(), String> {
        let mut bytes =
            serde_json::to_vec(&record).map_err(|_| "Could not encode diagnostics.".to_owned())?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_BYTES {
            return Err("Diagnostic event exceeds its limit.".into());
        }
        let current = self.path(0);
        if fs::metadata(&current).map(|m| m.len()).unwrap_or(0) + bytes.len() as u64 > MAX_BYTES {
            for index in (1..FILES).rev() {
                let previous = self.path(index - 1);
                if previous.exists() {
                    fs::rename(previous, self.path(index))
                        .map_err(|_| "Could not rotate diagnostics.".to_owned())?;
                }
            }
        }
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(current)
            .and_then(|mut f| f.write_all(&bytes))
            .map_err(|_| "Could not write diagnostics.".to_owned())
    }
}
fn record(category: Category, event: Event) -> Record {
    Record {
        time_unix_seconds: now(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        category,
        event,
        snapshot: None,
    }
}
/// Nonfatal initialization: the application may continue when diagnostics are unavailable.
pub fn initialize() -> Result<PathBuf, String> {
    if let Some(logger) = LOGGER.get() {
        return logger
            .lock()
            .map(|l| l.root.clone())
            .map_err(|_| "Diagnostics are unavailable.".into());
    }
    let root = diagnostics_directory()?;
    let logger = start_logger(root.clone())?;
    install_logger(logger);
    Ok(root)
}
fn diagnostics_directory() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("LOCALAPPDATA")
        .map(|p| PathBuf::from(p).join("NocFree RMK Companion/Logs"));
    #[cfg(target_os = "macos")]
    let root = std::env::var_os("HOME")
        .map(|p| PathBuf::from(p).join("Library/Logs/NocFree RMK Companion"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let root = std::env::var_os("XDG_STATE_HOME")
        .filter(|p| Path::new(p).is_absolute())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/state")))
        .map(|p| p.join("nocfree-rmk-companion"));
    root.ok_or_else(|| "Could not locate the diagnostics folder.".into())
}
/// Open a folder with the platform's file manager, without passing through a shell.
fn open_folder(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let program = "/usr/bin/open";
    #[cfg(target_os = "windows")]
    let program = "explorer.exe";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let program = "xdg-open";
    Command::new(program)
        .arg(path)
        .status()
        .ok()
        .filter(|s| s.success())
        .map(|_| ())
        .ok_or_else(|| "Could not open the diagnostics folder.".into())
}
pub fn reveal_archive(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("/usr/bin/open")
            .arg("-R")
            .arg(path)
            .status()
            .ok()
            .filter(|s| s.success())
            .map(|_| ())
            .ok_or_else(|| "Could not show the support bundle.".into())
    }
    #[cfg(not(target_os = "macos"))]
    open_folder(path.parent().ok_or("Support folder unavailable.")?)
}
fn archive_support(source: &Path, destination: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let result = Command::new("/usr/bin/ditto")
        .args(ARCHIVE_OPTIONS)
        .arg(source)
        .arg(destination)
        .status();
    #[cfg(target_os = "windows")]
    let result = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", "Compress-Archive -LiteralPath $env:NOCFREE_SUPPORT_CONTENTS -DestinationPath $env:NOCFREE_SUPPORT_ARCHIVE -ErrorAction Stop"])
        .env("NOCFREE_SUPPORT_CONTENTS", source)
        .env("NOCFREE_SUPPORT_ARCHIVE", destination).status();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let result = Command::new("zip")
        .args(["-q", "-r"])
        .arg(destination)
        .arg("contents")
        .current_dir(source.parent().ok_or("Support folder unavailable.")?)
        .status();
    result
        .ok()
        .filter(|s| s.success())
        .map(|_| ())
        .ok_or_else(|| "Could not create the support bundle.".into())
}
fn start_logger(root: PathBuf) -> Result<Logger, String> {
    private_directory(&root)?;
    let logger = Logger { root };
    if logger.root.join(MARKER).exists() {
        logger.write(record(Category::App, Event::PreviousUncleanExit))?;
    }
    private_write(&logger.root.join(MARKER), b"running\n")?;
    logger.write(record(Category::App, Event::Startup))?;
    Ok(logger)
}
fn install_logger(logger: Logger) {
    let _ = LOGGER.set(Mutex::new(logger));
    // Panic payloads and source paths may contain private data. Log neither.
    std::panic::set_hook(Box::new(|_| {
        // A panic during a diagnostic write must not wait on its own lock.
        if let Some(logger) = LOGGER.get()
            && let Ok(logger) = logger.try_lock()
        {
            let _ = logger.write(record(Category::App, Event::Panic));
        }
    }));
}
fn finish_logger(logger: &Logger) {
    let _ = fs::remove_file(logger.root.join(MARKER));
}
pub fn event(category: Category, event: Event) {
    if let Some(logger) = LOGGER.get()
        && let Ok(logger) = logger.try_lock()
    {
        let _ = logger.write(record(category, event));
    }
}
fn fixed_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b':' | b' '))
}
/// Page/stage must be compile-time labels chosen by the caller, never formatted user data.
pub fn snapshot(
    page: &'static str,
    stage: &'static str,
    busy: bool,
    usb_parts: u8,
    next_ready: bool,
    component: Option<Component>,
    mode: Option<ConnectionMode>,
) {
    if !fixed_label(page) || !fixed_label(stage) || usb_parts > 3 {
        return;
    }
    if let Some(logger) = LOGGER.get()
        && let Ok(logger) = logger.try_lock()
    {
        let mut entry = record(Category::Journey, Event::StateChanged);
        entry.snapshot = Some(Snapshot {
            page: page.into(),
            stage: stage.into(),
            busy,
            usb_parts,
            next_ready,
            component,
            mode,
        });
        let _ = logger.write(entry);
    }
}
pub fn shutdown() {
    if let Some(logger) = LOGGER.get()
        && let Ok(logger) = logger.lock()
    {
        finish_logger(&logger);
    }
}
pub fn open_logs() -> Result<(), String> {
    let logger = LOGGER
        .get()
        .ok_or("Diagnostics are unavailable.")?
        .lock()
        .map_err(|_| "Diagnostics are unavailable.")?;
    open_folder(&logger.root)
}
#[cfg(test)]
fn export_with(
    logger: &Logger,
    destination: &Path,
    archive: impl FnOnce(&Path, &Path) -> Result<(), String>,
) -> Result<PathBuf, String> {
    export_with_screenshot(logger, destination, Err("Unavailable".into()), archive)
}
fn export_with_screenshot(
    logger: &Logger,
    destination: &Path,
    screenshot: Result<Vec<u8>, String>,
    archive: impl FnOnce(&Path, &Path) -> Result<(), String>,
) -> Result<PathBuf, String> {
    private_directory(destination)?;
    let staging = destination.join("contents");
    private_directory(&staging)?;
    let result = (|| {
        for index in 0..FILES {
            let path = logger.path(index);
            if !path.exists() {
                continue;
            }
            if !fs::symlink_metadata(&path)
                .map_err(|_| "Could not read diagnostics.")?
                .is_file()
                || fs::metadata(&path)
                    .map_err(|_| "Could not read diagnostics.")?
                    .len()
                    > MAX_BYTES
            {
                return Err("Diagnostic file is invalid or exceeds its limit.".into());
            }
            let bytes = fs::read(&path).map_err(|_| "Could not read diagnostics.")?;
            let mut safe = Vec::new();
            for line in bytes.split(|b| *b == b'\n').filter(|line| !line.is_empty()) {
                let record: Record = serde_json::from_slice(line)
                    .map_err(|_| "Diagnostic file contains an invalid event.".to_owned())?;
                if record.app_version.len() > 32
                    || semver::Version::parse(&record.app_version).is_err()
                {
                    return Err("Diagnostic file contains an invalid application version.".into());
                }
                if record.snapshot.as_ref().is_some_and(|s| {
                    !fixed_label(&s.page) || !fixed_label(&s.stage) || s.usb_parts > 3
                }) {
                    return Err("Diagnostic file contains an invalid state.".into());
                }
                serde_json::to_writer(&mut safe, &record)
                    .map_err(|_| "Could not encode diagnostics.")?;
                safe.push(b'\n');
            }
            private_write(&staging.join(format!("companion-{index}.jsonl")), &safe)?;
        }
        #[cfg(target_os = "macos")]
        let os_version = Command::new("/usr/bin/sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|v| v.trim().to_owned())
            .filter(|v| v.len() < 32 && v.bytes().all(|b| b.is_ascii_digit() || b == b'.'));
        #[cfg(not(target_os = "macos"))]
        let os_version: Option<String> = None;
        let screenshot_status = match screenshot {
            Ok(bytes)
                if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() <= 16 * 1024 * 1024 =>
            {
                let path = staging.join("app-window.png");
                if private_write(&path, &bytes).is_ok() {
                    "captured"
                } else {
                    let _ = fs::remove_file(path);
                    "unavailable"
                }
            }
            _ => "unavailable",
        };
        let context = serde_json::json!({"screenshot": screenshot_status,"app_version":env!("CARGO_PKG_VERSION"), "os":std::env::consts::OS, "architecture":std::env::consts::ARCH, "os_version":os_version});
        private_write(
            &staging.join("context.json"),
            &serde_json::to_vec_pretty(&context)
                .map_err(|_| "Could not encode support context.")?,
        )?;
        let bundle = destination.join("NocFree-RMK-Companion-diagnostics.zip");
        archive(&staging, &bundle)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&bundle, fs::Permissions::from_mode(0o600))
                .map_err(|_| "Could not protect the support bundle.")?;
        }
        Ok(bundle)
    })();
    let _ = fs::remove_dir_all(&staging);
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}
pub fn export_logs(screenshot: Result<Vec<u8>, String>) -> Result<PathBuf, String> {
    let logger = LOGGER
        .get()
        .ok_or("Diagnostics are unavailable.")?
        .lock()
        .map_err(|_| "Diagnostics are unavailable.")?;
    let destination = std::env::temp_dir().join(format!(
        "NocFree-RMK-support-{}-{}",
        now(),
        std::process::id()
    ));
    if destination.exists() {
        return Err("A support export already exists. Try again in a moment.".into());
    }
    export_with_screenshot(&logger, &destination, screenshot, archive_support)
}

#[cfg(test)]
mod tests {
    use super::*;
    static TEMP_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    fn temporary() -> PathBuf {
        std::env::temp_dir().join(format!(
            "nocfree-diagnostics-test-{}-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    fn events(root: &Path) -> Vec<Event> {
        fs::read_to_string(root.join("companion-0.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Record>(line).unwrap().event)
            .collect()
    }
    #[test]
    fn startup_marker_distinguishes_unclean_exit_from_clean_shutdown() {
        let root = temporary();
        let first = start_logger(root.clone()).unwrap();
        assert!(root.join(MARKER).exists());
        assert!(matches!(events(&root).as_slice(), [Event::Startup]));
        // An abruptly ended process does not execute its shutdown helper.
        drop(first);
        let restarted = start_logger(root.clone()).unwrap();
        assert!(matches!(
            events(&root).as_slice(),
            [Event::Startup, Event::PreviousUncleanExit, Event::Startup]
        ));
        finish_logger(&restarted);
        assert!(!root.join(MARKER).exists());
        let clean = start_logger(root.clone()).unwrap();
        assert!(matches!(
            events(&root).as_slice(),
            [
                Event::Startup,
                Event::PreviousUncleanExit,
                Event::Startup,
                Event::Startup
            ]
        ));
        finish_logger(&clean);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn panic_child_process() {
        // Only the isolated parent test supplies this path; never repurpose HOME.
        let Some(root) = std::env::var_os("NOCFREE_DIAGNOSTICS_PANIC_TEST_ROOT") else {
            return;
        };
        install_logger(start_logger(PathBuf::from(root)).unwrap());
        panic!("PRIVATE PANIC PAYLOAD AND IDENTIFIER");
    }
    #[test]
    fn actual_panic_hook_retains_restart_evidence_without_exporting_payload() {
        let root = temporary();
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "diagnostics::tests::panic_child_process",
                "--nocapture",
            ])
            .env("NOCFREE_DIAGNOSTICS_PANIC_TEST_ROOT", &root)
            .output()
            .unwrap();
        assert!(!child.status.success());
        assert!(root.join(MARKER).exists());
        assert!(matches!(
            events(&root).as_slice(),
            [Event::Startup, Event::Panic]
        ));
        let logger = start_logger(root.clone()).unwrap();
        assert!(matches!(
            events(&root).as_slice(),
            [
                Event::Startup,
                Event::Panic,
                Event::PreviousUncleanExit,
                Event::Startup
            ]
        ));
        let destination = temporary();
        export_with(&logger, &destination, |staging, archive| {
            let log = fs::read_to_string(staging.join("companion-0.jsonl")).unwrap();
            assert!(log.contains("panic"));
            assert!(log.contains("previous_unclean_exit"));
            assert!(!log.contains("PRIVATE"));
            assert!(!log.contains(&root.display().to_string()));
            assert!(!staging.join(MARKER).exists());
            fs::write(archive, b"reviewed-export-fixture").unwrap();
            Ok(())
        })
        .unwrap();
        finish_logger(&logger);
        fs::remove_dir_all(destination).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn component_and_mode_are_fixed_enums_and_old_snapshots_still_load() {
        let mut value = serde_json::json!({"page":"install","stage":"waiting","busy":false,"usb_parts":1,"next_ready":false});
        let old: Snapshot = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(old.component, None);
        assert_eq!(old.mode, None);
        value["component"] = serde_json::json!("right");
        value["mode"] = serde_json::json!("bluetooth");
        let current: Snapshot = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(current.component, Some(Component::Right));
        assert_eq!(current.mode, Some(ConnectionMode::Bluetooth));
        value["component"] = serde_json::json!("private-device-identifier");
        assert!(serde_json::from_value::<Snapshot>(value.clone()).is_err());
        value["component"] = serde_json::json!("dongle");
        value["mode"] = serde_json::json!("private-free-text");
        assert!(serde_json::from_value::<Snapshot>(value).is_err());
    }
    #[test]
    fn rotation_bounds_each_file_and_retained_count() {
        let root = temporary();
        private_directory(&root).unwrap();
        let logger = Logger { root: root.clone() };
        for _ in 0..15000 {
            logger
                .write(record(Category::Health, Event::Heartbeat))
                .unwrap();
        }
        assert_eq!(fs::read_dir(&root).unwrap().count(), FILES);
        for index in 0..FILES {
            assert!(fs::metadata(logger.path(index)).unwrap().len() <= MAX_BYTES);
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn native_archive_command_exports_logs_and_removes_staging() {
        let root = temporary();
        private_directory(&root).unwrap();
        let logger = Logger { root: root.clone() };
        logger.write(record(Category::App, Event::Startup)).unwrap();
        let destination = temporary();
        let bundle = export_with(&logger, &destination, archive_support).unwrap();
        assert!(fs::read(&bundle).unwrap().starts_with(b"PK"));
        assert!(!destination.join("contents").exists());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(destination).unwrap();
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn native_export_creates_a_private_archive_with_bounded_logs() {
        let root = temporary();
        private_directory(&root).unwrap();
        let logger = Logger { root: root.clone() };
        logger.write(record(Category::App, Event::Startup)).unwrap();
        let destination = temporary();
        let bundle = export_with(&logger, &destination, |source, target| {
            assert!(
                Command::new("/usr/bin/xattr")
                    .args(["-w", "com.nocfree.diagnostics-test", "private-metadata"])
                    .arg(source.join("context.json"))
                    .status()
                    .unwrap()
                    .success()
            );
            Command::new("/usr/bin/ditto")
                .args(ARCHIVE_OPTIONS)
                .arg(source)
                .arg(target)
                .status()
                .ok()
                .filter(|s| s.success())
                .map(|_| ())
                .ok_or("Could not create the support bundle.".into())
        })
        .unwrap();
        assert!(fs::metadata(&bundle).unwrap().len() > 0);
        let listing = Command::new("/usr/bin/unzip")
            .args(["-Z", "-1"])
            .arg(&bundle)
            .output()
            .unwrap();
        assert!(listing.status.success());
        let listing = String::from_utf8(listing.stdout).unwrap();
        let mut files: Vec<_> = listing
            .lines()
            .filter(|name| !name.ends_with('/'))
            .collect();
        files.sort_unstable();
        assert_eq!(
            files,
            ["contents/companion-0.jsonl", "contents/context.json"]
        );
        assert!(!destination.join("contents").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&bundle).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        fs::remove_dir_all(destination).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn screenshot_is_bounded_and_failure_keeps_logs_exportable() {
        let root = temporary();
        let logger = start_logger(root.clone()).unwrap();
        for screenshot in [
            Ok(b"\x89PNG\r\n\x1a\nfixture".to_vec()),
            Err("PRIVATE ERROR".into()),
            Ok(vec![0; 17 * 1024 * 1024]),
        ] {
            let expected = screenshot.as_ref().is_ok_and(|bytes| {
                bytes.starts_with(b"\x89PNG") && bytes.len() <= 16 * 1024 * 1024
            });
            let destination = temporary();
            export_with_screenshot(&logger, &destination, screenshot, |staging, archive| {
                assert_eq!(staging.join("app-window.png").exists(), expected);
                let context = fs::read_to_string(staging.join("context.json")).unwrap();
                assert!(context.contains(if expected { "captured" } else { "unavailable" }));
                assert!(!context.contains("PRIVATE"));
                fs::write(archive, b"fixture").unwrap();
                Ok(())
            })
            .unwrap();
            fs::remove_dir_all(destination).unwrap();
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn export_failure_is_clean_and_only_fixed_safe_events_are_exported() {
        let root = temporary();
        private_directory(&root).unwrap();
        let logger = Logger { root: root.clone() };
        logger.write(record(Category::App, Event::Panic)).unwrap();
        fs::write(root.join("private.txt"), b"PRIVATE SECRET").unwrap();
        fs::write(root.join(MARKER), b"running").unwrap();
        let destination = temporary();
        let result = export_with(&logger, &destination, |staging, _| {
            assert!(!staging.join("private.txt").exists());
            assert!(!staging.join(MARKER).exists());
            let log = fs::read_to_string(staging.join("companion-0.jsonl")).unwrap();
            assert!(log.contains("panic"));
            assert!(!log.contains("PRIVATE"));
            Err("Could not create the support bundle.".into())
        });
        assert!(result.is_err());
        assert!(!destination.exists());
        let destination = temporary();
        fs::write(logger.path(0), b"{\"payload\":\"PRIVATE SECRET\"}\n").unwrap();
        assert!(
            export_with(&logger, &destination, |_, _| panic!(
                "invalid logs must not export"
            ))
            .is_err()
        );
        assert!(!destination.exists());
        fs::remove_dir_all(root).unwrap();
    }
}

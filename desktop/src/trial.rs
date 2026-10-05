//! Owner-guided startup observations. The controller owns all approved transfers;
//! this module only observes USB and writes private host-side status files.
use crate::device::{self, Role, Snapshot};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Step {
    ConnectLeft,
    EnterRecovery,
    UsbFirst,
    DockFirst,
    BatteryFirst,
    FactoryReturn,
    Wait,
    Paused,
    Finished,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: u32,
    session: String,
    sequence: u64,
    step: Step,
    location: Option<u64>,
}
impl Request {
    fn validate(&self) -> Result<(), String> {
        if self.schema != 1
            || self.session.is_empty()
            || self.session.len() > 128
            || !self
                .session
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        {
            return Err("The startup test instructions are invalid. Waiting for new instructions.".into());
        }
        if matches!(
            self.step,
            Step::EnterRecovery
                | Step::UsbFirst
                | Step::DockFirst
                | Step::BatteryFirst
                | Step::FactoryReturn
                | Step::Paused
        ) && self.location.is_none()
        {
            return Err("Identify the left USB connection before starting the test.".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug)]
enum Phase {
    Initial,
    Unplug,
    OffWait(Instant),
    Bluetooth,
    StartWait(Instant),
    Reconnect,
    Done,
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Mode {
    SerialBootloader,
    MscBootloader,
    Factory,
    Other,
    Absent,
}
#[derive(Serialize)]
struct Status<'a> {
    schema: u32,
    session: &'a str,
    sequence: u64,
    step: Step,
    complete: bool,
    location: Option<u64>,
    observed_mode: Mode,
    usb_absence_confirmed: bool,
    switch_ack: bool,
    reconnect_ready: bool,
    observed_at_unix_ms: u128,
    observation_valid: bool,
}
pub struct TrialView {
    pub title: String,
    pub instruction: String,
    pub ack_label: Option<&'static str>,
    pub finished: bool,
}
pub struct Trial {
    directory: PathBuf,
    request: Request,
    phase: Phase,
    location: Option<u64>,
    mode: Mode,
    complete: bool,
    absence_confirmed: bool,
    switch_ack: bool,
    fresh: bool,
    problem: Option<String>,
}
impl Trial {
    pub fn available() -> bool {
        directory().is_ok_and(|path| path.join("request.json").is_file())
    }
    pub fn new() -> Result<Self, String> {
        let directory = directory()?;
        let request = read_request(&directory)?;
        Ok(Self::from_request(directory, request))
    }
    fn from_request(directory: PathBuf, request: Request) -> Self {
        Self {
            location: request.location,
            directory,
            request,
            phase: Phase::Initial,
            mode: Mode::Absent,
            complete: false,
            absence_confirmed: false,
            switch_ack: false,
            fresh: false,
            problem: None,
        }
    }
    fn accept_request(&mut self, request: Request) -> Result<(), String> {
        request.validate()?;
        if request.session == self.request.session {
            if request.sequence < self.request.sequence {
                return Err(
                    "The startup instructions are out of date. Waiting for new instructions.".into(),
                );
            }
            if request.sequence == self.request.sequence && request != self.request {
                return Err(
                    "The startup instructions changed during this step. Waiting for new instructions."
                        .into(),
                );
            }
        }
        if request != self.request {
            *self = Self::from_request(self.directory.clone(), request);
        }
        Ok(())
    }
    pub fn observe(&mut self, snapshot: Result<Snapshot, String>) {
        match read_request(&self.directory).and_then(|request| self.accept_request(request)) {
            Ok(()) => self.observe_at(snapshot, Instant::now()),
            Err(error) => self.invalidate(error),
        }
        self.publish_or_stop();
    }
    fn invalidate(&mut self, error: String) {
        self.phase = Phase::Initial;
        self.complete = false;
        self.absence_confirmed = false;
        self.switch_ack = false;
        self.fresh = false;
        self.problem = Some(error);
    }
    fn observe_at(&mut self, snapshot: Result<Snapshot, String>, now: Instant) {
        self.fresh = false;
        self.complete = false;
        let snapshot = match snapshot {
            Ok(snapshot) => snapshot,
            Err(_) => {
                self.invalidate(
                    "USB could not be checked. Keep the keyboard connected while we retry.".into(),
                );
                return;
            }
        };
        let bootloaders: Vec<_> = snapshot
            .devices
            .iter()
            .filter(|d| d.vendor == 0x239a && matches!(d.product, 0x29 | 0x2a))
            .collect();
        if bootloaders.len() > 1 || snapshot.mounts.len() > 1 {
            self.invalidate(
                "More than one recovery device is connected. Leave only the left half connected."
                    .into(),
            );
            return;
        }
        if self.request.step == Step::ConnectLeft {
            let candidates: Vec<_> = snapshot
                .devices
                .iter()
                .filter(|d| d.factory_left())
                .collect();
            if candidates.len() > 1 {
                self.invalidate(
                    "Leave the dongle disconnected and connect only the left half.".into(),
                );
                return;
            }
            if let Some(candidate) = candidates
                .first()
                .filter(|_| bootloaders.is_empty() && snapshot.mounts.is_empty())
            {
                if self
                    .request
                    .location
                    .is_some_and(|location| location != candidate.location)
                {
                    self.invalidate("Use the left half's original USB connection.".into());
                    return;
                }
                self.location = Some(candidate.location);
                self.mode = Mode::Factory;
                self.fresh = true;
                self.problem = None;
                self.complete = true;
                self.phase = Phase::Done;
            } else {
                self.mode = Mode::Absent;
                self.fresh = true;
                self.problem = None;
                self.phase = Phase::Initial;
            }
            return;
        }
        if matches!(self.request.step, Step::Wait | Step::Finished) {
            self.fresh = true;
            self.problem = None;
            self.complete = self.request.step == Step::Finished;
            return;
        }
        if snapshot
            .devices
            .iter()
            .any(|d| d.role() == Some(Role::Left) && Some(d.location) != self.location)
        {
            self.invalidate(
                "The left half is on a different USB connection. Reconnect it where it was.".into(),
            );
            return;
        }
        if bootloaders
            .first()
            .is_some_and(|d| Some(d.location) != self.location)
        {
            self.invalidate("The recovery device moved to a different USB connection. Reconnect the left half where it was.".into());
            return;
        }
        let present: Vec<_> = snapshot
            .devices
            .iter()
            .filter(|d| Some(d.location) == self.location)
            .collect();
        if present.len() > 1 {
            self.invalidate(
                "More than one recovery device matches this USB connection. Disconnect the other recovery devices.".into(),
            );
            return;
        }
        self.mode = match present.first() {
            Some(d) if d.vendor == 0x239a && d.product == 0x2a => Mode::SerialBootloader,
            Some(d) if d.bootloader() => Mode::MscBootloader,
            Some(d) if d.factory_left() => Mode::Factory,
            Some(_) => Mode::Other,
            None => Mode::Absent,
        };
        let valid_mount = snapshot.mounts.len() == 1
            && snapshot.mounts[0]
                .info
                .lines()
                .map(str::trim)
                .any(|line| line == "UF2 Bootloader 0.9.2-39-g0147d71")
            && snapshot.mounts[0]
                .info
                .lines()
                .map(str::trim)
                .any(|line| line == "Model: NocFree &");
        // An orphaned volume is not evidence that the selected USB device disappeared.
        if self.mode == Mode::Absent && !snapshot.mounts.is_empty() {
            self.invalidate(
                "Waiting for the recovery drive to disappear completely. Check the left USB cable."
                    .into(),
            );
            return;
        }
        self.fresh = true;
        self.problem = None;
        match self.request.step {
            Step::EnterRecovery => {
                self.complete = self.mode == Mode::MscBootloader && valid_mount;
                self.phase = if self.complete {
                    Phase::Done
                } else {
                    Phase::Initial
                };
                if self.mode == Mode::MscBootloader && !snapshot.mounts.is_empty() && !valid_mount {
                    self.invalidate("The recovery drive information is not recognized. Waiting for the drive check.".into());
                }
            }
            Step::UsbFirst | Step::DockFirst | Step::BatteryFirst | Step::FactoryReturn => {
                self.phase = match self.phase {
                    Phase::Unplug if self.mode == Mode::Absent => {
                        self.absence_confirmed = true;
                        Phase::OffWait(now)
                    }
                    Phase::OffWait(_) | Phase::Bluetooth | Phase::StartWait(_)
                        if self.mode != Mode::Absent =>
                    {
                        self.absence_confirmed = false;
                        self.switch_ack = false;
                        self.problem = Some(
                            "USB returned too soon. Start this switch-and-cable step again.".into(),
                        );
                        Phase::Initial
                    }
                    Phase::OffWait(since)
                        if now.saturating_duration_since(since) >= Duration::from_secs(5) =>
                    {
                        if self.request.step == Step::BatteryFirst {
                            Phase::Bluetooth
                        } else {
                            Phase::Reconnect
                        }
                    }
                    Phase::StartWait(since)
                        if now.saturating_duration_since(since) >= Duration::from_secs(10) =>
                    {
                        Phase::Reconnect
                    }
                    Phase::Reconnect | Phase::Done if self.mode != Mode::Absent => {
                        if self.request.step == Step::BatteryFirst
                            || matches!(self.request.step, Step::UsbFirst | Step::DockFirst)
                                && self.mode == Mode::MscBootloader
                                && valid_mount
                            || self.request.step == Step::FactoryReturn
                                && self.mode == Mode::Factory
                                && snapshot.mounts.is_empty()
                        {
                            self.complete = true;
                            Phase::Done
                        } else if matches!(self.request.step, Step::UsbFirst | Step::DockFirst)
                            && self.mode == Mode::MscBootloader
                            && snapshot.mounts.is_empty()
                        {
                            Phase::Reconnect
                        } else {
                            self.problem = Some(if self.request.step == Step::FactoryReturn {
                                "It did not return to normal operation. Start the switch-and-cable step again.".into()
                            } else {
                                "It did not return to the recovery drive. Start the switch-and-cable step again.".into()
                            });
                            self.absence_confirmed = false;
                            self.switch_ack = false;
                            Phase::Initial
                        }
                    }
                    Phase::Done => Phase::Reconnect,
                    phase => phase,
                };
            }
            Step::Wait | Step::Paused => {}
            Step::Finished => {
                self.complete = true;
                self.phase = Phase::Done;
            }
            Step::ConnectLeft => unreachable!(),
        }
    }
    pub fn acknowledge(&mut self) {
        match read_request(&self.directory).and_then(|request| self.accept_request(request)) {
            Ok(()) => self.acknowledge_at(Instant::now()),
            Err(error) => self.invalidate(error),
        }
        self.publish_or_stop();
    }
    fn acknowledge_at(&mut self, now: Instant) {
        if !self.fresh {
            return;
        }
        match self.phase {
            Phase::Initial
                if matches!(
                    self.request.step,
                    Step::UsbFirst | Step::DockFirst | Step::BatteryFirst | Step::FactoryReturn
                ) =>
            {
                self.phase = Phase::Unplug;
                self.complete = false;
                self.absence_confirmed = false;
                self.switch_ack = self.request.step != Step::BatteryFirst;
                self.problem = None;
            }
            Phase::Bluetooth if self.mode == Mode::Absent => {
                self.switch_ack = true;
                self.phase = Phase::StartWait(now);
            }
            _ => {}
        }
    }
    pub fn view(&self) -> TrialView {
        let wait = |since: Instant, seconds: u64| {
            Duration::from_secs(seconds)
                .saturating_sub(Instant::now().saturating_duration_since(since))
                .as_millis()
                .div_ceil(1000)
        };
        let (title, instruction, ack_label) = match (self.request.step, self.phase) {
            (Step::Finished, _) => (
                "Startup test finished".into(),
                "The startup test is complete.".into(),
                None,
            ),
            (Step::Paused, _) => (
                "Ready for the next test".into(),
                "Leave the left half connected. The next step will appear when ready.".into(),
                None,
            ),
            (Step::Wait, _) | (_, Phase::Done) => (
                "Checking the keyboard".into(),
                "Keep the USB cable connected while Companion checks the result.".into(),
                None,
            ),
            (Step::ConnectLeft, _) => (
                "Connect the left half".into(),
                "Connect it by USB in WIRED mode. Leave the dongle disconnected.".into(),
                None,
            ),
            (Step::EnterRecovery, _) => (
                "Hold Fn + 5".into(),
                "Keep WIRED selected. Hold Fn + 5 for five seconds, then release.".into(),
                None,
            ),
            (_, Phase::Initial) => (
                "Set the switch to WIRED".into(),
                "Use the middle position on the left half.".into(),
                self.fresh.then_some("It's in WIRED"),
            ),
            (Step::DockFirst, Phase::Unplug) => (
                "Waiting for the USB port".into(), "Keep the USB cable connected. The port will cycle automatically.".into(), None
            ),
            (Step::DockFirst, Phase::OffWait(since)) => (
                "The USB port is off".into(), format!("Keep the cable connected. Wait {} more seconds.", wait(since, 5)), None
            ),
            (_, Phase::Unplug) => (
                "Unplug the left half".into(),
                "Leave its switch in WIRED.".into(),
                None,
            ),
            (_, Phase::OffWait(since)) => (
                "Keep USB unplugged".into(),
                format!("Wait {} more seconds.", wait(since, 5)),
                None,
            ),
            (_, Phase::Bluetooth) => (
                "Switch to Bluetooth".into(),
                "Keep USB unplugged. Move the left switch to Bluetooth.".into(),
                (self.fresh && self.mode == Mode::Absent).then_some("It's in Bluetooth"),
            ),
            (_, Phase::StartWait(since)) => (
                "Let it start".into(),
                format!("Keep USB unplugged for {} more seconds.", wait(since, 10)),
                None,
            ),
            (_, Phase::Reconnect)
                if matches!(self.request.step, Step::UsbFirst | Step::DockFirst) && self.mode == Mode::MscBootloader =>
            {
                (
                    "Waiting for the recovery drive".into(),
                    "Keep USB connected while the drive appears.".into(),
                    None,
                )
            }
            (Step::DockFirst, Phase::Reconnect) => (
                "The USB port will restart".into(), "Keep the cable connected. The port will restart automatically.".into(), None
            ),
            (_, Phase::Reconnect) => (
                "Reconnect the left half".into(),
                "Plug its USB cable into the same port on your computer.".into(),
                None,
            ),
        };
        TrialView {
            title,
            instruction: if self.request.step == Step::DockFirst && self.problem.is_some() {
                "The USB port stopped restarting. Keep the cable connected while Companion checks the connection.".into()
            } else {
                self.problem.clone().unwrap_or(instruction)
            },
            ack_label,
            finished: self.request.step == Step::Finished,
        }
    }
    fn publish_or_stop(&mut self) {
        if let Err(error) = self.publish() {
            // Never leave a previous completed status available after a failed refresh.
            let _ = fs::remove_file(self.directory.join("status.json"));
            self.invalidate(error);
        }
    }
    fn reconnect_ready(&self) -> bool {
        matches!(
            self.request.step,
            Step::UsbFirst | Step::DockFirst | Step::BatteryFirst | Step::FactoryReturn
        ) && (matches!(self.phase, Phase::Reconnect)
            || matches!(self.phase, Phase::Done) && self.complete)
            && self.fresh
            && self.absence_confirmed
            && self.switch_ack
    }
    fn publish(&self) -> Result<(), String> {
        let status = Status {
            schema: 1,
            session: &self.request.session,
            sequence: self.request.sequence,
            step: self.request.step,
            complete: self.complete && self.fresh,
            location: self.location,
            observed_mode: self.mode,
            usb_absence_confirmed: self.absence_confirmed,
            switch_ack: self.switch_ack,
            reconnect_ready: self.reconnect_ready(),
            observation_valid: self.fresh,
            observed_at_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| "Could not record the test time.")?
                .as_millis(),
        };
        let bytes =
            serde_json::to_vec_pretty(&status).map_err(|_| "Could not save the test result.")?;
        write_status(&self.directory, &bytes)
    }
}
fn directory() -> Result<PathBuf, String> {
    Ok(crate::host_storage::application_root()?.join("startup-trial"))
}
fn read_request(directory: &Path) -> Result<Request, String> {
    let bytes = device::read_bounded(&directory.join("request.json"), 8192)?;
    let request: Request = serde_json::from_slice(&bytes)
        .map_err(|_| "Could not read the startup test instructions.")?;
    request.validate()?;
    Ok(request)
}
fn write_status(directory: &Path, bytes: &[u8]) -> Result<(), String> {
    crate::host_storage::check_directory(directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
            .map_err(|_| "Could not set access permissions for the test folder.")?;
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "Could not record the test time.")?
        .as_nanos();
    let temporary = directory.join(format!(".status-{}-{stamp}.partial", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    crate::host_storage::private_file_options(&mut options);
    let mut file = options
        .open(&temporary)
        .map_err(|_| "Could not save the startup test result.")?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Could not finish saving the startup test result.")?;
    fs::rename(&temporary, directory.join("status.json"))
        .map_err(|_| "Could not finish saving the startup test result.")?;
    crate::host_storage::sync_directory(directory)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{BootMount, Device};
    fn request(step: Step, sequence: u64) -> Request {
        Request {
            schema: 1,
            session: "test-session".into(),
            sequence,
            step,
            location: if step == Step::ConnectLeft {
                None
            } else {
                Some(7)
            },
        }
    }
    fn trial(step: Step) -> Trial {
        Trial::from_request(PathBuf::from("/private/fixture"), request(step, 1))
    }
    fn normal() -> Snapshot {
        Snapshot {
            devices: vec![Device {
                location: 7,
                vendor: 0x2886,
                product: 0x8029,
                name: "NocFree & ANSI".into(),
            }],
            mounts: vec![],
        }
    }
    fn boot(serial: bool, mounted: bool) -> Snapshot {
        Snapshot {
            devices: vec![Device {
                location: 7,
                vendor: 0x239a,
                product: if serial { 0x2a } else { 0x29 },
                name: "NocFree &".into(),
            }],
            mounts: if mounted {
                vec![BootMount {
                    path: PathBuf::from("/fixture/volume"),
                    info: "UF2 Bootloader 0.9.2-39-g0147d71\nModel: NocFree &\n".into(),
                }]
            } else {
                vec![]
            },
        }
    }
    #[test]
    fn connect_binds_only_one_normal_candidate_and_recovery_waits_for_mount() {
        let now = Instant::now();
        let mut t = trial(Step::ConnectLeft);
        t.observe_at(Ok(boot(false, true)), now);
        assert!(!t.complete);
        t.observe_at(Ok(normal()), now);
        assert!(t.complete);
        assert_eq!(t.location, Some(7));
        t.accept_request(request(Step::EnterRecovery, 2)).unwrap();
        assert!(!t.complete);
        t.observe_at(Ok(boot(false, false)), now);
        assert!(!t.complete);
        t.observe_at(Ok(boot(false, true)), now);
        assert!(t.complete);
        let mut ambiguous = boot(false, true);
        let mut other = ambiguous.devices[0].clone();
        other.location = 9;
        ambiguous.devices.push(other);
        t.observe_at(Ok(ambiguous), now);
        assert!(!t.complete);
        assert!(!t.fresh);
    }
    #[test]
    fn usb_first_counts_only_observed_absence_and_requires_msc() {
        let now = Instant::now();
        let mut t = trial(Step::UsbFirst);
        t.acknowledge_at(now);
        assert!(matches!(t.phase, Phase::Initial));
        t.observe_at(Ok(normal()), now);
        t.acknowledge_at(now);
        t.observe_at(Ok(normal()), now + Duration::from_secs(100));
        assert!(matches!(t.phase, Phase::Unplug));
        assert!(!t.absence_confirmed);
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(101));
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(105));
        assert!(matches!(t.phase, Phase::OffWait(_)));
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(106));
        assert!(matches!(t.phase, Phase::Reconnect));
        t.observe_at(Ok(boot(false, false)), now + Duration::from_secs(107));
        assert!(matches!(t.phase, Phase::Reconnect));
        assert!(!t.complete);
        t.observe_at(Ok(boot(false, true)), now + Duration::from_secs(108));
        assert!(t.complete && t.absence_confirmed && t.switch_ack);
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(109));
        assert!(!t.complete);
    }
    #[test]
    fn battery_start_requires_bluetooth_ack_then_full_ten_seconds() {
        let now = Instant::now();
        let mut t = trial(Step::BatteryFirst);
        t.observe_at(Ok(boot(false, true)), now);
        t.acknowledge_at(now);
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(1));
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(6));
        assert!(matches!(t.phase, Phase::Bluetooth));
        assert!(!t.switch_ack);
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(100));
        assert!(matches!(t.phase, Phase::Bluetooth));
        t.acknowledge_at(now + Duration::from_secs(100));
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(109));
        assert!(matches!(t.phase, Phase::StartWait(_)));
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(110));
        assert!(matches!(t.phase, Phase::Reconnect));
        t.observe_at(Ok(boot(true, false)), now + Duration::from_secs(111));
        assert!(t.complete && t.switch_ack && t.absence_confirmed);
        assert_eq!(t.mode, Mode::SerialBootloader);
        // Completing the observation reports a mode, not whether the firmware passed.
        let mut next = request(Step::BatteryFirst, 2);
        next.session = "fresh-session".into();
        t.accept_request(next).unwrap();
        assert!(!t.complete && !t.absence_confirmed && !t.switch_ack);
    }
    #[test]
    fn early_reconnect_wrong_connection_and_errors_reset_evidence() {
        let now = Instant::now();
        let mut t = trial(Step::BatteryFirst);
        t.observe_at(Ok(normal()), now);
        t.acknowledge_at(now);
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(1));
        t.observe_at(Ok(normal()), now + Duration::from_secs(2));
        assert!(matches!(t.phase, Phase::Initial));
        assert!(!t.absence_confirmed && !t.complete);
        let mut wrong = boot(false, true);
        wrong.devices[0].location = 99;
        t.observe_at(Ok(wrong), now);
        assert!(!t.fresh && !t.complete);
        t.acknowledge_at(now);
        assert!(matches!(t.phase, Phase::Initial));
        t.observe_at(Ok(normal()), now);
        t.acknowledge_at(now);
        t.observe_at(Ok(Snapshot::default()), now);
        t.observe_at(Err("unavailable".into()), now + Duration::from_secs(5));
        assert!(!t.complete && !t.absence_confirmed && !t.fresh);
    }
    #[test]
    fn restart_and_new_sequence_do_not_reuse_timers_and_old_sequences_reject() {
        let now = Instant::now();
        let mut t = trial(Step::UsbFirst);
        t.observe_at(Ok(normal()), now);
        t.acknowledge_at(now);
        t.observe_at(Ok(Snapshot::default()), now);
        t.accept_request(request(Step::UsbFirst, 2)).unwrap();
        assert!(matches!(t.phase, Phase::Initial));
        assert!(!t.fresh && !t.absence_confirmed);
        assert!(t.accept_request(request(Step::UsbFirst, 1)).is_err());
        assert_eq!(t.request.sequence, 2);
        let restarted = Trial::from_request(PathBuf::from("/private/fixture"), t.request.clone());
        assert!(matches!(restarted.phase, Phase::Initial));
        assert!(!restarted.fresh && !restarted.complete);
        let mut modified = t.request.clone();
        modified.step = Step::Finished;
        assert!(t.accept_request(modified).is_err());
    }
    #[test]
    fn protocol_rejects_unknown_steps_missing_connection_and_instructions() {
        assert!(
            serde_json::from_str::<Request>(
                r#"{"schema":1,"session":"test","sequence":1,"step":"flash","location":7}"#
            )
            .is_err()
        );
        assert!(serde_json::from_str::<Request>(r#"{"schema":1,"session":"test","sequence":1,"step":"wait","instruction":"run command"}"#).is_err());
        let mut missing = request(Step::BatteryFirst, 1);
        missing.location = None;
        assert!(missing.validate().is_err());
        let mut invalid = request(Step::ConnectLeft, 1);
        invalid.session = "../escape".into();
        assert!(invalid.validate().is_err());
    }
    #[test]
    fn factory_return_requires_timed_absence_and_same_port_normal_without_mount() {
        let now = Instant::now();
        let mut t = trial(Step::FactoryReturn);
        let mut invalid = request(Step::FactoryReturn, 1);
        invalid.location = None;
        assert!(invalid.validate().is_err());
        t.observe_at(Ok(boot(false, true)), now);
        t.acknowledge_at(now);
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(1));
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(5));
        assert!(!t.complete);
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(6));
        assert!(matches!(t.phase, Phase::Reconnect));
        t.observe_at(Ok(boot(false, true)), now + Duration::from_secs(7));
        assert!(!t.complete);
        assert!(matches!(t.phase, Phase::Initial));
        t.acknowledge_at(now + Duration::from_secs(7));
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(8));
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(13));
        let mut with_mount = normal();
        with_mount.mounts = boot(false, true).mounts;
        t.observe_at(Ok(with_mount), now + Duration::from_secs(14));
        assert!(!t.complete);
        t.acknowledge_at(now + Duration::from_secs(14));
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(15));
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(20));
        let mut wrong = normal();
        wrong.devices[0].location = 99;
        t.observe_at(Ok(wrong), now + Duration::from_secs(21));
        assert!(!t.complete && !t.fresh);
        t.observe_at(Ok(normal()), now + Duration::from_secs(22));
        t.acknowledge_at(now + Duration::from_secs(22));
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(23));
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(28));
        t.observe_at(Ok(normal()), now + Duration::from_secs(29));
        assert!(t.complete && t.absence_confirmed && t.switch_ack);
        assert_eq!(t.mode, Mode::Factory);
    }
    #[test]
    fn paused_stays_live_without_completion_or_acknowledgment() {
        let now = Instant::now();
        let mut t = trial(Step::Paused);
        t.observe_at(Ok(boot(true, false)), now);
        assert!(t.fresh);
        assert_eq!(t.mode, Mode::SerialBootloader);
        assert!(!t.complete);
        let view = t.view();
        assert_eq!(view.title, "Ready for the next test");
        assert!(!view.finished);
        assert!(view.ack_label.is_none());
        t.acknowledge_at(now + Duration::from_secs(100));
        assert!(!t.complete && !t.switch_ack && !t.absence_confirmed);
        t.accept_request(request(Step::BatteryFirst, 2)).unwrap();
        assert!(!t.fresh && !t.complete);
        assert!(matches!(t.phase, Phase::Initial));
        t.observe_at(Err("USB unavailable".into()), now);
        assert!(!t.fresh && !t.complete);
    }
    #[test]
    fn paused_schema_requires_a_bound_connection_and_does_not_mean_finished() {
        let parsed: Request = serde_json::from_str(
            r#"{"schema":1,"session":"test","sequence":2,"step":"paused","location":7}"#,
        )
        .unwrap();
        assert!(parsed.validate().is_ok());
        assert_eq!(parsed.step, Step::Paused);
        let missing: Request =
            serde_json::from_str(r#"{"schema":1,"session":"test","sequence":2,"step":"paused"}"#)
                .unwrap();
        assert!(missing.validate().is_err());
    }
    #[test]
    fn reconnect_ready_requires_the_full_guide_not_just_usb_absence() {
        let now = Instant::now();
        for step in [Step::UsbFirst, Step::DockFirst, Step::FactoryReturn] {
            let mut t = trial(step);
            assert!(!t.reconnect_ready());
            t.observe_at(Ok(normal()), now);
            t.acknowledge_at(now);
            t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(1));
            assert!(t.absence_confirmed);
            assert!(!t.reconnect_ready());
            t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(5));
            assert!(!t.reconnect_ready());
            t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(6));
            assert!(t.reconnect_ready());
            t.observe_at(Err("USB unavailable".into()), now + Duration::from_secs(7));
            assert!(!t.reconnect_ready());
        }
        let mut battery = trial(Step::BatteryFirst);
        battery.observe_at(Ok(normal()), now);
        battery.acknowledge_at(now);
        battery.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(1));
        battery.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(6));
        assert!(!battery.reconnect_ready());
        battery.acknowledge_at(now + Duration::from_secs(6));
        battery.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(15));
        assert!(!battery.reconnect_ready());
        battery.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(16));
        assert!(battery.reconnect_ready());
        battery.observe_at(Ok(boot(true, false)), now + Duration::from_secs(17));
        assert!(battery.complete && battery.reconnect_ready());
        for step in [Step::Wait, Step::Paused] {
            battery
                .accept_request(request(step, battery.request.sequence + 1))
                .unwrap();
            battery.observe_at(Ok(boot(true, false)), now + Duration::from_secs(18));
            assert!(!battery.reconnect_ready());
        }
    }
    #[test]
    fn dock_first_keeps_the_cable_connected_but_requires_real_timed_usb_absence() {
        let now = Instant::now();
        let mut t = trial(Step::DockFirst);
        let mut unbound = request(Step::DockFirst, 1);
        unbound.location = None;
        assert!(unbound.validate().is_err());
        t.observe_at(Ok(boot(false, true)), now);
        t.acknowledge_at(now);
        assert!(
            t.view()
                .instruction
                .contains("Keep the USB cable connected")
        );
        t.observe_at(Ok(boot(false, true)), now + Duration::from_secs(100));
        assert!(!t.absence_confirmed && !t.reconnect_ready() && !t.complete);
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(101));
        assert!(t.view().instruction.contains("Keep the cable connected"));
        assert!(!t.reconnect_ready());
        t.observe_at(Ok(Snapshot::default()), now + Duration::from_secs(106));
        assert!(t.reconnect_ready());
        assert!(t.view().instruction.contains("restart automatically"));
        t.observe_at(Ok(boot(false, false)), now + Duration::from_secs(107));
        assert!(!t.complete);
        t.observe_at(Ok(boot(false, true)), now + Duration::from_secs(108));
        assert!(t.complete && t.reconnect_ready());
        t.observe_at(
            Err("USB unavailable".into()),
            now + Duration::from_secs(109),
        );
        assert!(!t.complete && !t.reconnect_ready());
        assert!(t.view().instruction.contains("Keep the cable connected"));
        let parsed: Request = serde_json::from_str(
            r#"{"schema":1,"session":"test","sequence":2,"step":"dock_first","location":7}"#,
        )
        .unwrap();
        assert!(parsed.validate().is_ok());
    }
    #[test]
    fn rmk_left_never_attests_factory_identification_or_restoration() {
        let now = Instant::now();
        let mut rmk = normal();
        rmk.devices[0].vendor = 0x4c4b;
        rmk.devices[0].product = 0x4643;
        rmk.devices[0].name = "NocFree RMK".into();
        let mut connect = trial(Step::ConnectLeft);
        connect.observe_at(Ok(rmk.clone()), now);
        assert!(!connect.complete);
        assert_ne!(connect.mode, Mode::Factory);
        let mut returning = trial(Step::FactoryReturn);
        returning.phase = Phase::Reconnect;
        returning.absence_confirmed = true;
        returning.switch_ack = true;
        returning.observe_at(Ok(rmk), now);
        assert_eq!(returning.mode, Mode::Other);
        assert!(!returning.complete);
        let mut factory = trial(Step::FactoryReturn);
        factory.phase = Phase::Reconnect;
        factory.absence_confirmed = true;
        factory.switch_ack = true;
        factory.observe_at(Ok(normal()), now);
        assert_eq!(factory.mode, Mode::Factory);
        assert!(factory.complete);
    }
}

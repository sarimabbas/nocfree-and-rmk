//! Read producer-owned USB snapshots or RMK's standard Bluetooth Battery Services.
#[path = "battery_vial.rs"]
pub(crate) mod vial;
use rynk::RynkDevice;
use rynk::rmk_types::battery::BatteryStatus;
use rynk_usb::UsbDevice;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{OnceLock, mpsc};
use std::time::{Duration, Instant};
use tokio::time::timeout;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Telemetry {
    pub(crate) flags: u8,
    pub(crate) known: u8,
}
impl Telemetry {
    pub(crate) fn has(self, bit: u8) -> bool {
        self.flags & (1 << bit) != 0
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Readings {
    pub(crate) left: BatteryStatus,
    pub(crate) right: BatteryStatus,
    pub(crate) right_connected: bool,
    pub(crate) right_link_known: bool,
    pub(crate) left_mode: Option<crate::device_status::Mode>,
    pub(crate) telemetry: Option<Telemetry>,
}

/// Last valid percentages survive transport changes; visibility follows connection.
#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct Levels {
    pub left: Option<u8>,
    pub right: Option<u8>,
}
impl Levels {
    pub(crate) fn observe(&mut self, readings: Readings) {
        let level = |status| match status {
            BatteryStatus::Available {
                level: Some(value), ..
            } if value <= 100 => Some(value),
            _ => None,
        };
        if let Some(value) = level(readings.left) {
            self.left = Some(value);
        }
        if let Some(value) = level(readings.right) {
            self.right = Some(value);
        }
    }
    pub(crate) fn visible(self, left_connected: bool, right_connected: bool) -> Self {
        Self {
            left: self.left.filter(|_| left_connected),
            right: self.right.filter(|_| right_connected),
        }
    }
}

impl Readings {
    pub(crate) fn known_levels(self) -> [bool; 2] {
        let known = |status| {
            matches!(
                status,
                BatteryStatus::Available {
                    level: Some(0..=100),
                    ..
                }
            )
        };
        [known(self.left), known(self.right)]
    }

    pub(crate) fn retain_for_usb_change(
        mut self,
        before: &crate::device_status::UsbKey,
        after: &crate::device_status::UsbKey,
    ) -> Option<Self> {
        if crate::device_status::battery_source(before)
            != crate::device_status::battery_source(after)
        {
            return None;
        }
        if crate::device_status::right_usb(before) != crate::device_status::right_usb(after) {
            self.right_connected = false;
            self.right_link_known = false;
            self.right = BatteryStatus::Unavailable;
        }
        Some(self)
    }
}

static QUERY_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
struct QueryGuard;
impl Drop for QueryGuard {
    fn drop(&mut self) {
        QUERY_IN_FLIGHT.store(false, Ordering::Release);
    }
}
impl QueryGuard {
    fn acquire() -> Result<Self, String> {
        QUERY_IN_FLIGHT
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| "The previous battery check is still finishing.".to_owned())
    }
}
pub(crate) fn read(via_bluetooth: bool) -> Result<Readings, String> {
    let guard = QueryGuard::acquire()?;
    let started = Instant::now();
    let (send, receive) = mpsc::sync_channel(1);
    battery_worker()?
        .send(Box::new(move || {
            // Keep the single-flight lease on the native worker, even if the UI
            // stops waiting while an OS call is still finishing.
            let _guard = guard;
            let _ = send.send(if via_bluetooth {
                read_bluetooth()
            } else {
                read_on_worker(started)
            });
        }))
        .map_err(|_| "The keyboard battery worker stopped.".to_owned())?;
    receive
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| "The battery check timed out. Details are unavailable.".to_owned())?
}

type BatteryTask = Box<dyn FnOnce() + Send>;

fn start_worker() -> Result<mpsc::SyncSender<BatteryTask>, String> {
    let (send, receive) = mpsc::sync_channel::<BatteryTask>(1);
    std::thread::Builder::new()
        .name("nocfree-battery".into())
        .spawn(move || {
            while let Ok(task) = receive.recv() {
                task();
            }
        })
        .map_err(|_| "Couldn't start the keyboard battery worker.".to_owned())?;
    Ok(send)
}

fn battery_worker() -> Result<&'static mpsc::SyncSender<BatteryTask>, String> {
    // macOS HIDAPI attaches its process-global manager to the first caller's
    // CFRunLoop. That thread must outlive every HID handle. GPUI's temporary
    // background threads cannot own it; this sender keeps one worker alive
    // until process exit. Every native HID handle is also dropped on it.
    static WORKER: OnceLock<Result<mpsc::SyncSender<BatteryTask>, String>> = OnceLock::new();
    WORKER
        .get_or_init(start_worker)
        .as_ref()
        .map_err(Clone::clone)
}

pub(crate) const NATIVE_BUSY: &str = "The keyboard connection is busy.";

/// All native HID users share the permanent owner thread and bounded host deadline.
pub(crate) fn native_task<T: Send + 'static>(
    task: impl FnOnce(Instant) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let started = Instant::now();
    let guard = loop {
        if let Ok(guard) = QueryGuard::acquire() {
            break guard;
        }
        if started.elapsed() >= Duration::from_secs(2) {
            return Err(NATIVE_BUSY.to_owned());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let (send, receive) = mpsc::sync_channel(1);
    battery_worker()?
        .try_send(Box::new(move || {
            let _guard = guard;
            let _ = send.send(task(started));
        }))
        .map_err(|error| match error {
            mpsc::TrySendError::Full(_) => NATIVE_BUSY.to_owned(),
            mpsc::TrySendError::Disconnected(_) => {
                "The keyboard connection worker stopped.".to_owned()
            }
        })?;
    receive
        .recv_timeout(Duration::from_secs(5).saturating_sub(started.elapsed()))
        .map_err(|_| {
            "The keyboard request timed out. Check its state before continuing.".to_owned()
        })?
}

fn read_on_worker(started: Instant) -> Result<Readings, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "Couldn't start the keyboard connection.".to_owned())?;
    runtime.block_on(read_usb(started))
}

#[cfg(target_os = "macos")]
fn read_bluetooth() -> Result<Readings, String> {
    use rynk::rmk_types::battery::ChargeState;
    use std::process::Command;

    let helper = std::env::current_exe()
        .map_err(|_| "Couldn't locate the Bluetooth battery reader.")?
        .with_file_name("nocfree-battery-bluetooth");
    let output = Command::new(helper)
        .output()
        .map_err(|_| "Couldn't start the Bluetooth battery reader.")?;
    if !output.status.success() || output.stdout.len() > 256 {
        return Err("Couldn't read the Bluetooth batteries.".into());
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Levels {
        left: Option<u8>,
        right: Option<u8>,
    }
    let levels: Levels = serde_json::from_slice(&output.stdout)
        .map_err(|_| "The Bluetooth battery reply was invalid.")?;
    let level = |value: Option<u8>| -> Result<BatteryStatus, String> {
        match value {
            Some(0..=100) => Ok(BatteryStatus::Available {
                level: value,
                charge_state: ChargeState::Unknown,
            }),
            Some(_) => Err("The Bluetooth battery level was invalid.".into()),
            None => Ok(BatteryStatus::Unavailable),
        }
    };
    let left = level(levels.left)?;
    let right = level(levels.right)?;
    if matches!(left, BatteryStatus::Unavailable) {
        return Err("The keyboard's Bluetooth battery service is unavailable.".into());
    }
    Ok(Readings {
        left,
        right,
        right_connected: !matches!(right, BatteryStatus::Unavailable),
        right_link_known: false,
        left_mode: Some(crate::device_status::Mode::Bluetooth),
        telemetry: None,
    })
}

#[cfg(not(target_os = "macos"))]
fn read_bluetooth() -> Result<Readings, String> {
    Err("Bluetooth battery reading is unavailable on this system.".into())
}

async fn read_usb(started: Instant) -> Result<Readings, String> {
    let devices = timeout(Duration::from_secs(5), UsbDevice::discover())
        .await
        .map_err(|_| "Looking for the keyboard took too long.".to_owned())?
        .map_err(|_| "Couldn't look for the keyboard.".to_owned())?;
    let mut selected: Vec<_> = devices
        .into_iter()
        .filter(|device| device.label() == "NocFree RMK")
        .collect();
    if selected.is_empty() {
        return vial::read(started);
    }
    if selected.len() != 1 {
        return Err("Connect only one NocFree RMK keyboard to check its batteries.".to_owned());
    }
    let selected_id = selected[0].id();
    // Handshake reads only protocol version and capabilities. The vendor session
    // is dropped after the getters; keyboard HID remains independently usable.
    let (client, mut driver) = timeout(Duration::from_secs(8), selected.remove(0).connect())
        .await
        .map_err(|_| "The keyboard didn't respond in time.".to_owned())?
        .map_err(|_| "Couldn't connect to this keyboard's battery service.".to_owned())?;
    let readings = async {
        let left = timeout(Duration::from_secs(5), client.get_battery_status())
            .await
            .map_err(|_| "The left battery didn't respond in time.".to_owned())?
            .map_err(|_| "Couldn't read the left battery.".to_owned())?;
        let right = timeout(Duration::from_secs(5), client.get_peripheral_status(0))
            .await
            .map_err(|_| "The right battery didn't respond in time.".to_owned())?
            .map_err(|_| "Couldn't read the right battery.".to_owned())?;
        convert(left, right.battery, right.connected)
    };
    let result = tokio::select! {
        _ = driver.run(&client) => Err("The keyboard connection ended before the battery check finished.".to_owned()),
        result = readings => result,
    }?;
    // A USB reconnect can reuse its port and product name. Require the same
    // enumerated connection, not just the same physical location.
    let fresh = timeout(Duration::from_secs(5), UsbDevice::discover())
        .await
        .map_err(|_| "Checking the keyboard connection took too long.".to_owned())?
        .map_err(|_| "Couldn't confirm the keyboard connection.".to_owned())?;
    if !same_connection(
        &selected_id,
        fresh
            .into_iter()
            .filter(|device| device.label() == "NocFree RMK")
            .map(|device| device.id()),
    ) {
        return Err("The keyboard connection changed during the battery check.".to_owned());
    }
    Ok(result)
}

/// The host deadline bounds observation, even when a native call completes
/// between UI ticks. A completed result after expiry is still discarded.
pub(crate) fn observation_result(
    started: Instant,
    now: Instant,
    result: Option<Result<Readings, String>>,
) -> Option<Result<Readings, String>> {
    if now.saturating_duration_since(started) >= Duration::from_secs(5) {
        Some(Err(
            "The battery check timed out. Details are unavailable.".into()
        ))
    } else {
        result
    }
}

fn same_connection<T: PartialEq>(expected: &T, observed: impl IntoIterator<Item = T>) -> bool {
    let mut observed = observed.into_iter();
    observed.next().as_ref() == Some(expected) && observed.next().is_none()
}

fn validate(status: BatteryStatus) -> Result<BatteryStatus, String> {
    match status {
        BatteryStatus::Available {
            level: Some(value), ..
        } if value > 100 => Err("The keyboard reported an invalid battery level.".to_owned()),
        _ => Ok(status),
    }
}

fn convert(
    left: BatteryStatus,
    right: BatteryStatus,
    right_connected: bool,
) -> Result<Readings, String> {
    let left = validate(left)?;
    // RMK retains the last peripheral snapshot when it disconnects. Do not
    // present that cached value as a current reading.
    let right = if right_connected {
        validate(right)?
    } else {
        BatteryStatus::Unavailable
    };
    Ok(Readings {
        left,
        right,
        right_connected,
        right_link_known: true,
        left_mode: None,
        telemetry: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rynk::rmk_types::battery::ChargeState;

    fn available(value: Option<u8>) -> BatteryStatus {
        BatteryStatus::Available {
            charge_state: ChargeState::Unknown,
            level: value,
        }
    }

    #[test]
    fn last_valid_percentages_survive_mode_change_and_hide_only_when_disconnected() {
        let mut levels = Levels::default();
        levels.observe(convert(available(Some(100)), available(Some(75)), true).unwrap());
        levels.observe(Readings {
            left: BatteryStatus::Unavailable,
            right: BatteryStatus::Unavailable,
            right_connected: false,
            right_link_known: false,
            left_mode: None,
            telemetry: None,
        });
        assert_eq!(levels.visible(true, true).left, Some(100));
        assert_eq!(levels.visible(true, true).right, Some(75));
        assert_eq!(levels.visible(false, false).left, None);
        assert_eq!(levels.visible(false, false).right, None);
        assert_eq!(levels.visible(true, true).right, Some(75));
        levels.observe(convert(available(Some(99)), available(None), true).unwrap());
        assert_eq!(levels.visible(true, true).left, Some(99));
        assert_eq!(levels.visible(true, true).right, Some(75));
    }
    #[test]
    fn expired_receiver_values_do_not_look_live() {
        let current = convert(available(Some(91)), available(Some(83)), true).unwrap();
        assert_eq!(current.known_levels(), [true, true]);
        let expired = convert(
            BatteryStatus::Unavailable,
            BatteryStatus::Unavailable,
            false,
        )
        .unwrap();
        assert_eq!(expired.known_levels(), [false, false]);
    }
    #[test]
    fn right_recovery_preserves_fresh_left_reading_but_not_right() {
        let normal = vec![
            (1, 0x4c4b, 0x4643, "NocFree RMK".into()),
            (2, 0x4c4b, 0x4671, "NocFree RMK Right".into()),
        ];
        let recovery = vec![normal[0].clone(), (2, 0x239a, 0x0029, "NocFree &".into())];
        let readings = convert(available(Some(100)), available(Some(75)), true).unwrap();
        let retained = readings.retain_for_usb_change(&normal, &recovery).unwrap();
        assert_eq!(retained.left, available(Some(100)));
        assert_eq!(retained.right, BatteryStatus::Unavailable);
        assert!(!retained.right_connected);
        assert!(
            readings
                .retain_for_usb_change(&normal, &recovery[1..].to_vec())
                .is_none()
        );
    }
    #[test]
    fn successive_short_lived_callers_share_one_living_native_worker() {
        let worker = start_worker().unwrap();
        let mut owner = None;
        for _ in 0..32 {
            let sender = worker.clone();
            let observed = std::thread::spawn(move || {
                let (reply, result) = mpsc::sync_channel(1);
                sender
                    .send(Box::new(move || {
                        reply.send(std::thread::current().id()).unwrap();
                    }))
                    .unwrap();
                result.recv_timeout(Duration::from_secs(1)).unwrap()
            })
            .join()
            .unwrap();
            assert_ne!(observed, std::thread::current().id());
            if let Some(expected) = owner {
                assert_eq!(observed, expected);
            } else {
                owner = Some(observed);
            }
        }
    }

    #[test]
    fn only_the_same_unique_usb_connection_accepts_a_snapshot() {
        assert!(same_connection(&1_u8, [1]));
        assert!(!same_connection(&1_u8, [2]));
        assert!(!same_connection(&1_u8, [1, 2]));
        assert!(!same_connection(&1_u8, [1, 1]));
        assert!(!same_connection(&1_u8, []));
    }

    #[test]
    fn completed_results_after_deadline_are_discarded_as_well_as_pending_results() {
        let started = Instant::now();
        let readings = convert(available(Some(42)), BatteryStatus::Unavailable, false).unwrap();
        assert!(observation_result(started, started + Duration::from_secs(2), None).is_none());
        assert!(
            observation_result(
                started,
                started + Duration::from_secs(2),
                Some(Ok(readings))
            )
            .unwrap()
            .is_ok()
        );
        assert!(
            observation_result(
                started,
                started + Duration::from_secs(5),
                Some(Ok(readings))
            )
            .unwrap()
            .is_err()
        );
        assert!(
            observation_result(started, started + Duration::from_secs(5), None)
                .unwrap()
                .is_err()
        );
    }

    #[test]
    fn an_unfinished_native_query_prevents_a_second_worker_until_it_exits() {
        let first = QueryGuard::acquire().unwrap();
        let worker = start_worker().unwrap();
        let (finish, wait) = mpsc::sync_channel(0);
        let (result, observation) = mpsc::sync_channel::<()>(1);
        let (exited, exit) = mpsc::sync_channel(1);
        worker
            .send(Box::new(move || {
                {
                    let _guard = first;
                    wait.recv().unwrap();
                    let _ = result.send(());
                }
                exited.send(()).unwrap();
            }))
            .unwrap();
        assert!(observation.recv_timeout(Duration::from_millis(10)).is_err());
        drop(observation);
        assert!(QueryGuard::acquire().is_err());
        finish.send(()).unwrap();
        exit.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(QueryGuard::acquire().is_ok());
    }

    #[test]
    fn physical_connection_role_and_port_must_all_remain_unique() {
        let expected = (7_u8, "left", 10_u32);
        assert!(same_connection(&expected, [expected]));
        for changed in [(8, "left", 10), (7, "receiver", 10), (7, "left", 11)] {
            assert!(!same_connection(&expected, [changed]));
        }
        assert!(!same_connection(&expected, [expected, expected]));
    }

    #[test]
    fn disconnected_peripheral_does_not_display_cached_level() {
        let readings = convert(available(Some(12)), available(Some(2)), false).unwrap();
        assert_eq!(readings.left, available(Some(12)));
        assert_eq!(readings.right, BatteryStatus::Unavailable);
        assert!(!readings.right_connected);
    }

    #[test]
    fn unavailable_and_unknown_levels_stay_unknown() {
        let readings = convert(BatteryStatus::Unavailable, available(None), true).unwrap();
        assert_eq!(readings.left, BatteryStatus::Unavailable);
        assert_eq!(readings.right, available(None));
        assert!(readings.right_connected);
    }

    #[test]
    fn zero_is_a_valid_battery_level() {
        let readings = convert(available(Some(0)), available(Some(100)), true).unwrap();
        assert_eq!(readings.left, available(Some(0)));
        assert_eq!(readings.right, available(Some(100)));
    }

    #[test]
    fn invalid_live_levels_are_rejected() {
        assert!(convert(available(Some(101)), available(Some(2)), true).is_err());
        assert!(convert(available(Some(12)), available(Some(255)), true).is_err());
    }

    #[test]
    fn producer_charging_metadata_survives_host_conversion() {
        let charging = BatteryStatus::Available {
            charge_state: ChargeState::Charging,
            level: Some(12),
        };
        let readings = convert(charging, BatteryStatus::Unavailable, false).unwrap();
        assert_eq!(readings.left, charging);
    }

    #[test]
    fn charging_state_survives_when_percentage_is_not_available_yet() {
        let charging = BatteryStatus::Available {
            charge_state: ChargeState::Charging,
            level: None,
        };
        assert_eq!(
            convert(charging, BatteryStatus::Unavailable, false)
                .unwrap()
                .left,
            charging
        );
    }

    #[test]
    fn disconnected_peripheral_does_not_retain_cached_charging_state() {
        let charging = BatteryStatus::Available {
            charge_state: ChargeState::Charging,
            level: Some(20),
        };
        assert_eq!(
            convert(available(Some(12)), charging, false).unwrap().right,
            BatteryStatus::Unavailable
        );
    }
}

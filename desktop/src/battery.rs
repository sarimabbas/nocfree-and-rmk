//! Read producer-owned snapshots through Vial custom GET or legacy Rynk getters.
#[path = "battery_vial.rs"]
mod vial;
use rynk::RynkDevice;
use rynk::rmk_types::battery::BatteryStatus;
use rynk_usb::UsbDevice;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::time::timeout;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Readings {
    pub(crate) left: BatteryStatus,
    pub(crate) right: BatteryStatus,
    pub(crate) right_connected: bool,
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
pub(crate) fn read() -> Result<Readings, String> {
    let _guard = QueryGuard::acquire()?;
    let started = Instant::now();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "Couldn't start the keyboard connection.".to_owned())?;
    runtime.block_on(read_usb(started))
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
        assert!(QueryGuard::acquire().is_err());
        drop(first);
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

//! Read producer-owned battery snapshots through RMK's read-only USB commands.
use rynk::RynkDevice;
use rynk::rmk_types::battery::BatteryStatus;
use rynk_usb::UsbDevice;
use std::time::Duration;
use tokio::time::timeout;

pub(crate) struct Readings {
    pub(crate) left: Option<u8>,
    pub(crate) right: Option<u8>,
    pub(crate) right_connected: bool,
}

pub(crate) fn read() -> Result<Readings, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "Couldn't start the keyboard connection.".to_owned())?;
    runtime.block_on(read_usb())
}

async fn read_usb() -> Result<Readings, String> {
    let devices = timeout(Duration::from_secs(5), UsbDevice::discover())
        .await
        .map_err(|_| "Looking for the keyboard took too long.".to_owned())?
        .map_err(|_| "Couldn't look for the keyboard.".to_owned())?;
    let mut selected: Vec<_> = devices
        .into_iter()
        .filter(|device| device.label() == "NocFree RMK")
        .collect();
    if selected.len() != 1 {
        return Err(if selected.is_empty() {
            "Connect the left half by USB to check both batteries.".to_owned()
        } else {
            "Connect only one NocFree RMK keyboard to check its batteries.".to_owned()
        });
    }
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
    tokio::select! {
        _ = driver.run(&client) => Err("The keyboard connection ended before the battery check finished.".to_owned()),
        result = readings => result,
    }
}

fn level(status: BatteryStatus) -> Result<Option<u8>, String> {
    match status {
        BatteryStatus::Available {
            level: Some(value), ..
        } if value > 100 => Err("The keyboard reported an invalid battery level.".to_owned()),
        BatteryStatus::Available { level, .. } => Ok(level),
        BatteryStatus::Unavailable => Ok(None),
    }
}

fn convert(
    left: BatteryStatus,
    right: BatteryStatus,
    right_connected: bool,
) -> Result<Readings, String> {
    let left = level(left)?;
    // RMK retains the last peripheral snapshot when it disconnects. Do not
    // present that cached value as a current reading.
    let right = if right_connected { level(right)? } else { None };
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
    fn disconnected_peripheral_does_not_display_cached_level() {
        let readings = convert(available(Some(12)), available(Some(2)), false).unwrap();
        assert_eq!(readings.left, Some(12));
        assert_eq!(readings.right, None);
        assert!(!readings.right_connected);
    }

    #[test]
    fn unavailable_and_unknown_levels_stay_unknown() {
        let readings = convert(BatteryStatus::Unavailable, available(None), true).unwrap();
        assert_eq!(readings.left, None);
        assert_eq!(readings.right, None);
        assert!(readings.right_connected);
    }

    #[test]
    fn zero_is_a_valid_battery_level() {
        let readings = convert(available(Some(0)), available(Some(100)), true).unwrap();
        assert_eq!(readings.left, Some(0));
        assert_eq!(readings.right, Some(100));
    }

    #[test]
    fn invalid_live_levels_are_rejected() {
        assert!(convert(available(Some(101)), available(Some(2)), true).is_err());
        assert!(convert(available(Some(12)), available(Some(255)), true).is_err());
    }
}

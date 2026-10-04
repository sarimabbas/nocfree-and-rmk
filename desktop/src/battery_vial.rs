//! One read-only VIA custom-value report using the existing raw-HID collection.
//! Native HID writes are synchronous: the UI bounds observation time, not the
//! OS syscall. Battery's single-flight guard prevents accumulating stuck calls.
use super::Readings;
use crate::runtime_recovery::Role;
use hidapi::{BusType, DeviceInfo as HidInfo, HidApi};
use nusb::{DeviceInfo, MaybeFuture};
use rynk::rmk_types::battery::{BatteryStatus, ChargeState};
use std::time::{Duration, Instant};

const SIGNATURE: &[u8; 4] = b"NCBT";
const HEADER: [u8; 4] = [0x08, 0x7e, 1, 1];

fn request() -> [u8; 33] {
    // HIDAPI requires a zero report-ID prefix; the firmware has no report IDs.
    let mut report = [0; 33];
    report[1..5].copy_from_slice(&HEADER);
    report[5..9].copy_from_slice(SIGNATURE);
    report
}

fn role(device: &DeviceInfo) -> Option<Role> {
    [Role::Left, Role::Receiver].into_iter().find(|role| {
        device.vendor_id() == 0x4c4b
            && device.product_id() == role.product()
            && device.product_string() == Some(role.name())
    })
}

fn hid_contract(
    role: Role,
    vendor: u16,
    product: u16,
    name: Option<&str>,
    usb: bool,
    usage: (u16, u16),
) -> bool {
    usb && vendor == 0x4c4b
        && product == role.product()
        && name == Some(role.name())
        && usage == (0xff60, 0x61)
}

fn hid_matches(info: &HidInfo, role: Role) -> bool {
    hid_contract(
        role,
        info.vendor_id(),
        info.product_id(),
        info.product_string(),
        matches!(info.bus_type(), BusType::Usb),
        (info.usage_page(), info.usage()),
    )
}

fn physical_key(device: &DeviceInfo) -> (nusb::DeviceId, Option<Role>, u32) {
    #[cfg(target_os = "macos")]
    let location = device.location_id();
    #[cfg(not(target_os = "macos"))]
    let location = 0;
    (device.id(), role(device), location)
}

fn select<T>(devices: impl IntoIterator<Item = (Role, T)>) -> Result<T, String> {
    let (mut left, mut receiver) = (Vec::new(), Vec::new());
    for (role, device) in devices {
        match role {
            Role::Left => left.push(device),
            Role::Receiver => receiver.push(device),
            Role::Right => {}
        }
    }
    let mut preferred = if left.is_empty() { receiver } else { left };
    if preferred.len() != 1 {
        return Err(if preferred.is_empty() {
            "Connect the left half or receiver by USB to check both batteries."
        } else {
            "More than one battery source is connected. Check the USB connections."
        }
        .into());
    }
    Ok(preferred.remove(0))
}

fn select_usb() -> Result<DeviceInfo, String> {
    select(
        nusb::list_devices()
            .wait()
            .map_err(|_| "Couldn't inspect USB battery support.")?
            .filter_map(|device| role(&device).map(|role| (role, device))),
    )
}

fn same_usb(expected: &DeviceInfo) -> Result<(), String> {
    let current = select_usb()?;
    if !super::same_connection(&physical_key(expected), [physical_key(&current)]) {
        return Err("The USB connection changed during the battery check.".into());
    }
    Ok(())
}

fn submission_allowed(started: Instant, now: Instant) -> Result<(), String> {
    if now.saturating_duration_since(started) >= Duration::from_secs(3) {
        return Err("The battery check expired before its request.".into());
    }
    Ok(())
}

pub(super) fn read(started: Instant) -> Result<Readings, String> {
    submission_allowed(started, Instant::now())?;
    let target = select_usb()?;
    let role = role(&target).ok_or("Unsupported keyboard identity.")?;
    let api = HidApi::new().map_err(|_| "Couldn't inspect the keyboard's HID battery service.")?;
    let mut interfaces = api.device_list().filter(|info| hid_matches(info, role));
    let info = interfaces
        .next()
        .ok_or("Battery details unavailable with this firmware.")?;
    if interfaces.next().is_some() {
        return Err("The keyboard's battery interface is ambiguous.".into());
    }
    let number = u8::try_from(info.interface_number())
        .map_err(|_| "Couldn't identify the raw-HID interface.")?;
    if !target.interfaces().any(|interface| {
        interface.interface_number() == number
            && (
                interface.class(),
                interface.subclass(),
                interface.protocol(),
            ) == (3, 0, 0)
    }) {
        return Err("The selected interface is not the keyboard's raw-HID service.".into());
    }
    let path = info.path().to_owned();
    let device = api
        .open_path(&path)
        .map_err(|_| "Couldn't open the keyboard's battery service.")?;
    let opened = device
        .get_device_info()
        .map_err(|_| "Couldn't confirm the HID connection.")?;
    if !hid_matches(&opened, role)
        || opened.path() != path.as_c_str()
        || opened.interface_number() != i32::from(number)
    {
        return Err("The HID connection changed before the battery check.".into());
    }
    let mut descriptor = [0; 256];
    let length = device
        .get_report_descriptor(&mut descriptor)
        .map_err(|_| "Couldn't inspect the battery report layout.")?;
    if !report_layout_valid(&descriptor[..length]) {
        return Err("The keyboard's battery report layout is unsupported.".into());
    }
    same_usb(&target)?;
    submission_allowed(started, Instant::now())?;
    if device
        .write(&request())
        .map_err(|_| "Couldn't request the keyboard's battery details.")?
        != 33
    {
        return Err("The battery request was incomplete.".into());
    }
    if started.elapsed() >= Duration::from_secs(3) {
        return Err("The battery check timed out.".into());
    }
    let mut reply = [0; 33];
    let length = device
        .read_timeout(&mut reply, 1000)
        .map_err(|_| "Couldn't read the keyboard's battery details.")?;
    let readings = decode(&reply[..length])?;
    same_usb(&target)?;
    Ok(readings)
}

// Validate only the fixed report shape, not a general HID decoder. Usage and
// collection identity come from the native HID enumeration above.
fn report_layout_valid(descriptor: &[u8]) -> bool {
    let (mut size, mut count, mut input, mut output) = (0_u32, 0_u32, 0_u32, 0_u32);
    let mut bytes = descriptor;
    while let Some((&tag, rest)) = bytes.split_first() {
        if tag == 0xfe || matches!(tag & 0xfc, 0x84 | 0xa4 | 0xb4) {
            return false;
        } // no long items/report IDs
        let length = match tag & 3 {
            3 => 4,
            length => usize::from(length),
        };
        if rest.len() < length {
            return false;
        }
        let value = rest[..length]
            .iter()
            .enumerate()
            .fold(0_u32, |value, (shift, byte)| {
                value | u32::from(*byte) << (shift * 8)
            });
        match tag & 0xfc {
            0x74 => size = value,
            0x94 => count = value,
            0x80 => {
                input = match size
                    .checked_mul(count)
                    .and_then(|bits| input.checked_add(bits))
                {
                    Some(bits) => bits,
                    None => return false,
                }
            }
            0x90 => {
                output = match size
                    .checked_mul(count)
                    .and_then(|bits| output.checked_add(bits))
                {
                    Some(bits) => bits,
                    None => return false,
                }
            }
            0xb0 => return false, // not a feature-report protocol
            _ => {}
        }
        bytes = &rest[length..];
    }
    input == 256 && output == 256
}

fn entry(bytes: &[u8]) -> Result<BatteryStatus, String> {
    let charge_state = match bytes[2] {
        0 => ChargeState::Unknown,
        1 => ChargeState::Charging,
        2 => ChargeState::Discharging,
        _ => return Err("The keyboard reported an invalid charging state.".into()),
    };
    match bytes {
        [0, 0xff, 0] => Ok(BatteryStatus::Unavailable),
        [1, level, _] if *level <= 100 || *level == 0xff => Ok(BatteryStatus::Available {
            level: if *level == 0xff { None } else { Some(*level) },
            charge_state,
        }),
        _ => Err("The keyboard reported invalid battery details.".into()),
    }
}

fn decode(reply: &[u8]) -> Result<Readings, String> {
    if reply.len() != 32 || reply[..3] != HEADER[..3] || &reply[4..8] != SIGNATURE {
        return Err("This firmware does not support Companion battery details.".into());
    }
    if reply[3] != 1 {
        return Err("This firmware uses an unsupported battery protocol version.".into());
    }
    if reply[8] != 0 {
        return Err("Battery details unavailable with this firmware.".into());
    }
    if reply[16..].iter().any(|byte| *byte != 0) || reply[12] > 2 {
        return Err("The keyboard reported invalid battery details.".into());
    }
    let left = entry(&reply[9..12])?;
    let right = entry(&reply[13..16])?;
    let right_connected = reply[12] == 1;
    if !right_connected && right != BatteryStatus::Unavailable {
        return Err("The keyboard reported stale disconnected battery details.".into());
    }
    Ok(Readings {
        left,
        right,
        right_connected,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reply() -> [u8; 32] {
        let mut reply = [0; 32];
        reply[..4].copy_from_slice(&HEADER);
        reply[4..8].copy_from_slice(SIGNATURE);
        reply[9..12].copy_from_slice(&[1, 45, 0]);
        reply[12] = 1;
        reply[13..16].copy_from_slice(&[1, 0xff, 1]);
        reply
    }
    #[test]
    fn slow_setup_cannot_submit_after_the_observation_window() {
        let started = Instant::now();
        assert!(submission_allowed(started, started + Duration::from_secs(2)).is_ok());
        assert!(submission_allowed(started, started + Duration::from_secs(3)).is_err());
        assert!(submission_allowed(started, started + Duration::from_secs(8)).is_err());
    }
    #[test]
    fn only_selected_usb_raw_hid_collection_can_be_opened() {
        for role in [Role::Left, Role::Receiver] {
            assert!(hid_contract(
                role,
                0x4c4b,
                role.product(),
                Some(role.name()),
                true,
                (0xff60, 0x61)
            ));
            assert!(!hid_contract(
                role,
                0x4c4b,
                role.product(),
                Some(role.name()),
                false,
                (0xff60, 0x61)
            ));
            assert!(!hid_contract(
                role,
                0x4c4b,
                role.product(),
                Some(role.name()),
                true,
                (1, 6)
            ));
            assert!(!hid_contract(
                role,
                0x4c4b,
                Role::Right.product(),
                Some(Role::Right.name()),
                true,
                (0xff60, 0x61)
            ));
            assert!(!hid_contract(
                role,
                0x4c4b,
                role.product(),
                Some("Another keyboard"),
                true,
                (0xff60, 0x61)
            ));
        }
    }
    #[test]
    fn direct_left_is_preferred_without_requiring_receiver_unplug() {
        assert_eq!(select([(Role::Receiver, 2), (Role::Left, 1)]).unwrap(), 1);
        assert_eq!(
            select([(Role::Receiver, 2), (Role::Receiver, 3), (Role::Left, 1)]).unwrap(),
            1
        );
        assert_eq!(select([(Role::Receiver, 2)]).unwrap(), 2);
        assert!(select([(Role::Left, 1), (Role::Left, 2), (Role::Receiver, 3)]).is_err());
        assert!(select([(Role::Receiver, 2), (Role::Receiver, 3)]).is_err());
        assert!(select::<u8>([]).is_err());
    }
    #[test]
    fn getter_is_only_custom_read_without_report_id() {
        assert_eq!(&request()[..9], &[0, 8, 0x7e, 1, 1, b'N', b'C', b'B', b'T']);
        assert!(request()[9..].iter().all(|byte| *byte == 0));
    }
    #[test]
    fn preserves_unknown_level_charge_and_connected_state() {
        let readings = decode(&reply()).unwrap();
        assert_eq!(
            readings.left,
            BatteryStatus::Available {
                level: Some(45),
                charge_state: ChargeState::Unknown
            }
        );
        assert_eq!(
            readings.right,
            BatteryStatus::Available {
                level: None,
                charge_state: ChargeState::Charging
            }
        );
        assert!(readings.right_connected);
    }
    #[test]
    fn disconnected_and_unconfigured_cannot_return_cached_level() {
        for connected in [0, 2] {
            let mut reply = reply();
            reply[12] = connected;
            assert!(decode(&reply).is_err());
            reply[13..16].copy_from_slice(&[0, 0xff, 0]);
            let readings = decode(&reply).unwrap();
            assert!(!readings.right_connected);
            assert_eq!(readings.right, BatteryStatus::Unavailable);
        }
    }
    #[test]
    fn version_unsupported_echo_short_and_invalid_responses_fail_closed() {
        for (index, value) in [
            (3, 2),
            (8, 1),
            (8, 3),
            (9, 2),
            (10, 101),
            (11, 3),
            (12, 3),
            (16, 1),
        ] {
            let mut response = reply();
            response[index] = value;
            assert!(decode(&response).is_err());
        }
        assert!(decode(&reply()[..31]).is_err());
        assert!(decode(&request()[1..]).is_err());
    }
    #[test]
    fn only_unnumbered_32_byte_input_and_output_reports_are_allowed() {
        let valid = [0x75, 8, 0x95, 32, 0x81, 2, 0x91, 2];
        assert!(report_layout_valid(&valid));
        assert!(!report_layout_valid(&[
            0x85, 1, 0x75, 8, 0x95, 32, 0x81, 2, 0x91, 2
        ]));
        assert!(!report_layout_valid(&valid[..6]));
        assert!(!report_layout_valid(&[0x75, 8, 0x95, 31, 0x81, 2, 0x91, 2]));
        assert!(!report_layout_valid(&[0x75]));
        assert!(!report_layout_valid(&[
            0xa4, 0x75, 8, 0x95, 32, 0x81, 2, 0x91, 2
        ]));
    }
}

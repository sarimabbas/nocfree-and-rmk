//! Read-only NocFree Link version query and framing.
//!
//! The vendor frontend uses WebUSB bulk transfers, not Vial raw HID.

use nusb::{
    DeviceInfo,
    transfer::{Buffer, Bulk, ControlOut, ControlType, In, Out, Recipient},
};
use std::time::Duration;
use tokio::time::{Instant, timeout_at};

fn identity(vendor: u16, product: u16, name: Option<&str>) -> bool {
    name.is_some_and(|name| {
        crate::device::factory_keyboard_identity(u64::from(vendor), u64::from(product), name)
    })
}

/// The factory keyboard product family uses this getter across layout names.
pub fn matches(device: &DeviceInfo) -> bool {
    identity(
        device.vendor_id(),
        device.product_id(),
        device.product_string(),
    )
}

fn activation(interface: u8, enabled: bool) -> ControlOut<'static> {
    ControlOut {
        control_type: ControlType::Class,
        recipient: Recipient::Interface,
        request: 0x22,
        value: u16::from(enabled),
        index: u16::from(interface),
        data: &[],
    }
}

/// Read the official factory-left version; no firmware setters, resets or DFU.
/// Main operations have a shared 2.8-second deadline; teardown gets 200 ms.
pub async fn read(selected: &DeviceInfo) -> Result<String, String> {
    if !matches(selected) {
        return Err("Unsupported factory firmware version interface".into());
    }
    let deadline = Instant::now() + Duration::from_millis(2800);
    let interface = timeout_at(deadline, async {
        let mut devices = nusb::list_devices()
            .await
            .map_err(|_| "Could not inspect factory keyboard")?
            .filter(matches);
        let current = devices.next().ok_or("Factory keyboard disconnected")?;
        if devices.next().is_some() || current.id() != selected.id() {
            return Err("Factory keyboard connection changed or is ambiguous");
        }
        let device = current
            .open()
            .await
            .map_err(|_| "Could not open factory keyboard")?;
        // Never reconfigure a composite keyboard. Read the existing configuration.
        let configuration = device
            .active_configuration()
            .map_err(|_| "Factory keyboard is not configured")?;
        let mut vendor = configuration
            .interface_alt_settings()
            .filter(|d| d.class() == 0xff && d.alternate_setting() == 0);
        let descriptor = vendor
            .next()
            .ok_or("Factory keyboard version interface is unavailable")?;
        if vendor.next().is_some() {
            return Err("Factory keyboard version interface is ambiguous");
        }
        let number = descriptor.interface_number();
        device
            .claim_interface(number)
            .await
            .map_err(|_| "Factory keyboard version interface is busy")
    })
    .await
    .map_err(|_| "Factory keyboard version query timed out".to_string())?
    .map_err(str::to_owned)?;

    let result = timeout_at(deadline, async {
        interface
            .set_alt_setting(0)
            .await
            .map_err(|_| "Could not select factory version interface")?;
        let descriptor = interface
            .descriptor()
            .ok_or("Factory version descriptor is unavailable")?;
        if descriptor.class() != 0xff {
            return Err("Factory version interface changed".to_string());
        }
        let endpoints: Vec<_> = descriptor.endpoints().collect();
        if endpoints.len() != 2
            || endpoints
                .iter()
                .any(|e| e.transfer_type() != nusb::descriptors::TransferType::Bulk)
        {
            return Err("Factory version endpoints are unsupported".into());
        }
        let input = endpoints
            .iter()
            .find(|e| e.address() & 0x80 != 0)
            .ok_or("Missing factory version input")?
            .address();
        let output = endpoints
            .iter()
            .find(|e| e.address() & 0x80 == 0)
            .ok_or("Missing factory version output")?
            .address();
        let mut input = interface
            .endpoint::<Bulk, In>(input)
            .map_err(|_| "Could not claim factory version input")?;
        let mut output = interface
            .endpoint::<Bulk, Out>(output)
            .map_err(|_| "Could not claim factory version output")?;
        interface
            .control_out(
                activation(interface.interface_number(), true),
                Duration::from_millis(500),
            )
            .await
            .map_err(|_| "Could not activate factory version interface")?;
        input.submit(Buffer::new(64));
        output.submit(READ_VERSION_REQUEST.into());
        output
            .next_complete()
            .await
            .into_result()
            .map_err(|_| "Factory version request failed")?;
        let mut frame = Vec::new();
        loop {
            let response = input
                .next_complete()
                .await
                .into_result()
                .map_err(|_| "Factory version response failed")?;
            frame.extend_from_slice(&response);
            if frame.len() > 261 {
                return Err("Factory version response is too large".into());
            }
            if frame.len() >= 4 && frame.len() >= usize::from(frame[3]) + 6 {
                return parse_version_response(&frame).map_err(str::to_owned);
            }
            input.submit(Buffer::new(64));
        }
    })
    .await
    .map_err(|_| "Factory keyboard version query timed out".to_string())
    .and_then(|r| r);
    let cleanup = interface
        .control_out(
            activation(interface.interface_number(), false),
            Duration::from_millis(200),
        )
        .await;
    if cleanup.is_err() {
        return Err("Factory version interface did not close cleanly".into());
    }
    result
}

/// `READ_VERSION` with an empty payload; deliberately excludes setters and DFU.
pub const READ_VERSION_REQUEST: [u8; 6] = [0xff, 0xfe, 0x51, 0, 0xfe, 0xff];

/// Parse one complete, successful version response. Unknown variants stay unknown.
///
/// The official client accepts a status byte followed by major/minor/patch. We
/// require that unambiguous form rather than interpreting arbitrary bytes as a
/// release number or accepting the vendor client's ambiguous statusless fallback.
pub fn parse_version_response(frame: &[u8]) -> Result<String, &'static str> {
    if frame.len() < 6 || frame[..2] != [0xff, 0xfe] || frame[2] != 0xd1 {
        return Err("Not a factory firmware version response");
    }
    let payload_len = usize::from(frame[3]);
    if frame.len() != payload_len + 6 || frame[frame.len() - 2..] != [0xfe, 0xff] {
        return Err("Malformed factory firmware version frame");
    }
    let payload = &frame[4..frame.len() - 2];
    if payload.len() < 4 {
        return Err("Factory firmware version is unavailable");
    }
    if payload[0] != 0 {
        return Err("Factory firmware version request failed");
    }
    Ok(format!("{}.{}.{}", payload[1], payload[2], payload[3]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn getter_targets_factory_left_without_dfu_or_setters() {
        for layout in ["ANSI", "ISO", "JP", "KR", "JIS"] {
            assert!(identity(
                0x2886,
                0x8029,
                Some(&format!("NocFree & {layout}"))
            ));
        }
        for (vendor, product, name) in [
            (0x2886, 0x8029, "NocFree_Dongle"),
            (0x239a, 0x80d8, "NocFree nRF52833 Right"),
            (0x4c4b, 0x4643, "NocFree RMK"),
            (0x2886, 0x8029, "Unrelated"),
        ] {
            assert!(!identity(vendor, product, Some(name)));
        }
        let setup = activation(2, true);
        assert_eq!((setup.request, setup.value, setup.index), (0x22, 1, 2));
        assert_eq!(activation(2, false).value, 0);
        assert_eq!(READ_VERSION_REQUEST[2], 0x51);
    }

    #[test]
    fn vendor_version_reply_is_not_usb_device_revision() {
        assert_eq!(
            parse_version_response(&[0xff, 0xfe, 0xd1, 4, 0, 2, 4, 5, 0xfe, 0xff]),
            Ok("2.4.5".into())
        );
        assert_eq!(READ_VERSION_REQUEST, [0xff, 0xfe, 0x51, 0, 0xfe, 0xff]);
    }

    #[test]
    fn rejects_errors_and_ambiguous_or_unrelated_frames() {
        for frame in [
            vec![0xff, 0xfe, 0xd1, 4, 1, 2, 4, 5, 0xfe, 0xff],
            vec![0xff, 0xfe, 0xd1, 3, 2, 4, 5, 0xfe, 0xff],
            vec![0xff, 0xfe, 0xd6, 4, 0, 2, 4, 5, 0xfe, 0xff],
            vec![0xff, 0xfe, 0xd1, 5, 0, 2, 4, 5, 0xfe, 0xff],
            vec![0xff, 0xfe, 0xd1, 4, 0, 2, 4, 5, 0, 0xff],
            vec![],
        ] {
            assert!(parse_version_response(&frame).is_err());
        }
    }
}

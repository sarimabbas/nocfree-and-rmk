//! Local runtime DFU recovery, explicitly armed by a Companion journey.
//! Targets the USB-connected component, never a forwarded Vial keyboard command.
//! Request completion is not recovery-drive validation. This module never flashes firmware.
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use nusb::{
    DeviceId, DeviceInfo,
    transfer::{ControlOut, ControlType, Recipient},
};

const VENDOR: u16 = 0x4c4b;
const ARM_WINDOW: Duration = Duration::from_secs(10);
const OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Left,
    Right,
    Receiver,
}

impl Role {
    pub fn product(self) -> u16 {
        match self {
            Self::Left => 0x4643,
            Self::Right => 0x4671,
            Self::Receiver => 0x4644,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Left => "NocFree RMK",
            Self::Right => "NocFree RMK Right",
            Self::Receiver => "NocFree RMK Receiver",
        }
    }
}

fn contract(
    role: Role,
    vendor: u16,
    product: u16,
    name: Option<&str>,
    interfaces: &[(u8, u8, u8, u8)],
) -> Option<u8> {
    if vendor != VENDOR || product != role.product() || name != Some(role.name()) {
        return None;
    }
    let mut dfu = interfaces
        .iter()
        .filter(|(_, class, subclass, protocol)| (*class, *subclass, *protocol) == (0xfe, 1, 1));
    let number = dfu.next()?.0;
    if dfu.next().is_some() {
        None
    } else {
        Some(number)
    }
}

pub fn matches(role: Role, device: &DeviceInfo) -> bool {
    dfu_interface(role, device).is_some()
}

fn dfu_interface(role: Role, device: &DeviceInfo) -> Option<u8> {
    contract(
        role,
        device.vendor_id(),
        device.product_id(),
        device.product_string(),
        &device
            .interfaces()
            .map(|i| (i.interface_number(), i.class(), i.subclass(), i.protocol()))
            .collect::<Vec<_>>(),
    )
}

fn functional_valid(descriptors: &[&[u8]]) -> bool {
    descriptors.len() == 1 && descriptors[0].len() == 9 && descriptors[0][0..3] == [9, 0x21, 8]
}

fn validate_descriptor(claimed: &nusb::Interface, number: u8) -> Result<(), &'static str> {
    let desc = claimed
        .descriptor()
        .ok_or("Missing recovery interface descriptor.")?;
    if desc.interface_number() != number
        || desc.class() != 0xfe
        || desc.subclass() != 1
        || desc.protocol() != 1
        || desc.alternate_setting() != 0
        || desc.num_endpoints() != 0
    {
        return Err("Recovery interface changed.");
    }
    let functions = desc
        .descriptors()
        .filter(|d| d.descriptor_type() == 0x21)
        .map(|d| d.to_vec())
        .collect::<Vec<_>>();
    if !functional_valid(&functions.iter().map(Vec::as_slice).collect::<Vec<_>>()) {
        return Err("This firmware does not support local recovery.");
    }
    Ok(())
}

fn request_allowed(cancelled: &AtomicBool, armed_at: Instant) -> Result<(), &'static str> {
    if cancelled.load(Ordering::Relaxed) {
        return Err("Recovery cancelled.");
    }
    if armed_at.elapsed() >= ARM_WINDOW {
        return Err("Recovery selection expired. Try again.");
    }
    Ok(())
}

/// Consumed by one request. A static role serial is not physical-device identity;
/// selection is bound to the exact currently enumerated USB connection instead.
pub struct ArmedRequest {
    role: Role,
    connection: DeviceId,
    armed_at: Instant,
    interface: u8,
    #[cfg(target_os = "macos")]
    location: u32,
}

impl ArmedRequest {
    /// Call only after an explicit owner action selects this observed endpoint.
    pub fn arm(role: Role, selected: &DeviceInfo) -> Result<Self, &'static str> {
        let interface = dfu_interface(role, selected)
            .ok_or("This firmware does not support automatic local recovery.")?;
        Ok(Self {
            role,
            connection: selected.id(),
            armed_at: Instant::now(),
            interface,
            #[cfg(target_os = "macos")]
            location: selected.location_id(),
        })
    }

    /// Send only DFU_DETACH, once. Success means request completion, not a mounted
    /// recovery drive. A disconnect/error may follow a reset; never auto-retry.
    pub async fn request_detach(self, cancelled: &AtomicBool) -> Result<(), &'static str> {
        if self.armed_at.elapsed() >= ARM_WINDOW {
            return Err("Recovery selection expired. Select the connected device again.");
        }
        tokio::time::timeout(OPERATION_TIMEOUT, async move {
            if cancelled.load(Ordering::Relaxed) { return Err("Recovery cancelled."); }
            let devices = nusb::list_devices().await.map_err(|_| "Could not inspect the USB connection.")?;
            let mut selected = devices.filter(|d| matches(self.role, d));
            let endpoint = selected.next().ok_or("The selected component disconnected.")?;
            if selected.next().is_some() || endpoint.id() != self.connection {
                return Err("The USB connection changed or is ambiguous. Select the device again.");
            }
            if cancelled.load(Ordering::Relaxed) { return Err("Recovery cancelled."); }
            #[cfg(target_os = "macos")]
            if endpoint.location_id() != self.location { return Err("The USB port changed."); }
            let device = endpoint.open().await.map_err(|_| "Could not open the selected recovery interface.")?;
            if cancelled.load(Ordering::Relaxed) { return Err("Recovery cancelled."); }
            let interface = device.claim_interface(self.interface).await.map_err(|_| "Could not claim the selected recovery interface.")?;
            validate_descriptor(&interface, self.interface)?;
            let mut fresh = nusb::list_devices().await.map_err(|_| "Could not recheck the USB connection.")?
                .filter(|d| matches(self.role, d));
            let current = fresh.next().ok_or("The selected component disconnected.")?;
            if fresh.next().is_some() || current.id() != self.connection || dfu_interface(self.role, &current) != Some(self.interface) {
                return Err("The USB connection changed before recovery.");
            }
            #[cfg(target_os = "macos")]
            if current.location_id() != self.location { return Err("The USB port changed."); }
            request_allowed(cancelled, self.armed_at)?;
            interface.control_out(detach_request(self.interface), Duration::from_secs(2)).await
                .map_err(|_| "Recovery request did not complete. Check the device state before trying again.")
        }).await.map_err(|_| "Recovery request timed out. Check the device state before trying again.")?
    }
}

fn detach_request(number: u8) -> ControlOut<'static> {
    ControlOut {
        control_type: ControlType::Class,
        recipient: Recipient::Interface,
        request: 0,
        value: 1000,
        index: u16::from(number),
        data: &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_role_is_distinct_from_forwarded_vial_and_legacy_startup() {
        for role in [Role::Left, Role::Right, Role::Receiver] {
            let interfaces = [(0, 3, 0, 0), (1, 0xff, 0, 0), (2, 0xfe, 1, 1)];
            assert_eq!(
                contract(role, VENDOR, role.product(), Some(role.name()), &interfaces),
                Some(2)
            );
            assert_eq!(
                contract(
                    role,
                    VENDOR,
                    role.product(),
                    Some(role.name()),
                    &interfaces[..2]
                ),
                None
            );
            assert_eq!(
                contract(
                    role,
                    VENDOR,
                    role.product(),
                    Some(Role::Right.name()),
                    &interfaces
                ),
                if role == Role::Right { Some(2) } else { None }
            );
            assert_eq!(
                contract(role, VENDOR, 0x4661, Some(role.name()), &interfaces),
                None
            );
            assert_eq!(
                contract(
                    role,
                    VENDOR,
                    role.product(),
                    Some(role.name()),
                    &[(0, 0xfe, 1, 1), (1, 0xfe, 1, 1)]
                ),
                None
            );
            assert_eq!(
                contract(role, 0x239a, role.product(), Some(role.name()), &interfaces),
                None
            );
        }
        assert_ne!(Role::Left.product(), Role::Receiver.product());
    }
    #[test]
    fn only_will_detach_runtime_descriptor_is_accepted() {
        let valid = [9, 0x21, 8, 0, 0, 0, 0, 0x10, 1];
        assert!(functional_valid(&[&valid]));
        assert!(!functional_valid(&[]));
        assert!(!functional_valid(&[&valid, &valid]));
        assert!(!functional_valid(&[&valid[..8]]));
        let mut download = valid;
        download[2] = 9;
        assert!(!functional_valid(&[&download]));
    }
    #[test]
    fn cancellation_and_expiration_prevent_dispatch() {
        let cancelled = AtomicBool::new(true);
        assert!(request_allowed(&cancelled, Instant::now()).is_err());
        cancelled.store(false, Ordering::Relaxed);
        assert!(request_allowed(&cancelled, Instant::now() - ARM_WINDOW).is_err());
        assert!(request_allowed(&cancelled, Instant::now()).is_ok());
    }
    #[test]
    fn request_is_local_class_interface_detach_without_payload() {
        let request = detach_request(3);
        assert_eq!(
            (request.control_type, request.recipient, request.request),
            (ControlType::Class, Recipient::Interface, 0)
        );
        assert_eq!(
            (request.value, request.index, request.data.len()),
            (1000, 3, 0)
        );
    }
}

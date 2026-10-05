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
        .ok_or("Could not read the recovery connection information.")?;
    if desc.interface_number() != number
        || desc.class() != 0xfe
        || desc.subclass() != 1
        || desc.protocol() != 1
        || desc.alternate_setting() != 0
        || desc.num_endpoints() != 0
    {
        return Err("The recovery connection changed.");
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

/// Only submitted uncertainty may be reconciled by watching for the recovery drive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchError {
    NotSent(&'static str),
    OutcomeUnknown(&'static str),
}
impl DispatchError {
    pub fn message(self) -> &'static str {
        match self {
            Self::NotSent(message) | Self::OutcomeUnknown(message) => message,
        }
    }
}
fn dispatch_error(submitted: bool, message: &'static str) -> DispatchError {
    if submitted {
        DispatchError::OutcomeUnknown(message)
    } else {
        DispatchError::NotSent(message)
    }
}
/// Consumed by one request. A static role serial is not physical-device identity;
/// selection is bound to the exact currently enumerated USB connection instead.
pub struct ArmedRequest {
    role: Role,
    connection: DeviceId,
    armed_at: Instant,
    interface: u8,
    location: u64,
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
            location: crate::device::usb_location(selected)
                .ok_or("Could not identify the physical USB port.")?,
        })
    }

    /// Send only DFU_DETACH, once. Success means request completion, not a mounted
    /// recovery drive. A disconnect/error may follow a reset; never auto-retry.
    pub async fn request_detach(self, cancelled: &AtomicBool) -> Result<(), DispatchError> {
        if self.armed_at.elapsed() >= ARM_WINDOW {
            return Err(DispatchError::NotSent(
                "Recovery selection expired. Select the connected device again.",
            ));
        }
        let submitted = AtomicBool::new(false);
        let attempted = &submitted;
        let result=tokio::time::timeout(OPERATION_TIMEOUT, async move {
            if cancelled.load(Ordering::Relaxed) { return Err("Recovery cancelled."); }
            let devices = nusb::list_devices().await.map_err(|_| "Could not inspect the USB connection.")?;
            let mut selected = devices.filter(|d| matches(self.role, d));
            let endpoint = selected.next().ok_or("The selected part disconnected.")?;
            if selected.next().is_some() || endpoint.id() != self.connection {
                return Err("The USB connection changed or matches more than one part. Select the part again.");
            }
            if cancelled.load(Ordering::Relaxed) { return Err("Recovery cancelled."); }
            if crate::device::usb_location(&endpoint) != Some(self.location) { return Err("The USB port changed."); }
            let device = endpoint.open().await.map_err(|_| "Could not open the selected recovery connection.")?;
            if cancelled.load(Ordering::Relaxed) { return Err("Recovery cancelled."); }
            let interface = device.claim_interface(self.interface).await.map_err(|_| "Could not use the selected recovery connection.")?;
            validate_descriptor(&interface, self.interface)?;
            let mut fresh = nusb::list_devices().await.map_err(|_| "Could not recheck the USB connection.")?
                .filter(|d| matches(self.role, d));
            let current = fresh.next().ok_or("The selected part disconnected.")?;
            if fresh.next().is_some() || current.id() != self.connection || dfu_interface(self.role, &current) != Some(self.interface) {
                return Err("The USB connection changed before recovery.");
            }
            if crate::device::usb_location(&current) != Some(self.location) { return Err("The USB port changed."); }
            request_allowed(cancelled, self.armed_at)?;
            attempted.store(true,Ordering::Release);
            interface.control_out(detach_request(self.interface), Duration::from_secs(2)).await
                .map_err(|_| "The recovery request did not finish. Check whether the recovery drive is open before trying again.")
        }).await.unwrap_or(Err("The recovery request timed out. Check whether the recovery drive is open before trying again."));
        result.map_err(|message| dispatch_error(submitted.load(Ordering::Acquire), message))
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
    fn dispatch_errors_distinguish_unsent_failure_from_possible_reset() {
        assert_eq!(
            dispatch_error(false, "Could not open the selected recovery connection."),
            DispatchError::NotSent("Could not open the selected recovery connection.")
        );
        assert_eq!(
            dispatch_error(true, "Disconnected during detach"),
            DispatchError::OutcomeUnknown("Disconnected during detach")
        );
        assert_eq!(
            dispatch_error(false, "Timeout"),
            DispatchError::NotSent("Timeout")
        );
        assert_eq!(
            dispatch_error(true, "Timeout"),
            DispatchError::OutcomeUnknown("Timeout")
        );
    }
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

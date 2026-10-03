//! Host request for the uninstalled USB rescue-stage prototype, not a user flow.
//! The stage is experimental; hardware recovery needs validation. USB attachment cannot reset
//! an already running battery-powered half. This module never flashes firmware.
use std::time::{Duration, Instant};

use nusb::{
    DeviceId, DeviceInfo,
    transfer::{ControlOut, ControlType, Recipient},
};

const VENDOR: u16 = 0x4c4b;
const INTERFACE: u8 = 0;
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
            Self::Left => 0x4660,
            Self::Right => 0x4661,
            Self::Receiver => 0x4662,
        }
    }
}

fn contract(role: Role, vendor: u16, product: u16, interfaces: &[(u8, u8, u8, u8)]) -> bool {
    vendor == VENDOR && product == role.product() && interfaces == [(INTERFACE, 0xfe, 1, 1)]
}

fn matches(role: Role, device: &DeviceInfo) -> bool {
    contract(
        role,
        device.vendor_id(),
        device.product_id(),
        &device
            .interfaces()
            .map(|i| (i.interface_number(), i.class(), i.subclass(), i.protocol()))
            .collect::<Vec<_>>(),
    )
}

/// Consumed by one request. A static role serial is not physical-device identity;
/// selection is bound to the exact currently enumerated USB connection instead.
pub struct ArmedRequest {
    role: Role,
    connection: DeviceId,
    armed_at: Instant,
}

impl ArmedRequest {
    /// Call only after an explicit owner action selects this observed endpoint.
    pub fn arm(role: Role, selected: &DeviceInfo) -> Result<Self, &'static str> {
        if !matches(role, selected) {
            return Err("This device is not the selected rescue-stage prototype.");
        }
        Ok(Self {
            role,
            connection: selected.id(),
            armed_at: Instant::now(),
        })
    }

    /// Send only DFU_DETACH, once. Success means request completion, not a mounted
    /// recovery drive. A disconnect/error may follow a reset; never auto-retry.
    pub async fn request_detach(self) -> Result<(), &'static str> {
        if self.armed_at.elapsed() >= ARM_WINDOW {
            return Err("Recovery selection expired. Select the connected device again.");
        }
        tokio::time::timeout(OPERATION_TIMEOUT, async move {
            let devices = nusb::list_devices().await.map_err(|_| "Could not inspect the USB connection.")?;
            let mut selected = devices.filter(|d| matches(self.role, d));
            let endpoint = selected.next().ok_or("The selected prototype disconnected.")?;
            if selected.next().is_some() || endpoint.id() != self.connection {
                return Err("The USB connection changed or is ambiguous. Select the device again.");
            }
            let device = endpoint.open().await.map_err(|_| "Could not open the selected rescue interface.")?;
            let interface = device.claim_interface(INTERFACE).await.map_err(|_| "Could not claim the selected rescue interface.")?;
            if self.armed_at.elapsed() >= ARM_WINDOW { return Err("Recovery selection expired before the request."); }
            interface.control_out(detach_request(), Duration::from_secs(2)).await
                .map_err(|_| "Recovery request did not complete. Check the device state before trying again.")
        }).await.map_err(|_| "Recovery request timed out. Check the device state before trying again.")?
    }
}

fn detach_request() -> ControlOut<'static> {
    ControlOut {
        control_type: ControlType::Class,
        recipient: Recipient::Interface,
        request: 0,
        value: 1000,
        index: u16::from(INTERFACE),
        data: &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_selected_role_and_single_runtime_dfu_interface_are_allowed() {
        for role in [Role::Left, Role::Right, Role::Receiver] {
            assert!(contract(role, VENDOR, role.product(), &[(0, 0xfe, 1, 1)]));
            assert!(!contract(role, 0x239a, role.product(), &[(0, 0xfe, 1, 1)]));
            assert!(!contract(role, VENDOR, 0x0029, &[(0, 0xfe, 1, 1)]));
            assert!(!contract(role, VENDOR, role.product(), &[(1, 0xfe, 1, 1)]));
            assert!(!contract(role, VENDOR, role.product(), &[(0, 0xfe, 1, 2)]));
            assert!(!contract(
                role,
                VENDOR,
                role.product(),
                &[(0, 0xfe, 1, 1), (1, 3, 0, 0)]
            ));
        }
        assert!(!contract(
            Role::Right,
            VENDOR,
            Role::Left.product(),
            &[(0, 0xfe, 1, 1)]
        ));
    }
    #[test]
    fn request_is_only_zero_length_interface_dfu_detach() {
        let request = detach_request();
        assert_eq!(
            (request.control_type, request.recipient, request.request),
            (ControlType::Class, Recipient::Interface, 0)
        );
        assert_eq!(
            (request.value, request.index, request.data.len()),
            (1000, 0, 0)
        );
    }
}

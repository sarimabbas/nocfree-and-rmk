//! Local runtime DFU recovery, explicitly armed by a Companion journey.
//! Targets the USB-connected component, never a forwarded Vial keyboard command.
//! Request completion is not recovery-drive validation. This module never flashes firmware.
use hidapi::{BusType, DeviceInfo as HidInfo, HidApi};
use nusb::MaybeFuture;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};
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
    recovery_interface(role, device).is_some()
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

// LEFT's VIA service executes locally. Receiver VIA reports are forwarded to
// LEFT, so only the receiver's local DFU interface may request its recovery.
fn recovery_interface(role: Role, device: &DeviceInfo) -> Option<u8> {
    if role != Role::Left {
        return dfu_interface(role, device);
    }
    if device.vendor_id() != VENDOR
        || device.product_id() != role.product()
        || device.product_string() != Some(role.name())
    {
        return None;
    }
    let mut raw = device
        .interfaces()
        .filter(|i| (i.class(), i.subclass(), i.protocol()) == (3, 0, 0));
    let number = raw.next()?.interface_number();
    Some(number)
}

fn left_hid_matches(info: &HidInfo) -> bool {
    left_hid_contract(
        matches!(info.bus_type(), BusType::Usb),
        info.vendor_id(),
        info.product_id(),
        info.product_string(),
        (info.usage_page(), info.usage()),
    )
}

fn left_hid_contract(
    usb: bool,
    vendor: u16,
    product: u16,
    name: Option<&str>,
    usage: (u16, u16),
) -> bool {
    usb && vendor == VENDOR
        && product == Role::Left.product()
        && name == Some(Role::Left.name())
        && usage == (0xff60, 0x61)
}

fn bootloader_report() -> [u8; 33] {
    let mut report = [0; 33];
    // Stock VIA command; HIDAPI's first byte is the zero report-ID prefix.
    report[1] = rynk::rmk_types::protocol::vial::ViaCommand::BootloaderJump as u8;
    report
}

fn functional_valid(descriptors: &[&[u8]]) -> bool {
    descriptors.len() == 1
        && descriptors[0].len() == 9
        && descriptors[0][0..2] == [9, 0x21]
        && matches!(descriptors[0][2], 8 | 9)
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DispatchError {
    NotSent(String),
    OutcomeUnknown(String),
}
fn dispatch_error(submitted: bool, message: impl Into<String>) -> DispatchError {
    if submitted {
        DispatchError::OutcomeUnknown(message.into())
    } else {
        DispatchError::NotSent(message.into())
    }
}
// Timeout and worker arbitrate dispatch once: pending=0, submitted=1, closed=2.
fn claim_dispatch(state: &AtomicU8) -> bool {
    state
        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}
fn close_dispatch(state: &AtomicU8) -> bool {
    state
        .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
        .unwrap_or_else(|state| state)
        == 1
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
        let interface = recovery_interface(role, selected)
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

    fn recheck_left(&self) -> Result<(), String> {
        let mut devices = nusb::list_devices()
            .wait()
            .map_err(|_| "Could not inspect the left USB connection.")?
            .filter(|d| matches(Role::Left, d));
        let current = devices.next().ok_or("The left half disconnected.")?;
        if devices.next().is_some()
            || current.id() != self.connection
            || crate::device::usb_location(&current) != Some(self.location)
            || !current.interfaces().any(|i| {
                i.interface_number() == self.interface
                    && (i.class(), i.subclass(), i.protocol()) == (3, 0, 0)
            })
        {
            return Err("The left USB connection changed. Select it again.".into());
        }
        Ok(())
    }

    fn request_left(mut self, cancelled: Arc<AtomicBool>) -> Result<(), DispatchError> {
        let submitted = Arc::new(AtomicU8::new(0));
        let attempted = submitted.clone();
        let result = crate::battery::native_task(move |started| {
            request_allowed(&cancelled, self.armed_at).map_err(str::to_owned)?;
            self.recheck_left()?;
            let api = HidApi::new().map_err(|_| "Could not inspect the left HID connection.")?;
            let mut candidates = api.device_list().filter(|i| left_hid_matches(i));
            let info = candidates
                .next()
                .ok_or("The left recovery command is unavailable.")?;
            if candidates.next().is_some() {
                return Err("More than one left HID connection is present.".into());
            }
            self.interface = u8::try_from(info.interface_number())
                .map_err(|_| "Could not identify the left raw-HID interface.")?;
            let path = info.path().to_owned();
            let device = api
                .open_path(&path)
                .map_err(|_| "Could not open the left recovery connection.")?;
            let opened = device
                .get_device_info()
                .map_err(|_| "Could not confirm the left HID connection.")?;
            if !left_hid_matches(&opened)
                || opened.path() != path.as_c_str()
                || opened.interface_number() != i32::from(self.interface)
            {
                return Err("The left HID connection changed.".into());
            }
            let mut descriptor = [0; 256];
            let length = device
                .get_report_descriptor(&mut descriptor)
                .map_err(|_| "Could not inspect the left recovery report.")?;
            if !crate::battery::vial::report_layout_valid(&descriptor[..length]) {
                return Err("The left recovery report is unsupported.".into());
            }
            self.recheck_left()?;
            request_allowed(&cancelled, self.armed_at).map_err(str::to_owned)?;
            if started.elapsed() >= OPERATION_TIMEOUT {
                return Err("The left recovery request expired before it was sent.".into());
            }
            if !claim_dispatch(&attempted) {
                return Err("The left recovery request expired before it was sent.".into());
            }
            if device
                .write(&bootloader_report())
                .map_err(|_| "The left recovery request did not finish.")?
                != 33
            {
                return Err("The left recovery request was incomplete.".into());
            }
            Ok(())
        });
        result.map_err(|message| {
            if close_dispatch(&submitted) {
                DispatchError::OutcomeUnknown(
                    "Recovery was requested. Waiting for the recovery drive.".into(),
                )
            } else {
                DispatchError::NotSent(message)
            }
        })
    }

    /// Send the local recovery command once. Success means completion, not a mounted
    /// recovery drive. A disconnect/error may follow a reset; never auto-retry.
    pub async fn request_detach(self, cancelled: Arc<AtomicBool>) -> Result<(), DispatchError> {
        if self.role == Role::Left {
            return self.request_left(cancelled);
        }
        if self.armed_at.elapsed() >= ARM_WINDOW {
            return Err(DispatchError::NotSent(
                "Recovery selection expired. Select the connected device again.".into(),
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
            request_allowed(&cancelled, self.armed_at)?;
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
    fn native_bootloader_command_targets_only_left_usb_via() {
        assert_eq!(bootloader_report()[..3], [0, 0x0b, 0]);
        assert!(bootloader_report()[2..].iter().all(|byte| *byte == 0));
        assert!(left_hid_contract(
            true,
            VENDOR,
            Role::Left.product(),
            Some(Role::Left.name()),
            (0xff60, 0x61)
        ));
        for role in [Role::Right, Role::Receiver] {
            assert!(!left_hid_contract(
                true,
                VENDOR,
                role.product(),
                Some(role.name()),
                (0xff60, 0x61)
            ));
        }
        assert!(!left_hid_contract(
            false,
            VENDOR,
            Role::Left.product(),
            Some(Role::Left.name()),
            (0xff60, 0x61)
        ));
        assert!(!left_hid_contract(
            true,
            VENDOR,
            Role::Left.product(),
            Some(Role::Left.name()),
            (1, 6)
        ));
        assert!(!left_hid_contract(
            true,
            VENDOR,
            Role::Left.product(),
            Some(Role::Receiver.name()),
            (0xff60, 0x61)
        ));
    }
    #[test]
    fn dispatch_errors_distinguish_unsent_failure_from_possible_reset() {
        assert_eq!(
            dispatch_error(false, "Could not open the selected recovery connection."),
            DispatchError::NotSent("Could not open the selected recovery connection.".into())
        );
        assert_eq!(
            dispatch_error(true, "Disconnected during detach"),
            DispatchError::OutcomeUnknown("Disconnected during detach".into())
        );
        assert_eq!(
            dispatch_error(false, "Timeout"),
            DispatchError::NotSent("Timeout".into())
        );
        assert_eq!(
            dispatch_error(true, "Timeout"),
            DispatchError::OutcomeUnknown("Timeout".into())
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
    fn production_and_upstream_runtime_detach_descriptors_are_accepted() {
        let valid = [9, 0x21, 8, 0, 0, 0, 0, 0x10, 1];
        assert!(functional_valid(&[&valid]));
        assert!(!functional_valid(&[]));
        assert!(!functional_valid(&[&valid, &valid]));
        assert!(!functional_valid(&[&valid[..8]]));
        // Upstream RMK advertises CAN_DOWNLOAD as well as WILL_DETACH on its
        // runtime interface. We send DETACH only; protocol 1 is checked above.
        let mut upstream = valid;
        upstream[2] = 9;
        assert!(functional_valid(&[&upstream]));
        for attributes in [0, 1, 2, 4, 10, 15] {
            let mut unsupported = valid;
            unsupported[2] = attributes;
            assert!(!functional_valid(&[&unsupported]));
        }
    }
    #[test]
    fn timed_out_worker_cannot_dispatch_after_not_sent() {
        let state = Arc::new(AtomicU8::new(0));
        let worker = state.clone();
        let (release, paused) = std::sync::mpsc::channel();
        let task = std::thread::spawn(move || {
            paused.recv().unwrap();
            claim_dispatch(&worker)
        });
        assert!(!close_dispatch(&state));
        release.send(()).unwrap();
        assert!(!task.join().unwrap());
        let submitted = AtomicU8::new(0);
        assert!(claim_dispatch(&submitted));
        assert!(close_dispatch(&submitted));
        assert!(!claim_dispatch(&submitted));
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

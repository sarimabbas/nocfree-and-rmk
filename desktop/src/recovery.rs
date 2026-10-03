//! Explicitly armed recovery guide. Requests recovery only; never writes firmware.
use crate::device;
use nocfree_companion::experimental_recovery::{ArmedRequest, Role};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub(crate) fn run(role: Role, cancelled: Arc<AtomicBool>) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("Recovery mode currently supports macOS only.".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "Couldn’t start recovery.".to_owned())?;
    runtime.block_on(async move {
        let mut requested_location=None;
        let mut requested_at=None;
        // Wait for the user's physical action without expiring while they read.
        // Once a matching stage appears, dispatch exactly once; the armed request
        // and subsequent drive observation retain their finite deadlines.
        loop {
            if cancelled.load(Ordering::Relaxed) { return Err("Recovery cancelled.".into()); }
            if drive_deadline_passed(requested_at, Instant::now()) {
                return Err("The recovery drive didn’t appear. Check its power and USB connection, then try again.".into());
            }
            if requested_at.is_none() {
                let devices=tokio::time::timeout(Duration::from_secs(2),nusb::list_devices()).await
                    .map_err(|_| "USB discovery took too long.")?.map_err(|_| "Couldn’t inspect USB devices.")?;
                let mut targets=devices.filter(|d| d.vendor_id()==0x4c4b && d.product_id()==role.product());
                if let Some(target)=targets.next() {
                    if targets.next().is_some() { return Err("Connect only one of the selected component.".into()); }
                    let request=ArmedRequest::arm(role,&target).map_err(str::to_owned)?;
                    if cancelled.load(Ordering::Relaxed) { return Err("Recovery cancelled.".into()); }
                    #[cfg(target_os="macos")]
                    { requested_location=Some(u64::from(target.location_id())); }
                    requested_at=Some(Instant::now());
                    // A successful reset may disconnect before acknowledgement.
                    // Only the subsequently correlated drive determines success.
                    let _=request.request_detach(&cancelled).await;
                }
            }
            if let Some(location)=requested_location {
                let snapshot=device::discover()?;
                if correlated_drive(&snapshot,location) { return Ok(()); }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
}

fn drive_deadline_passed(requested_at: Option<Instant>, now: Instant) -> bool {
    requested_at
        .is_some_and(|start| now.saturating_duration_since(start) >= Duration::from_secs(15))
}

fn correlated_drive(snapshot: &device::Snapshot, location: u64) -> bool {
    let boots: Vec<_> = snapshot.devices.iter().filter(|d| d.bootloader()).collect();
    boots.len() == 1
        && boots[0].location == location
        && snapshot.mounts.len() == 1
        && snapshot.mounts[0]
            .info
            .lines()
            .any(|l| l.trim() == "Board-ID: NocFree &")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_action_wait_does_not_expire_but_dispatched_attempt_does() {
        let now = Instant::now();
        assert!(!drive_deadline_passed(
            None,
            now + Duration::from_secs(3600)
        ));
        assert!(!drive_deadline_passed(
            Some(now),
            now + Duration::from_secs(14)
        ));
        assert!(drive_deadline_passed(
            Some(now),
            now + Duration::from_secs(15)
        ));
    }
    #[test]
    fn drive_must_follow_the_selected_stage_on_the_same_usb_connection() {
        let mut snapshot = device::Snapshot {
            devices: vec![device::Device {
                location: 10,
                vendor: 0x239a,
                product: 0x29,
                name: "NocFree &".into(),
            }],
            mounts: vec![device::BootMount {
                path: "/fixture".into(),
                info: "Model: NocFree &\nBoard-ID: NocFree &".into(),
            }],
        };
        assert!(correlated_drive(&snapshot, 10));
        assert!(!correlated_drive(&snapshot, 11));
        snapshot.devices.push(snapshot.devices[0].clone());
        assert!(!correlated_drive(&snapshot, 10));
        snapshot.devices.pop();
        snapshot.mounts.clear();
        assert!(!correlated_drive(&snapshot, 10));
    }
}

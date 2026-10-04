//! Explicitly armed recovery guide. Requests recovery only; never writes firmware.
use crate::{device, session::Session};
use nocfree_companion::experimental_recovery::{ArmedRequest, Role};
use nocfree_companion::recovery_journey::Procedure;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub(crate) fn run(
    role: Role,
    cancelled: Arc<AtomicBool>,
    progress: std::sync::mpsc::Sender<Procedure>,
) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("Recovery mode currently supports macOS only.".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "Couldn’t start recovery.".to_owned())?;
    runtime.block_on(async move {
        let mut session = Session::new();
        session.select(if role == Role::Right { device::Role::Right } else { device::Role::Left });
        let mut disconnected = false;
        let mut last_procedure = None;
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
            if requested_at.is_none() && !matches!(last_procedure, Some(Procedure::FactoryLeft | Procedure::FactoryRight | Procedure::FactoryReceiver)) {
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
                    let _ = progress.send(Procedure::StartupApp);
                    // A successful reset may disconnect before acknowledgement.
                    // Only the subsequently correlated drive determines success.
                    let _=request.request_detach(&cancelled).await;
                }
            }
            let snapshot = device::discover()?;
            if let Some(location)=requested_location {
                if correlated_drive(&snapshot,location) { return Ok(()); }
            } else {
                let (procedure, ready) = factory_observation(&mut session, role, &mut disconnected, snapshot)?;
                if let Some(procedure) = procedure && last_procedure != Some(procedure) {
                    let _ = progress.send(procedure);
                    last_procedure = Some(procedure);
                }
                if ready { return Ok(()); }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
}

// Session remains the single source of normal-device/port and boot metadata binding.
fn factory_observation(
    session: &mut Session,
    role: Role,
    disconnected: &mut bool,
    snapshot: device::Snapshot,
) -> Result<(Option<Procedure>, bool), String> {
    let normal_role = if role == Role::Right {
        device::Role::Right
    } else {
        device::Role::Left
    };
    if !*disconnected {
        if !snapshot
            .devices
            .iter()
            .any(|d| d.role() == Some(normal_role) || d.bootloader())
            && snapshot.mounts.is_empty()
        {
            *disconnected = true;
        }
        return Ok((None, false));
    }
    session.observe(Ok(snapshot.clone()));
    let view = session.view();
    if let Some(error) = view.error {
        return Err(error);
    }
    let procedure = match session.factory_role() {
        Some(device::Role::Left) if role == Role::Receiver => Some(Procedure::FactoryReceiver),
        Some(device::Role::Left) => Some(Procedure::FactoryLeft),
        Some(device::Role::Right) => Some(Procedure::FactoryRight),
        None if session.identified_normal() => Some(Procedure::Manual),
        _ => None,
    };
    if view.can_save
        && !snapshot.mounts[0]
            .info
            .lines()
            .any(|l| l.trim() == "Board-ID: NocFree &")
    {
        return Err("This recovery drive’s board identity is unfamiliar.".into());
    }
    Ok((procedure, view.can_save))
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
        && crate::session::validate_metadata(&snapshot.mounts[0].info).is_ok()
        && snapshot.mounts[0]
            .info
            .lines()
            .any(|l| l.trim() == "Board-ID: NocFree &")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn factory_normal(role: Role) -> device::Snapshot {
        device::Snapshot {
            devices: vec![device::Device {
                location: 10,
                vendor: if role == Role::Right { 0x239a } else { 0x2886 },
                product: if role == Role::Right { 0x80d8 } else { 0x8029 },
                name: if role == Role::Right {
                    "NocFree nRF52833 Right"
                } else {
                    "NocFree & ANSI"
                }
                .into(),
            }],
            mounts: vec![],
        }
    }
    fn factory_boot(location: u64) -> device::Snapshot {
        device::Snapshot {
            devices: vec![device::Device {
                location,
                vendor: 0x239a,
                product: 0x29,
                name: "NocFree &".into(),
            }],
            mounts: vec![device::BootMount {
                path: "/fixture".into(),
                info: "UF2 Bootloader 0.9.2-39-g0147d71\nModel: NocFree &\nBoard-ID: NocFree &"
                    .into(),
            }],
        }
    }
    #[test]
    fn factory_flow_requires_disconnect_identification_and_same_port_drive() {
        for role in [Role::Left, Role::Right, Role::Receiver] {
            let mut session = Session::new();
            session.select(if role == Role::Right {
                device::Role::Right
            } else {
                device::Role::Left
            });
            let mut disconnected = false;
            assert_eq!(
                factory_observation(&mut session, role, &mut disconnected, factory_boot(10))
                    .unwrap(),
                (None, false)
            );
            assert_eq!(
                factory_observation(&mut session, role, &mut disconnected, factory_normal(role))
                    .unwrap(),
                (None, false)
            );
            factory_observation(
                &mut session,
                role,
                &mut disconnected,
                device::Snapshot::default(),
            )
            .unwrap();
            let (procedure, ready) =
                factory_observation(&mut session, role, &mut disconnected, factory_normal(role))
                    .unwrap();
            assert_eq!(
                procedure,
                Some(match role {
                    Role::Left => Procedure::FactoryLeft,
                    Role::Right => Procedure::FactoryRight,
                    Role::Receiver => Procedure::FactoryReceiver,
                })
            );
            assert!(!ready);
            assert!(
                factory_observation(&mut session, role, &mut disconnected, factory_boot(10))
                    .unwrap()
                    .1
            );
            assert!(
                factory_observation(&mut session, role, &mut disconnected, factory_boot(11))
                    .is_err()
            );
        }
    }
    #[test]
    fn factory_ready_requires_reviewed_metadata_and_unique_mount() {
        let mut session = Session::new();
        session.select(device::Role::Left);
        let mut disconnected = true;
        factory_observation(
            &mut session,
            Role::Left,
            &mut disconnected,
            factory_normal(Role::Left),
        )
        .unwrap();
        assert!(
            factory_observation(
                &mut session,
                Role::Left,
                &mut disconnected,
                factory_boot(10)
            )
            .unwrap()
            .1
        );
        let mut ambiguous = factory_boot(10);
        ambiguous.mounts.push(ambiguous.mounts[0].clone());
        assert!(
            factory_observation(&mut session, Role::Left, &mut disconnected, ambiguous).is_err()
        );
        let mut session = Session::new();
        session.select(device::Role::Left);
        factory_observation(
            &mut session,
            Role::Left,
            &mut disconnected,
            factory_normal(Role::Left),
        )
        .unwrap();
        let mut unknown = factory_boot(10);
        unknown.mounts[0].info = "Model: NocFree &\nBoard-ID: NocFree &".into();
        assert!(factory_observation(&mut session, Role::Left, &mut disconnected, unknown).is_err());
    }
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
                info: "UF2 Bootloader 0.9.2-39-g0147d71\nModel: NocFree &\nBoard-ID: NocFree &"
                    .into(),
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

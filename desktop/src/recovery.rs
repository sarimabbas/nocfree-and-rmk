//! Explicitly armed recovery guide. Requests recovery only; never writes firmware.
use crate::recovery_journey::Procedure;
use crate::runtime_recovery::{ArmedRequest, Role, matches as runtime_matches};
use crate::{device, session::Session};
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
) -> Result<Session, String> {
    if !cfg!(target_os = "macos") {
        return Err("Recovery mode currently supports macOS only.".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "Couldn’t start recovery.".to_owned())?;
    let result = runtime.block_on(async move {
        let mut session = Session::new();
        session.select_recovery_role(role);
        let mut disconnected = false;
        let mut last_procedure = None;
        let mut requested_location=None;
        let mut requested_at=None;
        let mut inventory = DiscoveryPoll::default();
        // Wait for the user's physical action without expiring while they read.
        // Once a matching runtime device appears, dispatch exactly once; the armed request
        // and subsequent drive observation retain their finite deadlines.
        loop {
            if cancelled.load(Ordering::Relaxed) { return Err("Recovery cancelled.".into()); }
            if drive_deadline_passed(requested_at, Instant::now()) {
                return Err("The recovery drive didn’t appear. Check its power and USB connection, then try again.".into());
            }
            if requested_at.is_none() && !matches!(last_procedure, Some(Procedure::FactoryLeft | Procedure::FactoryRight | Procedure::FactoryReceiver)) {
                let devices=tokio::time::timeout(Duration::from_secs(2),nusb::list_devices()).await
                    .map_err(|_| "USB discovery took too long.")?.map_err(|_| "Couldn’t inspect USB devices.")?;
                let mut targets=devices.filter(|d| runtime_matches(role,d));
                if let Some(target)=targets.next() {
                    if targets.next().is_some() { return Err("Connect only one of the selected component.".into()); }
                    let request=ArmedRequest::arm(role,&target).map_err(str::to_owned)?;
                    if cancelled.load(Ordering::Relaxed) { return Err("Recovery cancelled.".into()); }
                    #[cfg(target_os="macos")]
                    { requested_location=Some(u64::from(target.location_id())); }
                    requested_at=Some(Instant::now());
                    let _ = progress.send(Procedure::RuntimeApp);
                    // A successful reset may disconnect before acknowledgement.
                    // Only the subsequently correlated drive determines success.
                    let _=request.request_detach(&cancelled).await;
                }
            }
            if cancelled.load(Ordering::Relaxed) { return Err("Recovery cancelled.".into()); }
            let Some(snapshot) = inventory.poll(requested_at).await else {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            };
            let snapshot = snapshot?;
            if cancelled.load(Ordering::Relaxed) { return Err("Recovery cancelled.".into()); }
            if let Some(location)=requested_location {
                // The runtime endpoint is bound locally before requesting recovery.
                // Bind and validate when recovery begins enumerating; discovery may
                // still see the runtime device until its detach/reset has taken effect.
                if snapshot.devices.iter().any(|device| device.bootloader()) || !snapshot.mounts.is_empty() {
                    session.bind_recovery(location);
                    session.observe(Ok(snapshot));
                    if let Some(error) = session.view().error { return Err(error); }
                    if session.view().can_save { return Ok(session); }
                }
            } else {
                let (procedure, ready) = factory_observation(&mut session, role, &mut disconnected, snapshot)?;
                if let Some(procedure) = procedure && last_procedure != Some(procedure) {
                    let _ = progress.send(procedure);
                    last_procedure = Some(procedure);
                }
                if ready { return Ok(session); }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });
    // A cancelled read-only OS inventory can finish in the background; it never
    // carries an armed request or accepts a recovery drive after cancellation.
    runtime.shutdown_timeout(Duration::from_millis(100));
    result
}

// Only one read-only inventory runs at a time. Slow ioreg must not block
// USB recovery polling. An inventory started before a
// detach request cannot prove that request's resulting recovery drive.
#[derive(Default)]
struct DiscoveryPoll {
    pending: Option<(
        Instant,
        tokio::task::JoinHandle<Result<device::Snapshot, String>>,
    )>,
}
impl DiscoveryPoll {
    async fn poll(&mut self, after: Option<Instant>) -> Option<Result<device::Snapshot, String>> {
        if self.pending.is_none() {
            self.pending = Some((
                Instant::now(),
                tokio::task::spawn_blocking(device::discover),
            ));
        }
        let (started, task) = self.pending.as_ref()?;
        if !task.is_finished() {
            if started.elapsed() >= Duration::from_secs(3) {
                return Some(Err(
                    "USB discovery took too long. Try recovery again.".into()
                ));
            }
            return None;
        }
        let stale = after.is_some_and(|after| *started < after);
        let (_, task) = self.pending.take()?;
        let result = task
            .await
            .unwrap_or_else(|_| Err("Couldn’t inspect USB devices.".into()));
        if stale { None } else { Some(result) }
    }
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
        let procedure = snapshot
            .devices
            .iter()
            .any(|d| d.role() == Some(normal_role))
            .then_some(Procedure::Reconnect);
        return Ok((procedure, false));
    }
    session.observe(Ok(snapshot.clone()));
    let view = session.view();
    if let Some(error) = view.error {
        return Err(error);
    }
    let procedure =
        match session.factory_role() {
            Some(device::Role::Left) if role == Role::Receiver => Some(Procedure::FactoryReceiver),
            Some(device::Role::Left) => Some(Procedure::FactoryLeft),
            Some(device::Role::Right) => Some(Procedure::FactoryRight),
            None if session.identified_normal() => return Err(
                "The RMK recovery interface wasn’t found. Check the USB connection and try again."
                    .into(),
            ),
            _ => None,
        };
    Ok((procedure, view.can_save))
}

fn drive_deadline_passed(requested_at: Option<Instant>, now: Instant) -> bool {
    requested_at
        .is_some_and(|start| now.saturating_duration_since(start) >= Duration::from_secs(15))
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
                (Some(Procedure::Reconnect), false)
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
    fn drive_must_follow_the_selected_runtime_on_the_same_usb_connection() {
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
        let mut session = Session::new();
        session.select(device::Role::Left);
        session.bind_recovery(10);
        session.observe(Ok(snapshot.clone()));
        assert!(session.view().can_save);
        let mut wrong_port = Session::new();
        wrong_port.select(device::Role::Left);
        wrong_port.bind_recovery(11);
        wrong_port.observe(Ok(snapshot.clone()));
        assert!(!wrong_port.view().can_save);
        assert!(wrong_port.view().error.is_some());
        snapshot.devices.push(snapshot.devices[0].clone());
        session.observe(Ok(snapshot.clone()));
        assert!(!session.view().can_save);
        snapshot.devices.pop();
        snapshot.mounts.clear();
        session.observe(Ok(snapshot.clone()));
        assert!(!session.view().can_save);
    }
    #[tokio::test]
    async fn slow_inventory_does_not_block_hot_polling_or_start_another_inventory() {
        let (release, wait) = std::sync::mpsc::channel();
        let started = Instant::now();
        let task = tokio::task::spawn_blocking(move || {
            wait.recv().unwrap();
            Ok(device::Snapshot::default())
        });
        let mut inventory = DiscoveryPoll {
            pending: Some((started, task)),
        };
        assert!(inventory.poll(None).await.is_none());
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(inventory.poll(None).await.is_none());
        assert_eq!(inventory.pending.as_ref().unwrap().0, started);
        release.send(()).unwrap();
        while !inventory.pending.as_ref().unwrap().1.is_finished() {
            tokio::task::yield_now().await;
        }
        assert!(inventory.poll(None).await.unwrap().is_ok());
        assert!(inventory.pending.is_none());
    }
    #[tokio::test]
    async fn inventory_started_before_detach_is_discarded_even_if_it_finishes_afterward() {
        let started = Instant::now();
        let requested = started + Duration::from_millis(1);
        let task = tokio::task::spawn_blocking(|| Ok(device::Snapshot::default()));
        while !task.is_finished() {
            tokio::task::yield_now().await;
        }
        let mut inventory = DiscoveryPoll {
            pending: Some((started, task)),
        };
        assert!(inventory.poll(Some(requested)).await.is_none());
        assert!(inventory.pending.is_none());
        let task = tokio::task::spawn_blocking(|| Ok(device::Snapshot::default()));
        while !task.is_finished() {
            tokio::task::yield_now().await;
        }
        inventory.pending = Some((requested, task));
        assert!(inventory.poll(Some(requested)).await.unwrap().is_ok());
    }
    #[test]
    fn cancellation_shutdown_does_not_wait_for_a_blocked_read_only_inventory() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (release, wait) = std::sync::mpsc::channel();
        let (started, ready) = std::sync::mpsc::channel();
        runtime.spawn_blocking(move || {
            started.send(()).unwrap();
            wait.recv().unwrap();
        });
        ready.recv_timeout(Duration::from_secs(1)).unwrap();
        let cancelled_at = Instant::now();
        runtime.shutdown_timeout(Duration::from_millis(100));
        release.send(()).unwrap();
        assert!(cancelled_at.elapsed() < Duration::from_secs(1));
    }
}

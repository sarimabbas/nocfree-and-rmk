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
    run_with(role, cancelled, progress, false)
}
/// Backup can archive an explicitly selected, already-mounted drive without
/// claiming its role is proven or granting firmware-write eligibility.
pub(crate) fn run_backup(
    role: Role,
    cancelled: Arc<AtomicBool>,
    progress: std::sync::mpsc::Sender<Procedure>,
) -> Result<Session, String> {
    run_with(role, cancelled, progress, true)
}
fn run_with(
    role: Role,
    cancelled: Arc<AtomicBool>,
    progress: std::sync::mpsc::Sender<Procedure>,
    archive_only: bool,
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
        let mut mounting_at=None;
        let mut inventory = DiscoveryPoll::default();
        let mut entry_gate = crate::completion_gate::CompletionGate::default();
        let mut completion_gate = crate::completion_gate::CompletionGate::default();
        let mut ready_drive = false;
        let mut archive_adopted = false;
        // Wait for the user's physical action without expiring while they read.
        // Once a matching runtime device appears, dispatch exactly once; the armed request
        // and subsequent drive observation retain their finite deadlines.
        loop {
            if cancelled.load(Ordering::Relaxed) { return Err("Recovery cancelled.".into()); }
            if !ready_drive && drive_deadline_passed(requested_at.or(mounting_at), Instant::now()) {
                return Err("The recovery drive didn’t appear. Check its power and USB connection, then try again.".into());
            }
            if requested_at.is_none() && !matches!(last_procedure, Some(Procedure::FactoryLeft | Procedure::FactoryRight | Procedure::FactoryReceiver)) {
                let devices=tokio::time::timeout(Duration::from_secs(2),nusb::list_devices()).await
                    .map_err(|_| "USB discovery took too long.")?.map_err(|_| "Couldn’t inspect USB devices.")?;
                let mut targets=devices.filter(|d| runtime_matches(role,d));
                if let Some(target)=targets.next() {
                    if targets.next().is_some() { return Err("Connect only one of the selected component.".into()); }
                    if !entry_gate.ready(target.id(), true, Instant::now()) {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                    let request=ArmedRequest::arm(role,&target).map_err(str::to_owned)?;
                    if cancelled.load(Ordering::Relaxed) { return Err("Recovery cancelled.".into()); }
                    #[cfg(target_os="macos")]
                    { requested_location=Some(u64::from(target.location_id())); }
                    requested_at=Some(Instant::now());
                    let _ = progress.send(Procedure::RuntimeApp);
                    // A successful reset may disconnect before acknowledgement.
                    // Only the subsequently correlated drive determines success.
                    match request.request_detach(&cancelled).await {
                        Err(crate::runtime_recovery::DispatchError::NotSent(message)) => return Err(message.into()),
                        Ok(()) | Err(crate::runtime_recovery::DispatchError::OutcomeUnknown(_)) => {},
                    }
                } else { entry_gate.reset(); }
            }
            if cancelled.load(Ordering::Relaxed) { return Err("Recovery cancelled.".into()); }
            let Some(snapshot) = inventory.poll(requested_at).await else {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            };
            let snapshot = snapshot?;
            let completion_key = if snapshot.mounts.len() == 1 {
                let boot: Vec<_> = snapshot.devices.iter().filter(|device| device.bootloader()).collect();
                (boot.len() == 1).then(|| (boot[0].location, snapshot.mounts[0].clone()))
            } else { None };
            ready_drive = false;
            if cancelled.load(Ordering::Relaxed) { return Err("Recovery cancelled.".into()); }
            if archive_only && requested_location.is_none() && (archive_adopted || snapshot.devices.iter().any(|d|d.bootloader())) {
                mounting_at.get_or_insert_with(Instant::now);
                let known=crate::status_cache::recovery_locations();
                if recovery_part_conflicts(role,&known,&snapshot) {
                    return Err("The recovery drive belongs to another part. Select that part to back it up.".into());
                }
                if archive_adopted { session.observe(Ok(snapshot.clone())); }
                else { archive_adopted = session.adopt_archive_drive(snapshot.clone())?; }
                ready_drive = session.view().can_save;
                if let Some(key) = completion_key.clone()
                    && completion_gate.ready(key, ready_drive, Instant::now()) {
                    if cancelled.load(Ordering::Relaxed) { return Err("Recovery cancelled.".into()); }
                    return Ok(session);
                }
                if !ready_drive || completion_key.is_none() { completion_gate.reset(); }
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
            if cancelled.load(Ordering::Relaxed) { return Err("Recovery cancelled.".into()); }
            if let Some(location)=requested_location {
                // The runtime endpoint is bound locally before requesting recovery.
                // Bind and validate when recovery begins enumerating; discovery may
                // still see the runtime device until its detach/reset has taken effect.
                if snapshot.devices.iter().any(|device| device.bootloader()) || !snapshot.mounts.is_empty() {
                    session.bind_recovery(location);
                    session.observe(Ok(snapshot));
                    if let Some(error) = session.view().error { return Err(error); }
                    ready_drive = session.view().can_save;
                }
            } else {
                let (procedure, ready) = factory_observation(&mut session, role, &mut disconnected, snapshot)?;
                if let Some(procedure) = procedure && last_procedure != Some(procedure) {
                    let _ = progress.send(procedure);
                    last_procedure = Some(procedure);
                }
                ready_drive = ready;
            }
            if let Some(key) = completion_key {
                if completion_gate.ready(key, ready_drive, Instant::now()) { return Ok(session); }
            } else { completion_gate.reset(); }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });
    // A cancelled read-only OS inventory can finish in the background; it never
    // carries an armed request or accepts a recovery drive after cancellation.
    runtime.shutdown_timeout(Duration::from_millis(100));
    if let Ok(session) = &result
        && let Ok((role, location, _)) = session.recovery_binding()
    {
        crate::status_cache::confirm(role, location);
    }
    result
}

fn recovery_part_conflicts(
    role: Role,
    known: &[Option<u64>; 3],
    snapshot: &device::Snapshot,
) -> bool {
    let selected = match role {
        Role::Left => 0,
        Role::Right => 1,
        Role::Receiver => 2,
    };
    known.iter().enumerate().any(|(index, location)| {
        index != selected
            && location.is_some_and(|location| {
                snapshot
                    .devices
                    .iter()
                    .any(|d| d.bootloader() && d.location == location)
            })
    })
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
    // Stock left and dongle have indistinguishable USB descriptors. Require
    // their shared identities to disappear before the owner reconnects only
    // the selected part; never pick one of two identities by enumeration order.
    let ambiguous_stock = role != Role::Right
        && snapshot
            .devices
            .iter()
            .filter(|device| device.factory_left())
            .count()
            > 1;
    if ambiguous_stock || session.shared_factory_connection_conflicts(&snapshot) {
        session.select_recovery_role(role);
        *disconnected = false;
        return Ok((Some(Procedure::Reconnect), false));
    }
    if !*disconnected {
        // The uniquely identified right half can be isolated while its paired
        // factory left stays powered over normal USB to provide modifier state.
        // Shared left/dongle identities still require their existing isolation.
        if !snapshot.devices.iter().any(|d| {
            d.role() == Some(normal_role)
                || role != Role::Right && d.factory_left()
                || d.bootloader()
        }) && snapshot.mounts.is_empty()
        {
            *disconnected = true;
        }
        let procedure = snapshot
            .devices
            .iter()
            .any(|d| {
                d.role() == Some(normal_role)
                    || role != Role::Right && d.factory_left()
                    || d.bootloader()
            })
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
    fn backup_does_not_relabel_a_known_other_component() {
        let snapshot = factory_boot(10);
        for (index, role) in [Role::Left, Role::Right, Role::Receiver]
            .into_iter()
            .enumerate()
        {
            let mut known = [None; 3];
            known[index] = Some(10);
            assert!(!recovery_part_conflicts(role, &known, &snapshot));
            for selected in [Role::Left, Role::Right, Role::Receiver] {
                if selected != role {
                    assert!(recovery_part_conflicts(selected, &known, &snapshot));
                }
            }
        }
        assert!(!recovery_part_conflicts(
            Role::Receiver,
            &[None; 3],
            &snapshot
        ));
    }
    #[test]
    fn factory_flow_requires_disconnect_identification_and_same_port_drive() {
        for role in [Role::Left, Role::Right, Role::Receiver] {
            let mut session = Session::new();
            session.select_recovery_role(role);
            let mut disconnected = false;
            assert_eq!(
                factory_observation(&mut session, role, &mut disconnected, factory_boot(10))
                    .unwrap(),
                (Some(Procedure::Reconnect), false)
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
    fn factory_right_can_recover_with_its_paired_left_powered_on_usb() {
        let mut session = Session::new();
        session.select_recovery_role(Role::Right);
        let mut disconnected = false;
        let mut left = factory_normal(Role::Left);
        left.devices[0].location = 20;
        let with_left = |mut snapshot: device::Snapshot| {
            snapshot.devices.extend(left.devices.clone());
            snapshot
        };
        assert_eq!(
            factory_observation(
                &mut session,
                Role::Right,
                &mut disconnected,
                with_left(factory_normal(Role::Right))
            )
            .unwrap(),
            (Some(Procedure::Reconnect), false)
        );
        assert!(!disconnected);
        assert_eq!(
            factory_observation(&mut session, Role::Right, &mut disconnected, left.clone())
                .unwrap(),
            (None, false)
        );
        assert!(disconnected);
        assert_eq!(
            factory_observation(
                &mut session,
                Role::Right,
                &mut disconnected,
                with_left(factory_normal(Role::Right))
            )
            .unwrap(),
            (Some(Procedure::FactoryRight), false)
        );
        assert_eq!(
            factory_observation(
                &mut session,
                Role::Right,
                &mut disconnected,
                with_left(factory_boot(10))
            )
            .unwrap(),
            (None, true)
        );
        assert_eq!(session.factory_recovery_role(), Some(Role::Right));
        assert!(
            factory_observation(
                &mut session,
                Role::Right,
                &mut disconnected,
                with_left(factory_boot(11))
            )
            .is_err()
        );
    }
    #[test]
    fn stock_dongle_recovery_requires_all_shared_usb_identities_to_disconnect() {
        let mut session = Session::new();
        session.select_recovery_role(Role::Receiver);
        let mut disconnected = false;
        let mut both = factory_normal(Role::Receiver);
        let mut left = factory_normal(Role::Left).devices.remove(0);
        left.location = 20;
        both.devices.push(left.clone());
        assert_eq!(
            factory_observation(&mut session, Role::Receiver, &mut disconnected, both).unwrap(),
            (Some(Procedure::Reconnect), false)
        );
        // Unplugging the dongle alone leaves the factory left's identical USB
        // identity. It cannot satisfy the disconnect or identify the dongle.
        let left_only = device::Snapshot {
            devices: vec![left],
            mounts: vec![],
        };
        assert_eq!(
            factory_observation(&mut session, Role::Receiver, &mut disconnected, left_only)
                .unwrap(),
            (Some(Procedure::Reconnect), false)
        );
        assert!(!disconnected);
        assert!(session.recovery_binding().is_err());
        factory_observation(
            &mut session,
            Role::Receiver,
            &mut disconnected,
            device::Snapshot::default(),
        )
        .unwrap();
        assert_eq!(
            factory_observation(
                &mut session,
                Role::Receiver,
                &mut disconnected,
                factory_normal(Role::Receiver)
            )
            .unwrap(),
            (Some(Procedure::FactoryReceiver), false)
        );
        assert!(
            factory_observation(
                &mut session,
                Role::Receiver,
                &mut disconnected,
                factory_boot(10)
            )
            .unwrap()
            .1
        );
        assert!(session.shared_factory_recovery());
        assert_eq!(session.recovery_binding().unwrap().0, Role::Receiver);
    }
    #[test]
    fn reconnecting_stock_left_during_dongle_dfu_invalidates_the_binding() {
        let mut session = Session::new();
        session.select_recovery_role(Role::Receiver);
        let mut disconnected = true;
        factory_observation(
            &mut session,
            Role::Receiver,
            &mut disconnected,
            factory_normal(Role::Receiver),
        )
        .unwrap();
        let mut mixed = factory_boot(10);
        let mut left = factory_normal(Role::Left).devices.remove(0);
        left.location = 20;
        mixed.devices.push(left);
        assert_eq!(
            factory_observation(&mut session, Role::Receiver, &mut disconnected, mixed).unwrap(),
            (Some(Procedure::Reconnect), false)
        );
        assert!(!disconnected);
        assert!(!session.shared_factory_recovery());
        assert!(session.recovery_binding().is_err());
        // An isolated boot drive after that conflict still cannot infer its role.
        assert_eq!(
            factory_observation(
                &mut session,
                Role::Receiver,
                &mut disconnected,
                factory_boot(10)
            )
            .unwrap(),
            (Some(Procedure::Reconnect), false)
        );
        assert!(session.recovery_binding().is_err());
    }
    #[test]
    fn stock_origin_requires_observed_normal_binding_not_defaults_or_an_archive() {
        let mut session = Session::new();
        session.select_recovery_role(Role::Receiver);
        session.observe(Ok(factory_boot(10)));
        assert!(!session.shared_factory_recovery());
        session.bind_recovery(10);
        session.observe(Ok(factory_boot(10)));
        assert!(session.recovery_binding().is_ok());
        assert!(!session.shared_factory_recovery());
        let mut archive = Session::new();
        archive.select_recovery_role(Role::Receiver);
        assert!(archive.adopt_archive_drive(factory_boot(10)).unwrap());
        assert!(!archive.shared_factory_recovery());
        assert!(archive.recovery_binding().is_err());
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

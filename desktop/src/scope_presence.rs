//! USB attachment is a prerequisite; typing route and wireless telemetry are not.
use crate::{device_status::UsbKey, runtime_recovery::Role};
use statig::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

fn shared(entry: &(u64, u64, u64, String)) -> bool {
    crate::device::factory_keyboard_identity(entry.1, entry.2, &entry.3)
}

fn bootloader(entry: &(u64, u64, u64, String)) -> bool {
    (entry.1, entry.2) == (0x239a, 0x0029)
}
fn identifiable(entry: &(u64, u64, u64, String)) -> bool {
    shared(entry) || bootloader(entry)
}

static GENERATION: AtomicU64 = AtomicU64::new(1);
fn generation() -> u64 {
    GENERATION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .expect("Identification generation exhausted")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Identification {
    Idle,
    Disconnect(Role),
    Connect(Role),
}

mod identification_machine {
    use super::*;
    type Candidate = (u64, u64, u64, String);
    pub(super) enum Proof {
        Disconnected,
        Connected(Candidate),
    }
    pub struct Data {
        pub bindings: [Option<u64>; 3],
        pub identities: [Option<(u64, u64, String)>; 3],
        pub ticket: u64,
        pub(super) pending: Option<(Instant, Proof)>,
    }
    pub enum Event<'a> {
        Start(Role),
        Observe(u64, &'a UsbKey, bool, Instant),
        Next(u64, Instant),
        Cancel,
    }
    impl Data {
        pub fn ready_for_next(&self, now: Instant) -> bool {
            self.pending.as_ref().is_some_and(|(seen, _)| {
                now.checked_duration_since(*seen)
                    .is_some_and(|age| age <= std::time::Duration::from_secs(5))
            })
        }
        fn fresh(&mut self, event: &Event<'_>) -> Option<Vec<u64>> {
            let Event::Observe(ticket, devices, true, _) = event else {
                return None;
            };
            if *ticket != self.ticket {
                return None;
            }
            for (binding, identity) in self.bindings.iter_mut().zip(&mut self.identities) {
                if binding.is_some_and(|location| {
                    devices.iter().filter(|entry| entry.0 == location).count() != 1
                        || !devices.iter().any(|entry| {
                            entry.0 == location
                                && identifiable(entry)
                                && identity.as_ref().is_some_and(|identity| {
                                    identity.0 == entry.1
                                        && identity.1 == entry.2
                                        && identity.2 == entry.3
                                })
                        })
                }) {
                    *binding = None;
                    *identity = None;
                }
            }
            Some(
                devices
                    .iter()
                    .filter(|entry| identifiable(entry) && !self.bindings.contains(&Some(entry.0)))
                    .map(|entry| entry.0)
                    .collect(),
            )
        }
    }
    #[state_machine(
        initial = "State::idle()",
        state(derive(Debug)),
        superstate(derive(Debug))
    )]
    impl Data {
        #[state(superstate = "root")]
        fn idle(&mut self, event: &Event<'_>) -> Outcome<State> {
            self.fresh(event);
            Super
        }
        #[state(superstate = "root")]
        fn disconnect(&mut self, role: &mut Role, event: &Event<'_>) -> Outcome<State> {
            if let Event::Next(ticket, now) = event
                && *ticket == self.ticket
                && self.ready_for_next(*now)
                && matches!(self.pending, Some((_, Proof::Disconnected)))
            {
                self.pending = None;
                self.ticket = generation();
                return Transition(State::connect(*role));
            }
            let ready = self
                .fresh(event)
                .is_some_and(|unassigned| unassigned.is_empty());
            if let Event::Observe(ticket, _, _, now) = event
                && *ticket == self.ticket
            {
                self.pending = ready.then_some((*now, Proof::Disconnected));
            }
            Super
        }
        #[state(superstate = "root")]
        fn connect(&mut self, role: &mut Role, event: &Event<'_>) -> Outcome<State> {
            if let Event::Next(ticket, now) = event
                && *ticket == self.ticket
                && self.ready_for_next(*now)
                && let Some((_, Proof::Connected(candidate))) = self.pending.take()
            {
                let (location, vendor, product, name) = candidate;
                self.bindings[index(*role)] = Some(location);
                self.identities[index(*role)] = Some((vendor, product, name));
                self.ticket = generation();
                return Transition(State::idle());
            }
            let mut candidate = None;
            if let Some(unassigned) = self.fresh(event)
                && let [location] = unassigned.as_slice()
            {
                let Event::Observe(_, devices, _, _) = event else {
                    unreachable!()
                };
                if devices.iter().filter(|entry| entry.0 == *location).count() == 1
                    && (*role != Role::Right
                        || devices
                            .iter()
                            .any(|entry| entry.0 == *location && bootloader(entry)))
                {
                    candidate = devices.iter().find(|entry| entry.0 == *location).cloned();
                }
            }
            if let Event::Observe(ticket, _, _, now) = event
                && *ticket == self.ticket
            {
                self.pending = candidate.map(|candidate| (*now, Proof::Connected(candidate)));
            }
            Super
        }
        #[superstate]
        fn root(&mut self, event: &Event<'_>) -> Outcome<State> {
            match event {
                Event::Start(role) => {
                    self.pending = None;
                    self.bindings[index(*role)] = None;
                    self.identities[index(*role)] = None;
                    self.ticket = generation();
                    Transition(State::disconnect(*role))
                }
                Event::Cancel => {
                    self.pending = None;
                    self.ticket = generation();
                    Transition(State::idle())
                }
                _ => Handled,
            }
        }
    }
}

/// Read-only physical identification. No HID command or recovery entry is issued.
pub(crate) struct Identifier(StateMachine<identification_machine::Data>);
impl Default for Identifier {
    fn default() -> Self {
        Self(
            identification_machine::Data {
                bindings: [None; 3],
                identities: [None, None, None],
                ticket: generation(),
                pending: None,
            }
            .state_machine(),
        )
    }
}
impl Identifier {
    pub(crate) fn start(&mut self, role: Role) {
        self.0.handle(&identification_machine::Event::Start(role));
    }
    pub(crate) fn ticket(&self) -> u64 {
        self.0.inner().ticket
    }
    pub(crate) fn observe(&mut self, ticket: u64, devices: &UsbKey, fresh: bool) {
        self.observe_at(ticket, devices, fresh, Instant::now());
    }
    pub(crate) fn observe_at(&mut self, ticket: u64, devices: &UsbKey, fresh: bool, now: Instant) {
        self.0.handle(&identification_machine::Event::Observe(
            ticket, devices, fresh, now,
        ));
    }
    pub(crate) fn can_next(&self) -> bool {
        self.0.inner().ready_for_next(Instant::now())
    }
    pub(crate) fn next(&mut self, ticket: u64) -> bool {
        self.next_at(ticket, Instant::now())
    }
    fn next_at(&mut self, ticket: u64, now: Instant) -> bool {
        let before = self.ticket();
        self.0
            .handle(&identification_machine::Event::Next(ticket, now));
        self.ticket() != before
    }
    pub(crate) fn cancel(&mut self) {
        self.0.handle(&identification_machine::Event::Cancel);
    }
    pub(crate) fn bindings(&self) -> [Option<u64>; 3] {
        self.0.inner().bindings
    }
    pub(crate) fn state(&self) -> Identification {
        use identification_machine::State;
        match self.0.state() {
            State::Idle {} => Identification::Idle,
            State::Disconnect { role } => Identification::Disconnect(*role),
            State::Connect { role } => Identification::Connect(*role),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Presence {
    Connected,
    Disconnected,
    Unidentified,
}

impl Presence {
    pub(crate) fn connected(self) -> bool {
        self == Self::Connected
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Connected => "USB connected",
            Self::Disconnected => "Not connected via USB",
            Self::Unidentified => "Identify this part",
        }
    }
}

/// Recovery locations are role bindings, never inferred from a generic DFU name.
/// Shared factory left/dongle descriptors deliberately prove neither role.
pub(crate) fn derive(
    devices: &UsbKey,
    recovery_locations: [Option<u64>; 3],
    fresh: bool,
    identified_locations: [Option<u64>; 3],
) -> [Presence; 3] {
    if !fresh {
        return [Presence::Disconnected; 3];
    }
    let factory_shared = devices.iter().any(shared);
    let unbound_recovery = devices.iter().any(|(location, vendor, product, _)| {
        (*vendor, *product) == (0x239a, 0x0029)
            && !recovery_locations.contains(&Some(*location))
            && !identified_locations.contains(&Some(*location))
    });
    std::array::from_fn(|index| {
        let unique_role = |location| {
            (0..3).filter(|other| *other != index).all(|other| {
                recovery_locations[other] != Some(location)
                    && identified_locations[other] != Some(location)
            })
        };
        let identified = identified_locations[index].is_some_and(|location| {
            unique_role(location)
                && identified_locations
                    .iter()
                    .filter(|bound| **bound == Some(location))
                    .count()
                    == 1
                && devices
                    .iter()
                    .filter(|(found, _, _, _)| *found == location)
                    .count()
                    == 1
                && devices.iter().any(|entry| {
                    entry.0 == location && (bootloader(entry) || index != 1 && shared(entry))
                })
        });
        let normal_devices: Vec<_> = devices
            .iter()
            .filter(|(_, vendor, product, name)| {
                matches!(
                    (index, *vendor, *product, name.as_str()),
                    (0, 0x4c4b, 0x4643, "NocFree RMK")
                        | (1, 0x4c4b, 0x4671, "NocFree RMK Right")
                        | (1, 0x239a, 0x80d8, "NocFree nRF52833 Right")
                        | (2, 0x4c4b, 0x4644, "NocFree RMK Receiver")
                ) || index == 2 && crate::device::factory_dongle_identity(*vendor, *product, name)
            })
            .collect();
        let normal = normal_devices.len();
        if normal_devices.iter().any(|entry| {
            !unique_role(entry.0) || devices.iter().filter(|d| d.0 == entry.0).count() != 1
        }) {
            return Presence::Unidentified;
        }
        let recovery = recovery_locations[index].is_some_and(|location| {
            unique_role(location)
                && recovery_locations
                    .iter()
                    .filter(|bound| **bound == Some(location))
                    .count()
                    == 1
                && devices
                    .iter()
                    .filter(|(found, _, _, _)| *found == location)
                    .count()
                    == 1
                && devices.iter().any(|(found, vendor, product, _)| {
                    *found == location && (*vendor, *product) == (0x239a, 0x0029)
                })
        });
        let same_recovery =
            recovery && identified && recovery_locations[index] == identified_locations[index];
        match normal + usize::from(recovery) + usize::from(identified && !same_recovery) {
            1 => Presence::Connected,
            0 if unbound_recovery || index != 1 && factory_shared => Presence::Unidentified,
            0 => Presence::Disconnected,
            _ => Presence::Unidentified,
        }
    })
}

pub(crate) fn index(role: Role) -> usize {
    match role {
        Role::Left => 0,
        Role::Right => 1,
        Role::Receiver => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn derive(devices: &UsbKey, locations: [Option<u64>; 3], fresh: bool) -> [Presence; 3] {
        super::derive(devices, locations, fresh, [None; 3])
    }

    fn rmk() -> UsbKey {
        vec![
            (1, 0x4c4b, 0x4643, "NocFree RMK".into()),
            (2, 0x4c4b, 0x4671, "NocFree RMK Right".into()),
            (3, 0x4c4b, 0x4644, "NocFree RMK Receiver".into()),
        ]
    }

    #[test]
    fn usb_presence_needs_no_mode_or_wireless_state() {
        assert_eq!(derive(&rmk(), [None; 3], true), [Presence::Connected; 3]);
        assert_eq!(
            derive(&vec![], [None; 3], true),
            [Presence::Disconnected; 3]
        );
        assert_eq!(
            derive(&rmk(), [None; 3], false),
            [Presence::Disconnected; 3]
        );
    }

    #[test]
    fn exact_identity_and_unique_role_are_required() {
        let mut devices = rmk();
        devices[0].3 = "NocFree RMK Right".into();
        devices.push(devices[1].clone());
        assert_eq!(
            derive(&devices, [None; 3], true),
            [
                Presence::Disconnected,
                Presence::Unidentified,
                Presence::Connected
            ]
        );
    }

    #[test]
    fn exact_factory_dongle_is_connected_without_guessing_shared_layouts() {
        let mut devices = vec![(7, 0x2886, 0x8029, "NocFree_Dongle".into())];
        assert_eq!(
            derive(&devices, [None; 3], true),
            [
                Presence::Disconnected,
                Presence::Disconnected,
                Presence::Connected
            ]
        );
        assert_eq!(
            super::derive(&devices, [None; 3], true, [Some(7), None, None])[2],
            Presence::Unidentified
        );
        assert_eq!(
            derive(&devices, [Some(7), None, None], true)[2],
            Presence::Unidentified
        );
        devices.push(devices[0].clone());
        assert_eq!(derive(&devices, [None; 3], true)[2], Presence::Unidentified);
        devices[1].0 = 8;
        assert_eq!(derive(&devices, [None; 3], true)[2], Presence::Unidentified);
        devices.pop();
        devices.push((7, 0x4c4b, 0x4643, "NocFree RMK".into()));
        assert_eq!(
            derive(&devices, [None; 3], true),
            [
                Presence::Unidentified,
                Presence::Disconnected,
                Presence::Unidentified
            ]
        );
    }

    #[test]
    fn every_factory_layout_requires_the_same_explicit_role_identification() {
        for layout in ["ANSI", "ISO", "JP", "KR", "JIS"] {
            let devices = vec![(1, 0x2886, 0x8029, format!("NocFree & {layout}"))];
            assert_eq!(
                super::derive(&devices, [None; 3], true, [None; 3]),
                [
                    Presence::Unidentified,
                    Presence::Disconnected,
                    Presence::Unidentified
                ]
            );
            assert_eq!(
                super::derive(&devices, [None; 3], true, [Some(1), None, None]),
                [
                    Presence::Connected,
                    Presence::Disconnected,
                    Presence::Unidentified
                ]
            );
        }
    }

    #[test]
    fn factory_shared_identity_never_guesses_left_or_dongle() {
        let mut devices = vec![
            (1, 0x2886, 0x8029, "NocFree & ANSI".into()),
            (2, 0x239a, 0x80d8, "NocFree nRF52833 Right".into()),
        ];
        let expected = [
            Presence::Unidentified,
            Presence::Connected,
            Presence::Unidentified,
        ];
        assert_eq!(derive(&devices, [None; 3], true), expected);
        devices.push((3, 0x2886, 0x8029, "NocFree _ ANSI".into()));
        assert_eq!(derive(&devices, [None; 3], true), expected);
    }

    #[test]
    fn recovery_requires_one_exact_role_bound_location() {
        let devices = vec![(9, 0x239a, 0x0029, "NocFree &".into())];
        assert_eq!(
            derive(&devices, [None; 3], true),
            [Presence::Unidentified; 3]
        );
        assert_eq!(
            derive(&devices, [Some(9), None, None], true),
            [
                Presence::Connected,
                Presence::Disconnected,
                Presence::Disconnected
            ]
        );
        assert_eq!(
            derive(&devices, [Some(8), None, None], true),
            [Presence::Unidentified; 3]
        );
        assert!(
            !derive(&devices, [Some(9), Some(9), None], true)
                .iter()
                .any(|p| p.connected())
        );
    }

    #[test]
    fn recovery_location_collision_or_wrong_identity_is_not_connected() {
        let devices = vec![
            (9, 0x239a, 0x0029, "NocFree &".into()),
            (9, 1, 2, "Other".into()),
        ];
        assert!(!derive(&devices, [Some(9), None, None], true)[0].connected());
        assert!(
            !derive(
                &vec![(9, 1, 2, "Other".into())],
                [Some(9), None, None],
                true
            )[0]
            .connected()
        );
    }

    #[test]
    fn normal_and_recovery_for_same_role_are_ambiguous() {
        let mut devices = rmk();
        devices.push((9, 0x239a, 0x0029, "NocFree &".into()));
        assert_eq!(
            derive(&devices, [Some(9), None, None], true)[0],
            Presence::Unidentified
        );
    }

    fn factory(location: u64) -> UsbKey {
        vec![(location, 0x2886, 0x8029, "NocFree & ANSI".into())]
    }

    impl Identifier {
        fn observe_then_next(&mut self, ticket: u64, devices: &UsbKey, fresh: bool) {
            let now = Instant::now();
            self.observe_at(ticket, devices, fresh, now);
            self.next_at(ticket, now);
        }
    }

    #[test]
    fn observations_only_enable_next_and_confirmation_returns_to_picker() {
        let mut machine = Identifier::default();
        machine.start(Role::Left);
        let now = Instant::now();
        let ticket = machine.ticket();
        machine.observe_at(ticket, &vec![], true, now);
        assert_eq!(machine.state(), Identification::Disconnect(Role::Left));
        assert!(machine.0.inner().ready_for_next(now));
        assert!(!machine.next_at(ticket, now + std::time::Duration::from_secs(6)));
        machine.observe_at(ticket, &factory(9), true, now);
        assert!(!machine.next_at(ticket, now));
        machine.observe_at(ticket, &vec![], true, now);
        assert!(machine.next_at(ticket, now));
        assert_eq!(machine.state(), Identification::Connect(Role::Left));
        let connected = machine.ticket();
        machine.observe_at(connected, &factory(9), true, now);
        assert_eq!(machine.bindings(), [None; 3]);
        machine.observe_at(connected, &factory(9), false, now);
        assert!(!machine.next_at(connected, now));
        machine.observe_at(connected, &factory(9), true, now);
        assert!(!machine.next_at(ticket, now));
        assert!(machine.next_at(connected, now));
        assert_eq!(machine.state(), Identification::Idle);
    }

    fn identify(machine: &mut Identifier, role: Role, location: u64) {
        machine.start(role);
        machine.observe_then_next(machine.ticket(), &vec![], true);
        assert_eq!(machine.state(), Identification::Connect(role));
        machine.observe_then_next(machine.ticket(), &factory(location), true);
        assert_eq!(machine.state(), Identification::Idle);
    }

    #[test]
    fn factory_identification_requires_disconnect_then_isolated_reconnect() {
        let mut machine = Identifier::default();
        machine.start(Role::Left);
        machine.observe_then_next(machine.ticket(), &factory(9), true);
        assert_eq!(machine.state(), Identification::Disconnect(Role::Left));
        machine.observe_then_next(machine.ticket(), &vec![], true);
        assert_eq!(machine.state(), Identification::Connect(Role::Left));
        let mut two = factory(9);
        two.extend(factory(10));
        machine.observe_then_next(machine.ticket(), &two, true);
        assert_eq!(machine.state(), Identification::Connect(Role::Left));
        machine.observe_then_next(machine.ticket(), &factory(9), true);
        assert_eq!(machine.bindings(), [Some(9), None, None]);
        assert_eq!(
            super::derive(&factory(9), [None; 3], true, machine.bindings())[0],
            Presence::Connected
        );
    }

    #[test]
    fn stale_queries_and_failed_discovery_cannot_identify() {
        let mut machine = Identifier::default();
        let stale = machine.ticket();
        machine.start(Role::Left);
        machine.observe_then_next(stale, &vec![], true);
        assert_eq!(machine.state(), Identification::Disconnect(Role::Left));
        machine.observe_then_next(machine.ticket(), &vec![], false);
        assert_eq!(machine.state(), Identification::Disconnect(Role::Left));
        let before_connect = machine.ticket();
        machine.observe_then_next(before_connect, &vec![], true);
        machine.observe_then_next(before_connect, &factory(9), true);
        assert_eq!(machine.bindings(), [None; 3]);
        machine.cancel();
        machine.observe_then_next(before_connect, &factory(9), true);
        assert_eq!(machine.state(), Identification::Idle);
        assert_eq!(machine.bindings(), [None; 3]);
    }

    #[test]
    fn cancel_returns_to_picker_keeps_prior_bindings_and_rejects_pending_confirmation() {
        let mut machine = Identifier::default();
        identify(&mut machine, Role::Left, 9);
        machine.start(Role::Receiver);
        let left = factory(9);
        machine.observe_then_next(machine.ticket(), &left, true);
        let ticket = machine.ticket();
        let mut both = left;
        both.extend(factory(10));
        let now = Instant::now();
        machine.observe_at(ticket, &both, true, now);
        assert!(machine.0.inner().ready_for_next(now));
        machine.cancel();
        assert_eq!(machine.state(), Identification::Idle);
        assert_eq!(machine.bindings(), [Some(9), None, None]);
        assert!(!machine.0.inner().ready_for_next(now));
        assert!(!machine.next_at(ticket, now));
        machine.observe_at(ticket, &both, true, now);
        assert_eq!(machine.bindings(), [Some(9), None, None]);
        machine.observe_at(machine.ticket(), &vec![], true, now);
        assert_eq!(machine.bindings(), [None; 3]);
    }

    #[test]
    fn existing_identified_factory_part_may_remain_connected() {
        let mut machine = Identifier::default();
        identify(&mut machine, Role::Left, 9);
        machine.start(Role::Receiver);
        machine.observe_then_next(machine.ticket(), &factory(9), true);
        assert_eq!(machine.state(), Identification::Connect(Role::Receiver));
        let mut both = factory(9);
        both.extend(factory(10));
        machine.observe_then_next(machine.ticket(), &both, true);
        assert_eq!(machine.bindings(), [Some(9), None, Some(10)]);
        assert_eq!(
            super::derive(&both, [None; 3], true, machine.bindings()),
            [
                Presence::Connected,
                Presence::Disconnected,
                Presence::Connected
            ]
        );
    }

    #[test]
    fn bindings_expire_on_disconnect_or_reused_location() {
        let mut machine = Identifier::default();
        identify(&mut machine, Role::Left, 9);
        machine.observe_then_next(machine.ticket(), &vec![], false);
        assert_eq!(machine.bindings()[0], Some(9));
        machine.observe_then_next(machine.ticket(), &vec![], true);
        assert_eq!(machine.bindings()[0], None);
        machine.observe_then_next(machine.ticket(), &factory(9), true);
        assert_eq!(machine.bindings()[0], None);
        identify(&mut machine, Role::Left, 9);
        machine.observe_then_next(machine.ticket(), &vec![(9, 1, 2, "Other".into())], true);
        assert_eq!(machine.bindings()[0], None);
        identify(&mut machine, Role::Left, 9);
        machine.observe_then_next(
            machine.ticket(),
            &vec![(9, 0x239a, 0x0029, "NocFree &".into())],
            true,
        );
        assert_eq!(machine.bindings()[0], None);
    }

    #[test]
    fn factory_identification_never_assigns_right_or_colliding_location() {
        let mut machine = Identifier::default();
        machine.start(Role::Right);
        machine.observe_then_next(machine.ticket(), &vec![], true);
        machine.observe_then_next(machine.ticket(), &factory(9), true);
        assert_eq!(machine.bindings(), [None; 3]);
        machine.start(Role::Receiver);
        machine.observe_then_next(machine.ticket(), &vec![], true);
        let mut collision = factory(9);
        collision.push((9, 1, 2, "Other".into()));
        machine.observe_then_next(machine.ticket(), &collision, true);
        assert_eq!(machine.bindings(), [None; 3]);
    }

    #[test]
    fn anonymous_recovery_needs_physical_owner_identification_for_any_role() {
        for role in [Role::Left, Role::Right, Role::Receiver] {
            let mut machine = Identifier::default();
            let recovery = vec![(9, 0x239a, 0x0029, "NocFree &".into())];
            machine.start(role);
            machine.observe_then_next(machine.ticket(), &recovery, true);
            assert_eq!(machine.state(), Identification::Disconnect(role));
            machine.observe_then_next(machine.ticket(), &vec![], true);
            machine.observe_then_next(machine.ticket(), &recovery, true);
            assert_eq!(machine.state(), Identification::Idle);
            let present = super::derive(&recovery, [None; 3], true, machine.bindings());
            assert_eq!(present[index(role)], Presence::Connected);
            assert_eq!(present.iter().filter(|p| p.connected()).count(), 1);
            assert_eq!(
                super::derive(&recovery, machine.bindings(), true, machine.bindings()),
                present
            );
            machine.observe_then_next(machine.ticket(), &vec![], true);
            assert_eq!(machine.bindings(), [None; 3]);
        }
    }

    #[test]
    fn conflicting_cached_and_owner_identified_roles_cannot_enable_same_drive() {
        let recovery = vec![(9, 0x239a, 0x0029, "NocFree &".into())];
        let present = super::derive(
            &recovery,
            [Some(9), None, None],
            true,
            [None, Some(9), None],
        );
        assert!(!present.iter().any(|p| p.connected()));
    }
}

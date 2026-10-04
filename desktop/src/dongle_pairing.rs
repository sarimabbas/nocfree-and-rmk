//! Explicit, one-shot repair of the dedicated dongle pairing. Status polls never mutate bonds.
use crate::runtime_recovery::Role;
use hidapi::{BusType, HidApi, HidDevice};
use nusb::{DeviceId, DeviceInfo, MaybeFuture};
use statig::{
    Outcome,
    blocking::{IntoStateMachine, IntoStateMachineExt, State as StatigState, StateMachine},
};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use std::time::{Duration, Instant};

const PREFIX: [u8; 8] = [8, 0x7e, 4, 1, b'N', b'C', b'P', b'R'];
const REQUEST_AGE: Duration = Duration::from_secs(5);
const PAIRING_TIMEOUT: Duration = Duration::from_secs(45);
const USB_CHANGED: &str =
    "A USB connection changed. Pairing was stopped; it will not retry automatically.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    Idle,
    Searching,
    Pairing,
    Encrypted,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitLink {
    Unknown,
    Disconnected,
    Connected,
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct RadioAddress {
    kind: u8,
    bytes: [u8; 6],
}
impl std::fmt::Debug for RadioAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RadioAddress(<private>)")
    }
}
fn address(kind: u8, bytes: &[u8]) -> Result<Option<RadioAddress>, String> {
    let bytes: [u8; 6] = bytes.try_into().map_err(|_| "Invalid pairing identity.")?;
    match kind {
        255 if bytes == [0; 6] => Ok(None),
        0 | 1 if bytes != [0; 6] => Ok(Some(RadioAddress { kind, bytes })),
        _ => Err("Invalid pairing identity.".into()),
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub repair_supported: bool,
    pub link: Link,
    pub dongle_mode: bool,
    pub right_link: SplitLink,
    local: Option<RadioAddress>,
    peer: Option<RadioAddress>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Binding {
    left: DeviceId,
    dongle: DeviceId,
}
#[derive(Clone, Debug)]
pub struct Observation {
    pub left: Option<Snapshot>,
    pub dongle: Option<Snapshot>,
    binding: Option<Binding>,
    started: Instant,
    seen: Instant,
}
impl Observation {
    pub fn connected(&self) -> bool {
        match (self.left, self.dongle) {
            (Some(left), Some(dongle)) => {
                left.dongle_mode
                    && left.link == Link::Encrypted
                    && dongle.link == Link::Encrypted
                    && left.local.is_some()
                    && dongle.local.is_some()
                    && left.peer == dongle.local
                    && dongle.peer == left.local
            }
            _ => false,
        }
    }
    pub fn complete(&self) -> bool {
        self.left
            .is_some_and(|left| left.right_link == SplitLink::Connected)
            && (self.dongle.is_none() || self.connected())
    }
    fn ready(&self) -> bool {
        self.left.is_some_and(|s| {
            s.dongle_mode
                && s.repair_supported
                && s.local.is_some()
                && s.right_link == SplitLink::Connected
        }) && self
            .dongle
            .is_some_and(|s| s.repair_supported && s.local.is_some())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Connect,
    SwitchMode,
    TurnOnRight,
    Ready,
    Pairing,
    Connected,
    Failed(String),
    Cancelled,
}
#[derive(Default)]
struct PairingData {
    request: Option<Arc<AtomicU8>>,
    binding: Option<Binding>,
    accepted: Option<Instant>,
    deadline: Option<Instant>,
}

enum PairingEvent<'a> {
    Observe(&'a Result<Observation, String>, Instant),
    Begin {
        binding: &'a Binding,
        permit: &'a Arc<AtomicU8>,
        now: Instant,
    },
    Accepted(&'a Result<(), String>, Instant),
    Cancel,
}

impl IntoStateMachine for PairingData {
    type Event<'a> = PairingEvent<'a>;
    type Context<'a> = ();
    type State = State;
    type Superstate<'a> = ();
    fn initial() -> State {
        State::Connect
    }
}

impl StatigState<PairingData> for State {
    fn call_handler(
        &mut self,
        data: &mut PairingData,
        event: &PairingEvent<'_>,
        _: &mut (),
    ) -> Outcome<Self> {
        use Outcome::{Handled, Transition};
        if let PairingEvent::Cancel = event {
            return if *self == State::Connected {
                Handled
            } else {
                data.deadline = None;
                Transition(State::Cancelled)
            };
        }
        if matches!(self, State::Failed(_) | State::Cancelled | State::Connected) {
            return Handled;
        }
        match event {
            PairingEvent::Begin {
                binding,
                permit,
                now,
            } if *self == State::Ready => {
                data.binding = Some((*binding).clone());
                data.request = Some((*permit).clone());
                data.accepted = None;
                data.deadline = Some(*now + PAIRING_TIMEOUT);
                Transition(State::Pairing)
            }
            PairingEvent::Accepted(result, now) if *self == State::Pairing => {
                if data.deadline.is_some_and(|deadline| *now >= deadline) {
                    Transition(State::Failed("Pairing timed out. Check whether the dongle reconnects before trying again.".into()))
                } else {
                    match result {
                        Err(error) => Transition(State::Failed(error.clone())),
                        Ok(()) => {
                            data.accepted = Some(*now);
                            Handled
                        }
                    }
                }
            }
            PairingEvent::Observe(observation, now) => {
                if data.deadline.is_some_and(|deadline| *now >= deadline) {
                    return Transition(State::Failed(
                        "Pairing timed out. The keyboard's other Bluetooth pairings are unchanged."
                            .into(),
                    ));
                }
                let observed = match observation {
                    Ok(value) => value,
                    Err(error) if error == crate::battery::NATIVE_BUSY => return Handled,
                    // A mode switch can replace the USB instance while a getter
                    // runs. Before any write, discard its result and reconnect.
                    // Once pairing starts, retain the one-shot failure behavior.
                    Err(error) if error == USB_CHANGED && *self != State::Pairing => {
                        return Transition(State::Connect);
                    }
                    Err(error) => return Transition(State::Failed(error.clone())),
                };
                if now.saturating_duration_since(observed.seen) >= REQUEST_AGE {
                    return Transition(State::Failed(
                        "The keyboard check expired. Check both USB connections.".into(),
                    ));
                }
                if *self == State::Pairing && data.binding != observed.binding {
                    return Transition(State::Failed(
                        "A USB connection changed during pairing.".into(),
                    ));
                }
                if *self == State::Pairing
                    && data
                        .accepted
                        .is_none_or(|accepted| observed.started < accepted)
                {
                    return Handled;
                }
                if observed.left.is_some_and(|s| s.link == Link::Failed)
                    || observed.dongle.is_some_and(|s| s.link == Link::Failed)
                {
                    return Transition(State::Failed("Dongle pairing could not finish. Keep both USB cables connected and the left switch in Dongle mode.".into()));
                }
                if observed.complete() {
                    data.deadline = None;
                    return Transition(State::Connected);
                }
                if *self == State::Pairing {
                    return if observed.left.is_none() || observed.dongle.is_none() {
                        Transition(State::Failed(
                            "A USB connection changed during pairing.".into(),
                        ))
                    } else if !observed.left.is_some_and(|s| s.dongle_mode) {
                        Transition(State::Failed(
                            "Keep the left switch in Dongle mode while pairing.".into(),
                        ))
                    } else {
                        Handled
                    };
                }
                let next = if observed.left.is_none() {
                    State::Connect
                } else if !observed
                    .left
                    .is_some_and(|s| s.right_link == SplitLink::Connected)
                {
                    State::TurnOnRight
                } else if observed.dongle.is_none() {
                    State::Connected
                } else if !observed.left.is_some_and(|s| s.dongle_mode) {
                    State::SwitchMode
                } else if !observed.ready() {
                    State::Failed("Install matching RMK firmware on the left half and dongle to repair their pairing.".into())
                } else {
                    State::Ready
                };
                if *self == next {
                    Handled
                } else {
                    Transition(next)
                }
            }
            _ => Handled,
        }
    }
}

pub struct Journey {
    machine: StateMachine<PairingData>,
}
impl Default for Journey {
    fn default() -> Self {
        Self {
            machine: PairingData::default().state_machine(),
        }
    }
}
impl Journey {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn state(&self) -> &State {
        self.machine.state()
    }
    pub fn observe(&mut self, observation: Result<Observation, String>, now: Instant) {
        self.machine
            .handle(&PairingEvent::Observe(&observation, now));
    }
    /// Only the explicit user action creates a native one-shot permit.
    pub fn begin(&mut self, observed: &Observation, now: Instant) -> Result<BeginRequest, String> {
        if self.state() != &State::Ready
            || !observed.ready()
            || now.saturating_duration_since(observed.seen) >= REQUEST_AGE
        {
            return Err("Check the left half and dongle before starting pairing.".into());
        }
        let binding = observed
            .binding
            .clone()
            .ok_or("Check both USB connections before pairing.")?;
        let permit = Arc::new(AtomicU8::new(0));
        self.machine.handle(&PairingEvent::Begin {
            binding: &binding,
            permit: &permit,
            now,
        });
        Ok(BeginRequest {
            binding,
            armed: now,
            submitted: permit,
        })
    }
    pub fn accepted(&mut self, result: Result<(), String>, now: Instant) {
        self.machine.handle(&PairingEvent::Accepted(&result, now));
    }
    pub fn cancel(&mut self) {
        if let Some(request) = &self.machine.inner().request {
            let _ = request.compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire);
        }
        self.machine.handle(&PairingEvent::Cancel);
    }
}

pub struct BeginRequest {
    binding: Binding,
    armed: Instant,
    submitted: Arc<AtomicU8>,
}
fn allowed(started: Instant) -> Result<(), String> {
    if started.elapsed() >= REQUEST_AGE {
        Err("The pairing request expired before submission.".into())
    } else {
        Ok(())
    }
}
fn request(role: Role, peer: Option<RadioAddress>) -> [u8; 33] {
    let mut request = [0; 33];
    request[1..9].copy_from_slice(&PREFIX);
    request[9] = u8::from(peer.is_some());
    request[10] = if role == Role::Left { 1 } else { 2 };
    if let Some(peer) = peer {
        request[11] = peer.kind;
        request[12..18].copy_from_slice(&peer.bytes);
    }
    request
}
fn decode(bytes: &[u8], role: Role) -> Result<Snapshot, String> {
    if bytes.len() != 32 || bytes[..8] != PREFIX || bytes[29..].iter().any(|b| *b != 0) {
        return Err("This firmware does not support dongle pairing in Companion.".into());
    }
    let expected = if role == Role::Left { 1 } else { 2 };
    if bytes[8] != 0 {
        return Err(match bytes[8] {
            4 => "Move the left switch to Dongle mode before pairing.",
            5 => "Pairing is already in progress. Wait for it to finish.",
            _ => "This firmware cannot start dongle pairing.",
        }
        .into());
    }
    if bytes[9] != expected || bytes[10] & !1 != 0 || bytes[12] > 3 || bytes[13] != 1 {
        return Err(
            "Couldn’t check pairing. Reconnect the left half and dongle, then try again.".into(),
        );
    }
    let link = match bytes[11] {
        0 => Link::Idle,
        1 => Link::Searching,
        2 => Link::Pairing,
        3 => Link::Encrypted,
        4 => Link::Failed,
        _ => return Err("The dongle pairing state is invalid.".into()),
    };
    let right_link = match bytes[14] {
        0 => SplitLink::Unknown,
        1 if role == Role::Left => SplitLink::Disconnected,
        2 if role == Role::Left => SplitLink::Connected,
        _ => return Err("Invalid right-half connection status.".into()),
    };
    Ok(Snapshot {
        repair_supported: bytes[10] & 1 != 0,
        link,
        dongle_mode: role == Role::Left && bytes[12] == 3,
        right_link,
        local: address(bytes[15], &bytes[16..22])?,
        peer: address(bytes[22], &bytes[23..29])?,
    })
}
fn identity(device: &DeviceInfo, role: Role) -> bool {
    device.vendor_id() == 0x4c4b
        && device.product_id() == role.product()
        && device.product_string() == Some(role.name())
}
fn target(role: Role) -> Result<Option<DeviceInfo>, String> {
    let mut devices = nusb::list_devices()
        .wait()
        .map_err(|_| "Couldn't check the keyboard USB connections.")?
        .filter(|d| identity(d, role));
    let found = devices.next();
    if devices.next().is_some() {
        return Err("Connect only one left half and one dongle.".into());
    }
    Ok(found)
}
fn unchanged(expected: &DeviceInfo, role: Role) -> Result<(), String> {
    if target(role)?.is_some_and(|now| now.id() == expected.id()) {
        Ok(())
    } else {
        Err(USB_CHANGED.into())
    }
}
fn open(api: &HidApi, target: &DeviceInfo, role: Role) -> Result<HidDevice, String> {
    let matches = |info: &hidapi::DeviceInfo| {
        info.vendor_id() == 0x4c4b
            && info.product_id() == role.product()
            && info.product_string() == Some(role.name())
            && matches!(info.bus_type(), BusType::Usb)
            && (info.usage_page(), info.usage()) == (0xff60, 0x61)
    };
    let mut interfaces = api.device_list().filter(|info| matches(info));
    let info = interfaces
        .next()
        .ok_or("Dongle pairing is unavailable with this firmware.")?;
    if interfaces.next().is_some() {
        return Err("The pairing interface is ambiguous.".into());
    }
    let number =
        u8::try_from(info.interface_number()).map_err(|_| "The pairing interface is invalid.")?;
    if !target.interfaces().any(|i| {
        i.interface_number() == number && (i.class(), i.subclass(), i.protocol()) == (3, 0, 0)
    }) {
        return Err("The pairing interface is invalid.".into());
    }
    let path = info.path().to_owned();
    let device = api
        .open_path(&path)
        .map_err(|_| "Couldn't open the dongle pairing connection.")?;
    let opened = device
        .get_device_info()
        .map_err(|_| "Couldn't confirm the pairing connection.")?;
    if !matches(&opened)
        || opened.path() != path.as_c_str()
        || opened.interface_number() != i32::from(number)
    {
        return Err("The pairing connection changed.".into());
    }
    let mut desc = [0; 256];
    let len = device
        .get_report_descriptor(&mut desc)
        .map_err(|_| "Couldn't check the pairing report layout.")?;
    if !crate::battery::vial::report_layout_valid(&desc[..len]) {
        return Err("The pairing report layout is unsupported.".into());
    }
    Ok(device)
}
fn exchange(
    device: &HidDevice,
    target: &DeviceInfo,
    role: Role,
    peer: Option<RadioAddress>,
    started: Instant,
) -> Result<Snapshot, String> {
    unchanged(target, role)?;
    allowed(started)?;
    if device.write(&request(role, peer)).map_err(
        |_| "The pairing request could not be submitted. Check its state before trying again.",
    )? != 33
    {
        return Err("The pairing request was incomplete. It will not retry automatically.".into());
    }
    let mut reply = [0; 32];
    let len = device
        .read_timeout(&mut reply, 500)
        .map_err(|_| "The pairing response is unavailable. Check its state before trying again.")?;
    unchanged(target, role)?;
    allowed(started)?;
    response(&reply[..len], role, peer)
}
fn response(bytes: &[u8], role: Role, peer: Option<RadioAddress>) -> Result<Snapshot, String> {
    if bytes == &request(role, peer)[1..] {
        return Err("Update the left half and dongle to check pairing here.".into());
    }
    decode(bytes, role)
}

pub fn query() -> Result<Observation, String> {
    crate::battery::native_task(|started| {
        allowed(started)?;
        let left = target(Role::Left)?;
        let dongle = target(Role::Receiver)?;
        let api = HidApi::new().map_err(|_| "Couldn't inspect the pairing interfaces.")?;
        let read = |target: &Option<DeviceInfo>, role| -> Result<Option<Snapshot>, String> {
            target
                .as_ref()
                .map(|t| open(&api, t, role).and_then(|d| exchange(&d, t, role, None, started)))
                .transpose()
        };
        let left_status = read(&left, Role::Left)?;
        let dongle_status = read(&dongle, Role::Receiver)?;
        if let Some(left) = &left {
            unchanged(left, Role::Left)?;
        }
        if let Some(dongle) = &dongle {
            unchanged(dongle, Role::Receiver)?;
        }
        let binding = left.zip(dongle).map(|(left, dongle)| Binding {
            left: left.id(),
            dongle: dongle.id(),
        });
        Ok(Observation {
            left: left_status,
            dongle: dongle_status,
            binding,
            started,
            seen: Instant::now(),
        })
    })
}
/// Explicit, one-shot mutation. Both capabilities are re-read before either dedicated bond changes.
pub fn begin(request: BeginRequest) -> Result<(), String> {
    crate::battery::native_task(move |started| {
        allowed(request.armed)?;
        allowed(started)?;
        let left = target(Role::Left)?.ok_or("Connect the left half by USB.")?;
        let dongle = target(Role::Receiver)?.ok_or("Connect the dongle by USB.")?;
        if left.id() != request.binding.left || dongle.id() != request.binding.dongle {
            return Err(
                "The pairing connections changed. Check them before starting again.".into(),
            );
        }
        let api = HidApi::new().map_err(|_| "Couldn't inspect the pairing interfaces.")?;
        let left_hid = open(&api, &left, Role::Left)?;
        let dongle_hid = open(&api, &dongle, Role::Receiver)?;
        let left_status = exchange(&left_hid, &left, Role::Left, None, started)?;
        let dongle_status = exchange(&dongle_hid, &dongle, Role::Receiver, None, started)?;
        if !left_status.repair_supported
            || !dongle_status.repair_supported
            || !left_status.dongle_mode
        {
            return Err(
                "Both devices must support pairing and the left switch must be in Dongle mode."
                    .into(),
            );
        }
        let left_address = left_status
            .local
            .ok_or("The left pairing identity is unavailable.")?;
        let dongle_address = dongle_status
            .local
            .ok_or("The dongle pairing identity is unavailable.")?;
        if left_status.link == Link::Encrypted
            && dongle_status.link == Link::Encrypted
            && left_status.peer == Some(dongle_address)
            && dongle_status.peer == Some(left_address)
        {
            return Ok(());
        }
        unchanged(&left, Role::Left)?;
        unchanged(&dongle, Role::Receiver)?;
        allowed(request.armed)?;
        request
            .submitted
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "This pairing request was cancelled or already submitted.".to_owned())?;
        exchange(
            &dongle_hid,
            &dongle,
            Role::Receiver,
            Some(left_address),
            started,
        )?;
        exchange(&left_hid, &left, Role::Left, Some(dongle_address), started).map_err(|error| {
            format!("The dongle accepted pairing, but the left half did not confirm it. {error}")
        })?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(link: Link, mode: bool) -> Snapshot {
        Snapshot {
            repair_supported: true,
            link,
            dongle_mode: mode,
            right_link: SplitLink::Connected,
            local: Some(RadioAddress {
                kind: 1,
                bytes: if mode { [1; 6] } else { [2; 6] },
            }),
            peer: Some(RadioAddress {
                kind: 1,
                bytes: if mode { [2; 6] } else { [1; 6] },
            }),
        }
    }
    fn observation(left: Option<Snapshot>, dongle: Option<Snapshot>, now: Instant) -> Observation {
        Observation {
            left,
            dongle,
            binding: None,
            started: now,
            seen: now,
        }
    }
    fn reply(role: Role, state: u8) -> [u8; 32] {
        let mut bytes = [0; 32];
        bytes[..8].copy_from_slice(&PREFIX);
        bytes[9] = if role == Role::Left { 1 } else { 2 };
        bytes[10] = 1;
        bytes[11] = state;
        bytes[12] = if role == Role::Left { 3 } else { 0 };
        bytes[13] = 1;
        bytes[14] = if role == Role::Left { 2 } else { 0 };
        bytes[15] = 1;
        bytes[16..22].copy_from_slice(&[1; 6]);
        bytes[22] = 255;
        bytes
    }
    #[test]
    fn mode_switch_usb_reenumeration_before_pairing_returns_to_connection_check() {
        let now = Instant::now();
        let mut journey = Journey::new();
        journey.observe(
            Ok(observation(
                Some(snapshot(Link::Idle, false)),
                Some(snapshot(Link::Searching, false)),
                now,
            )),
            now,
        );
        assert_eq!(journey.state(), &State::SwitchMode);
        journey.observe(
            Err(
                "A USB connection changed. Pairing was stopped; it will not retry automatically."
                    .into(),
            ),
            now,
        );
        assert_eq!(journey.state(), &State::Connect);
        assert!(journey.machine.inner().request.is_none());
        journey.observe(
            Ok(observation(
                Some(snapshot(Link::Encrypted, true)),
                Some(snapshot(Link::Encrypted, false)),
                now,
            )),
            now,
        );
        assert_eq!(journey.state(), &State::Connected);
    }
    #[test]
    fn usb_reenumeration_after_pairing_started_is_terminal() {
        let now = Instant::now();
        let mut journey = Journey::new();
        unsafe {
            *journey.machine.state_mut() = State::Pairing;
            journey.machine.inner_mut().deadline = Some(now + PAIRING_TIMEOUT);
        }
        journey.observe(Err(USB_CHANGED.into()), now);
        assert_eq!(journey.state(), &State::Failed(USB_CHANGED.into()));
        journey.observe(
            Ok(observation(
                Some(snapshot(Link::Encrypted, true)),
                Some(snapshot(Link::Encrypted, false)),
                now,
            )),
            now,
        );
        journey.accepted(Ok(()), now);
        assert_eq!(journey.state(), &State::Failed(USB_CHANGED.into()));
    }
    #[test]
    fn unsupported_firmware_echo_has_friendly_update_instruction() {
        for peer in [
            None,
            Some(RadioAddress {
                kind: 1,
                bytes: [1; 6],
            }),
        ] {
            let packet = request(Role::Left, peer);
            assert_eq!(
                response(&packet[1..], Role::Left, peer).unwrap_err(),
                "Update the left half and dongle to check pairing here."
            );
        }
        assert!(response(&reply(Role::Left, 3), Role::Left, None).is_ok());
    }
    #[test]
    fn status_packet_never_arms_repair_and_protocol_is_role_bound() {
        assert_eq!(request(Role::Left, None)[9], 0);
        assert_eq!(
            request(
                Role::Receiver,
                Some(RadioAddress {
                    kind: 1,
                    bytes: [1; 6]
                })
            )[9],
            1
        );
        assert_eq!(
            request(
                Role::Receiver,
                Some(RadioAddress {
                    kind: 1,
                    bytes: [1; 6]
                })
            )[10],
            2
        );
        assert_eq!(
            decode(&reply(Role::Left, 3), Role::Left).unwrap().link,
            Link::Encrypted
        );
        assert!(decode(&reply(Role::Left, 3), Role::Receiver).is_err());
        for (index, value) in [
            (3, 2),
            (8, 3),
            (10, 2),
            (11, 5),
            (12, 4),
            (13, 0),
            (14, 3),
            (15, 2),
            (22, 2),
            (29, 1),
        ] {
            let mut bytes = reply(Role::Left, 3);
            bytes[index] = value;
            assert!(decode(&bytes, Role::Left).is_err());
        }
        assert!(decode(&reply(Role::Left, 3)[..31], Role::Left).is_err());
    }
    #[test]
    fn reconnect_is_detected_without_pairing_and_searching_never_completes() {
        let now = Instant::now();
        let mut journey = Journey::new();
        journey.observe(Ok(observation(None, None, now)), now);
        assert_eq!(journey.state(), &State::Connect);
        journey.observe(
            Ok(observation(
                Some(snapshot(Link::Idle, false)),
                Some(snapshot(Link::Searching, false)),
                now,
            )),
            now,
        );
        assert_eq!(journey.state(), &State::SwitchMode);
        journey.observe(
            Ok(observation(
                Some(snapshot(Link::Searching, true)),
                Some(snapshot(Link::Searching, false)),
                now,
            )),
            now,
        );
        assert_eq!(journey.state(), &State::Ready);
        // Test fixtures lack USB bindings; even Ready cannot manufacture a mutation request.
        assert!(
            journey
                .begin(
                    &observation(
                        Some(snapshot(Link::Idle, true)),
                        Some(snapshot(Link::Idle, false)),
                        now
                    ),
                    now
                )
                .is_err()
        );
        journey.observe(
            Ok(observation(
                Some(snapshot(Link::Encrypted, true)),
                Some(snapshot(Link::Encrypted, false)),
                now,
            )),
            now,
        );
        assert_eq!(journey.state(), &State::Connected);
    }
    #[test]
    fn peer_reciprocity_and_optional_dongle_are_separate_from_right_link() {
        let now = Instant::now();
        let left = snapshot(Link::Encrypted, true);
        let mut dongle = snapshot(Link::Encrypted, false);
        dongle.peer = Some(RadioAddress {
            kind: 1,
            bytes: [3; 6],
        });
        let wrong = observation(Some(left), Some(dongle), now);
        assert!(!wrong.connected());
        assert!(!wrong.complete());
        let halves = observation(Some(snapshot(Link::Idle, false)), None, now);
        assert!(!halves.connected());
        assert!(halves.complete());
        let mut journey = Journey::new();
        journey.observe(Ok(halves), now);
        assert_eq!(journey.state(), &State::Connected);
        let mut right_off = left;
        right_off.right_link = SplitLink::Disconnected;
        let mut journey = Journey::new();
        journey.observe(Ok(observation(Some(right_off), None, now)), now);
        assert_eq!(journey.state(), &State::TurnOnRight);
    }
    #[test]
    fn fresh_in_age_pre_submission_reply_and_busy_cannot_complete() {
        let now = Instant::now();
        let mut journey = Journey::new();
        unsafe {
            *journey.machine.state_mut() = State::Pairing;
            journey.machine.inner_mut().deadline = Some(now + PAIRING_TIMEOUT);
        }
        unsafe {
            journey.machine.inner_mut().accepted = Some(now + Duration::from_secs(1));
        }
        let old = observation(
            Some(snapshot(Link::Encrypted, true)),
            Some(snapshot(Link::Encrypted, false)),
            now,
        );
        journey.observe(Ok(old), now + Duration::from_secs(2));
        assert_eq!(journey.state(), &State::Pairing);
        journey.observe(
            Err(crate::battery::NATIVE_BUSY.into()),
            now + Duration::from_secs(2),
        );
        assert_eq!(journey.state(), &State::Pairing);
        journey.observe(
            Ok(observation(
                Some(snapshot(Link::Encrypted, true)),
                Some(snapshot(Link::Encrypted, false)),
                now + Duration::from_secs(2),
            )),
            now + Duration::from_secs(2),
        );
        assert_eq!(journey.state(), &State::Connected);
    }
    #[test]
    fn capability_staleness_failure_and_cancel_are_terminal() {
        let now = Instant::now();
        let mut journey = Journey::new();
        let mut left = snapshot(Link::Idle, true);
        left.repair_supported = false;
        journey.observe(
            Ok(observation(
                Some(left),
                Some(snapshot(Link::Idle, false)),
                now,
            )),
            now,
        );
        assert!(matches!(journey.state(), State::Failed(_)));
        journey.observe(Ok(observation(None, None, now)), now);
        assert!(matches!(journey.state(), State::Failed(_)));
        let mut journey = Journey::new();
        journey.observe(Ok(observation(None, None, now)), now + REQUEST_AGE);
        assert!(matches!(journey.state(), State::Failed(_)));
        let mut journey = Journey::new();
        let token = Arc::new(AtomicU8::new(0));
        unsafe {
            journey.machine.inner_mut().request = Some(token.clone());
        }
        journey.cancel();
        assert_eq!(token.load(Ordering::Acquire), 2);
        journey.observe(
            Ok(observation(
                Some(snapshot(Link::Encrypted, true)),
                Some(snapshot(Link::Encrypted, false)),
                now,
            )),
            now,
        );
        assert_eq!(journey.state(), &State::Cancelled);
    }
    #[test]
    fn acceptance_events_cannot_start_or_revive_a_pairing_attempt() {
        let now = Instant::now();
        let mut journey = Journey::new();
        journey.accepted(Ok(()), now);
        assert_eq!(journey.state(), &State::Connect);
        assert!(journey.machine.inner().accepted.is_none());
        journey.cancel();
        journey.accepted(Ok(()), now);
        journey.accepted(Err("late command result".into()), now);
        assert_eq!(journey.state(), &State::Cancelled);
        assert!(journey.machine.inner().accepted.is_none());
    }
    #[test]
    fn accepted_command_is_not_proof_and_timeout_never_retries() {
        let now = Instant::now();
        let mut journey = Journey::new();
        unsafe {
            *journey.machine.state_mut() = State::Pairing;
            journey.machine.inner_mut().deadline = Some(now + PAIRING_TIMEOUT);
        }
        journey.accepted(Ok(()), now);
        assert_eq!(journey.state(), &State::Pairing);
        journey.observe(
            Ok(observation(
                Some(snapshot(Link::Pairing, true)),
                Some(snapshot(Link::Searching, false)),
                now,
            )),
            now,
        );
        assert_eq!(journey.state(), &State::Pairing);
        journey.observe(
            Ok(observation(
                Some(snapshot(Link::Encrypted, true)),
                Some(snapshot(Link::Encrypted, false)),
                now + PAIRING_TIMEOUT,
            )),
            now + PAIRING_TIMEOUT,
        );
        assert!(matches!(journey.state(), State::Failed(_)));
    }
}

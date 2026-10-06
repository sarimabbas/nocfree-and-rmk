//! Board-only status and local USB recovery. Vial/key/radio/storage stay in RMK.
use core::cell::Cell;
use embassy_sync::blocking_mutex::Mutex;
use embassy_time::{Duration, Instant, Timer, with_timeout};
use embassy_usb::{
    Builder,
    class::hid::{Config, HidBootProtocol, HidReaderWriter, HidSubclass, State},
    driver::Driver,
};
#[cfg(not(feature = "receiver"))]
use rmk::event::BatteryStatusEvent;
#[cfg(any(not(feature = "receiver"), test))]
use rmk::types::battery::BatteryStatus;
use rmk::{
    core_traits::Runnable,
    custom_message::{CustomMessage, CustomMessageTarget},
    event::{EventSubscriber, SubscribableEvent},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Left,
    Right,
    Receiver,
}
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg(any(not(feature = "receiver"), test))]
struct Snapshot {
    local: BatteryStatus,
    right: BatteryStatus,
    connected: bool,
    facts: [u8; 3],
    #[cfg(not(feature = "receiver"))]
    diagnostic: Option<(i16, u64, u8, [u32; 4])>,
    #[cfg(not(feature = "receiver"))]
    adc_at: Option<Instant>,
    peer_at: Option<Instant>,
    receiver_link: bool,
    #[cfg(not(feature = "receiver"))]
    central_link: bool,
    #[cfg(not(feature = "receiver"))]
    sleeping: bool,
}
#[cfg(any(not(feature = "receiver"), test))]
impl Snapshot {
    const EMPTY: Self = Self {
        local: BatteryStatus::Unavailable,
        right: BatteryStatus::Unavailable,
        connected: false,
        facts: [0; 3],
        #[cfg(not(feature = "receiver"))]
        diagnostic: None,
        #[cfg(not(feature = "receiver"))]
        adc_at: None,
        peer_at: None,
        receiver_link: false,
        #[cfg(not(feature = "receiver"))]
        central_link: false,
        #[cfg(not(feature = "receiver"))]
        sleeping: false,
    };
    fn wire(self) -> [u8; 32] {
        let mut req = [0; 32];
        req[..8].copy_from_slice(&[8, 0x7e, 1, 2, b'N', b'C', b'B', b'T']);
        status_reply(
            &req,
            true,
            self.local,
            Some((self.connected, self.right)),
            self.facts,
        )
        .unwrap()
    }
}
#[cfg(not(feature = "receiver"))]
static CACHE: Mutex<rmk::RawMutex, Cell<Snapshot>> = Mutex::new(Cell::new(Snapshot::EMPTY));
#[cfg(not(feature = "receiver"))]
static CHANGED: embassy_sync::signal::Signal<rmk::RawMutex, ()> =
    embassy_sync::signal::Signal::new();
#[cfg(not(feature = "receiver"))]
fn update(f: impl FnOnce(&mut Snapshot)) {
    let changed = CACHE.lock(|c| {
        let before = c.get();
        let mut s = before;
        f(&mut s);
        c.set(s);
        before != s
    });
    #[cfg(not(feature = "receiver"))]
    if changed {
        CHANGED.signal(());
    }
    #[cfg(feature = "receiver")]
    let _ = changed;
}
#[cfg(not(feature = "receiver"))]
fn snapshot() -> Snapshot {
    CACHE.lock(Cell::get)
}
#[cfg(not(feature = "receiver"))]
pub fn set_mode(mode: u8) {
    update(|s| {
        s.facts[1] = mode.min(3);
        if mode != 0 {
            s.facts[2] |= 8;
        } else {
            s.facts[2] &= !8;
        }
    });
}
#[cfg(not(feature = "receiver"))]
pub fn record_adc(signed: i16, registers: [u32; 4], levels: u8) {
    update(|s| {
        s.diagnostic = Some((signed, 0, levels, registers));
        s.adc_at = Some(Instant::now());
    });
}
#[cfg(not(feature = "receiver"))]
struct Power;
#[cfg(not(feature = "receiver"))]
impl embassy_usb::Handler for Power {
    fn enabled(&mut self, enabled: bool) {
        update(|s| {
            s.facts[0] = (s.facts[0] & !1) | u8::from(enabled);
            s.facts[2] |= 1;
        });
    }
}
const REPORT: &[u8] = &[
    0x06, 0x61, 0xff, 0x09, 0x61, 0xa1, 1, 0x15, 0, 0x26, 0xff, 0, 0x75, 8, 0x95, 32, 0x09, 0x62,
    0x81, 2, 0x95, 32, 0x09, 0x63, 0x91, 2, 0xc0,
];
fn recovery(r: &[u8; 32]) -> bool {
    r[..8] == [8, 0x7e, 5, 1, b'N', b'C', b'R', b'C'] && r[8..].iter().all(|b| *b == 0)
}
pub fn attach<D: Driver<'static>>(builder: &mut Builder<'static, D>, role: Role) -> Service<D> {
    #[cfg(feature = "receiver")]
    let _ = role;
    static HID: static_cell::StaticCell<State<'static>> = static_cell::StaticCell::new();
    #[cfg(not(feature = "receiver"))]
    static POWER: static_cell::StaticCell<Power> = static_cell::StaticCell::new();
    #[cfg(not(feature = "receiver"))]
    builder.handler(POWER.init(Power));
    let hid = HidReaderWriter::new(
        builder,
        HID.init(State::new()),
        Config {
            report_descriptor: REPORT,
            request_handler: None,
            poll_ms: 20,
            max_packet_size: 32,
            hid_boot_protocol: HidBootProtocol::None,
            hid_subclass: HidSubclass::No,
        },
    );
    Service {
        hid,
        #[cfg(not(feature = "receiver"))]
        role,
        #[cfg(not(feature = "receiver"))]
        battery: BatteryStatusEvent::subscriber(),
        custom: CustomMessage::subscriber(),
        #[cfg(not(feature = "receiver"))]
        sleep: rmk::event::SleepStateEvent::subscriber(),
        #[cfg(not(feature = "receiver"))]
        connection: rmk::event::ConnectionStatusChangeEvent::subscriber(),
        #[cfg(feature = "right")]
        central: rmk::event::CentralConnectedEvent::subscriber(),
        #[cfg(feature = "left")]
        connected: rmk::event::PeripheralConnectedEvent::subscriber(),
        #[cfg(feature = "left")]
        peripheral: rmk::event::PeripheralBatteryEvent::subscriber(),
        #[cfg(feature = "receiver")]
        dongle: rmk::event::DongleStateEvent::subscriber(),
    }
}
pub struct Service<D: Driver<'static>> {
    hid: HidReaderWriter<'static, D, 32, 32>,
    #[cfg(not(feature = "receiver"))]
    role: Role,
    #[cfg(not(feature = "receiver"))]
    battery: <BatteryStatusEvent as SubscribableEvent>::Subscriber,
    custom: <CustomMessage as SubscribableEvent>::Subscriber,
    #[cfg(not(feature = "receiver"))]
    sleep: <rmk::event::SleepStateEvent as SubscribableEvent>::Subscriber,
    #[cfg(not(feature = "receiver"))]
    connection: <rmk::event::ConnectionStatusChangeEvent as SubscribableEvent>::Subscriber,
    #[cfg(feature = "right")]
    central: <rmk::event::CentralConnectedEvent as SubscribableEvent>::Subscriber,
    #[cfg(feature = "left")]
    connected: <rmk::event::PeripheralConnectedEvent as SubscribableEvent>::Subscriber,
    #[cfg(feature = "left")]
    peripheral: <rmk::event::PeripheralBatteryEvent as SubscribableEvent>::Subscriber,
    #[cfg(feature = "receiver")]
    dongle: <rmk::event::DongleStateEvent as SubscribableEvent>::Subscriber,
}
impl<D: Driver<'static>> Runnable for Service<D> {
    async fn run(&mut self) -> ! {
        #[cfg(feature = "receiver")]
        {
            self.run_receiver().await
        }
        #[cfg(not(feature = "receiver"))]
        {
            let hid = &mut self.hid;
            let role = self.role;
            let battery = &mut self.battery;
            let custom = &mut self.custom;
            let usb = async {
                loop {
                    let Some(req) = next_request(hid).await else {
                        continue;
                    };
                    if recovery(&req) {
                        let _ = with_timeout(Duration::from_millis(250), hid.write(&req)).await;
                        rmk::boot::jump_to_bootloader();
                    } else if pairing_request(&req) {
                        let mut response = req;
                        response[8] = 3;
                        #[cfg(feature = "left")]
                        if pairing_allowed(
                            role,
                            snapshot().facts[1],
                            rmk::state::current_connection_status().ble.profile,
                        ) {
                            response[8] = if with_timeout(
                                Duration::from_millis(250),
                                rmk::output_selection::clear_dongle_bond(),
                            )
                            .await
                            .is_ok()
                            {
                                0
                            } else {
                                4
                            };
                        }
                        let _ =
                            with_timeout(Duration::from_millis(250), hid.write(&response)).await;
                    } else {
                        if role == Role::Left {
                            refresh_route();
                        }
                        let mut s = snapshot();
                        if let (Some(d), Some(at)) = (&mut s.diagnostic, s.adc_at) {
                            d.1 = at.elapsed().as_millis();
                        }
                        let response = status_reply(
                            &req,
                            true,
                            s.local,
                            Some((s.connected, s.right)),
                            s.facts,
                        )
                        .or_else(|| reply(&req, true, s.local, Some((s.connected, s.right))));
                        let response = response
                            .or_else(|| diagnostic_reply(&req, s.diagnostic))
                            .or_else(|| mode_reply(&req, s.facts[1]));
                        if let Some(r) = response {
                            let _ = with_timeout(Duration::from_millis(250), hid.write(&r)).await;
                        }
                    }
                }
            };
            let local = async {
                loop {
                    let e = battery.next_event().await;
                    update(|s| s.local = e.0);
                }
            };
            let incoming = async {
                loop {
                    let e = custom.next_event().await;
                    accept_message(role, &e);
                }
            };
            let outgoing = async {
                loop {
                    if !can_forward(role, snapshot()) {
                        CHANGED.wait().await;
                    } else {
                        let _ = rmk::embassy_futures::select::select(
                            CHANGED.wait(),
                            Timer::after_secs(5),
                        )
                        .await;
                    }
                    let current = snapshot();
                    if !can_forward(role, current) {
                        continue;
                    }
                    let message = match role {
                        Role::Left => {
                            refresh_route();
                            CustomMessage::new(
                                &pack(snapshot().wire(), 1),
                                CustomMessageTarget::Dongle,
                            )
                        }
                        Role::Right => CustomMessage::new(
                            &pack(current.wire(), 2),
                            CustomMessageTarget::Central,
                        ),
                        Role::Receiver => continue,
                    };
                    if let Ok(message) = message {
                        rmk::custom_message::send(message);
                    }
                }
            };
            #[cfg(feature = "left")]
            let split = async {
                let connected = &mut self.connected;
                let peripheral = &mut self.peripheral;
                loop {
                    match rmk::embassy_futures::select::select(
                        connected.next_event(),
                        peripheral.next_event(),
                    )
                    .await
                    {
                        rmk::embassy_futures::select::Either::First(e) => {
                            if e.id == 0 {
                                update(|s| {
                                    s.connected = e.connected;
                                    if !e.connected {
                                        s.right = BatteryStatus::Unavailable;
                                        s.facts[0] &= !2;
                                        s.facts[2] &= !2;
                                        s.peer_at = None;
                                    }
                                })
                            }
                        }
                        rmk::embassy_futures::select::Either::Second(e) => {
                            if e.id == 0 {
                                update(|s| s.right = e.state.0)
                            }
                        }
                    }
                }
            };
            let sleep = async {
                let sub = &mut self.sleep;
                loop {
                    let e = sub.next_event().await;
                    update(|s| s.sleeping = e.0);
                }
            };
            #[cfg(feature = "right")]
            let central = async {
                let sub = &mut self.central;
                loop {
                    let e = sub.next_event().await;
                    update(|s| s.central_link = e.connected);
                }
            };
            let connection = async {
                let sub = &mut self.connection;
                loop {
                    let _ = sub.next_event().await;
                    if role == Role::Left {
                        refresh_route();
                    }
                    CHANGED.signal(());
                }
            };
            let basic = async {
                rmk::futures::join!(usb, local, incoming, outgoing, sleep, connection);
            };
            #[cfg(feature = "left")]
            {
                rmk::futures::join!(basic, split);
            }
            #[cfg(feature = "right")]
            {
                rmk::futures::join!(basic, central);
            }
            #[cfg(not(any(feature = "left", feature = "right", feature = "receiver")))]
            {
                basic.await;
            }
            loop {
                Timer::after_secs(3600).await;
            }
        }
    }
}
#[cfg(feature = "receiver")]
impl<D: Driver<'static>> Service<D> {
    async fn run_receiver(&mut self) -> ! {
        use rmk::embassy_futures::select::{Either4, select4};
        loop {
            RECEIVER.lock(|c| {
                let mut s = c.get();
                s.expire(Instant::now());
                c.set(s);
            });
            let expiry = async {
                if let Some(at) = RECEIVER.lock(|c| c.get().seen) {
                    Timer::at(at + Duration::from_secs(12)).await;
                } else {
                    core::future::pending::<()>().await;
                }
            };
            match select4(
                self.custom.next_event(),
                self.dongle.next_event(),
                next_request(&mut self.hid),
                expiry,
            )
            .await
            {
                Either4::First(message) => RECEIVER.lock(|c| {
                    let mut s = c.get();
                    s.accept(&message, Instant::now());
                    c.set(s);
                }),
                Either4::Second(event) => RECEIVER.lock(|c| {
                    let mut s = c.get();
                    s.linked = event.0 == rmk::event::DongleState::Connected;
                    if !s.linked {
                        s.invalidate();
                    }
                    c.set(s);
                }),
                Either4::Third(Some(request)) => {
                    if recovery(&request) {
                        let _ = with_timeout(Duration::from_millis(250), self.hid.write(&request))
                            .await;
                        rmk::boot::jump_to_bootloader();
                    }
                    let response = RECEIVER.lock(|c| {
                        let mut s = c.get();
                        s.expire(Instant::now());
                        c.set(s);
                        s.reply(&request)
                    });
                    if let Some(response) = response {
                        let _ = with_timeout(Duration::from_millis(250), self.hid.write(&response))
                            .await;
                    }
                }
                Either4::Third(None) => {}
                Either4::Fourth(()) => {} // The next loop expires the cache.
            }
        }
    }
}
#[cfg(any(feature = "receiver", test))]
#[derive(Clone, Copy)]
struct ReceiverCache {
    report: [u8; 32],
    seen: Option<Instant>,
    linked: bool,
}
#[cfg(any(feature = "receiver", test))]
impl ReceiverCache {
    const EMPTY: Self = Self {
        report: {
            let mut report = [0; 32];
            let header = [8, 0x7e, 1, 2, b'N', b'C', b'B', b'T'];
            let mut i = 0;
            while i < header.len() {
                report[i] = header[i];
                i += 1;
            }
            report[10] = 255;
            report[14] = 255;
            report
        },
        seen: None,
        linked: false,
    };
    fn invalidate(&mut self) {
        self.report = Self::EMPTY.report;
        self.seen = None;
    }
    fn expire(&mut self, now: Instant) {
        if self
            .seen
            .is_some_and(|at| now >= at + Duration::from_secs(12))
        {
            self.invalidate();
        }
    }
    fn accept(&mut self, message: &CustomMessage, now: Instant) {
        if !self.linked
            || message.target != CustomMessageTarget::Dongle
            || message.data.len() != 37
            || message.data[..5] != [b'N', b'C', b'S', 1, 1]
            || !valid_status(&message.data[5..])
        {
            return;
        }
        self.report.copy_from_slice(&message.data[5..]);
        if self.report[9] == 2 {
            self.report[9..12].copy_from_slice(&[0, 255, 0]);
        }
        if self.report[12] != 1 {
            self.report[12] = 0;
            self.report[13..16].copy_from_slice(&[0, 255, 0]);
        } else if self.report[13] == 2 {
            self.report[13..16].copy_from_slice(&[0, 255, 0]);
        }
        self.seen = Some(now);
    }
    fn reply(self, request: &[u8; 32]) -> Option<[u8; 32]> {
        if pairing_request(request) {
            let mut r = *request;
            r[8] = 3;
            return Some(r);
        }
        if request[..3] != HEADER || request[4..8] != *SIGNATURE {
            return None;
        }
        let status = if !matches!(request[3], 1 | 2) {
            1
        } else if request[8..].iter().any(|b| *b != 0) {
            2
        } else {
            0
        };
        let mut r = if status == 0 {
            self.report
        } else {
            let mut empty = Self::EMPTY.report;
            empty[12] = 2;
            empty
        };
        r[3] = if request[3] == 2 { 2 } else { 1 };
        r[8] = status;
        if r[3] == 1 {
            r[16..].fill(0);
        }
        Some(r)
    }
}
#[cfg(feature = "receiver")]
static RECEIVER: Mutex<rmk::RawMutex, Cell<ReceiverCache>> =
    Mutex::new(Cell::new(ReceiverCache::EMPTY));
#[cfg(not(feature = "receiver"))]
fn can_forward(role: Role, s: Snapshot) -> bool {
    if s.sleeping {
        return false;
    }
    match role {
        Role::Right => s.central_link,
        Role::Left => {
            let status = rmk::state::current_connection_status();
            status.ble.state == rmk::types::ble::BleState::Connected
                && usize::from(status.ble.profile) == rmk::types::constants::NUM_BLE_PROFILE
        }
        Role::Receiver => false,
    }
}
#[cfg(not(feature = "receiver"))]
fn refresh_route() {
    let status = rmk::state::current_connection_status();
    update(|s| {
        s.facts[0] &= !28;
        s.facts[2] |= 4;
        use rmk::types::{ble::BleState, connection::UsbState};
        if s.facts[1] == 1 && matches!(status.usb, UsbState::Configured | UsbState::Suspended) {
            s.facts[0] |= 4;
        } else if status.ble.state == BleState::Connected {
            if s.facts[1] == 2
                && usize::from(status.ble.profile) < rmk::types::constants::NUM_BLE_PROFILE
            {
                s.facts[0] |= 8;
            } else if s.facts[1] == 3
                && usize::from(status.ble.profile) == rmk::types::constants::NUM_BLE_PROFILE
            {
                s.facts[0] |= 16;
            }
        }
        if s.peer_at
            .is_some_and(|at| at.elapsed() > Duration::from_secs(12))
        {
            s.facts[0] &= !2;
            s.facts[2] &= !2;
        }
    });
}
#[cfg(any(not(feature = "receiver"), test))]
fn pack(report: [u8; 32], role: u8) -> [u8; 37] {
    let mut out = [0; 37];
    out[..5].copy_from_slice(&[b'N', b'C', b'S', 1, role]);
    out[5..].copy_from_slice(&report);
    out
}
#[cfg(not(feature = "receiver"))]
fn accept_message(role: Role, message: &CustomMessage) {
    if role != Role::Left
        || message.target != CustomMessageTarget::Central
        || message.data.len() != 37
        || message.data[..5] != [b'N', b'C', b'S', 1, 2]
        || !valid_status(&message.data[5..])
    {
        return;
    }
    let report = &message.data[5..];
    update(|s| {
        if s.connected {
            s.facts[0] = (s.facts[0] & !2) | ((report[16] & 1) << 1);
            s.facts[2] = (s.facts[2] & !2) | ((report[18] & 1) << 1);
            s.peer_at = Some(Instant::now());
        }
    });
}
#[cfg(not(feature = "receiver"))]
fn mode_reply(req: &[u8; 32], mode: u8) -> Option<[u8; 32]> {
    if req[..3] != [8, 0x7e, 3] || req[4..8] != *b"NCMO" {
        return None;
    }
    let mut r = [0; 32];
    r[..8].copy_from_slice(&[8, 0x7e, 3, 1, b'N', b'C', b'M', b'O']);
    r[8] = if req[3] != 1 {
        1
    } else if req[8..].iter().any(|v| *v != 0) {
        2
    } else {
        0
    };
    if r[8] == 0 {
        r[9] = mode;
    }
    Some(r)
}

// Existing Companion telemetry payloads on the board-owned HID collection.
// Unobserved producer facts stay unknown; disconnected links invalidate them.
#[cfg(any(not(feature = "receiver"), test))]
use rmk::types::battery::ChargeState;
const HEADER: [u8; 3] = [0x08, 0x7e, 1];
const SIGNATURE: &[u8; 4] = b"NCBT";
#[cfg(any(not(feature = "receiver"), test))]
const VERSION: u8 = 1;

/// Version two adds authoritative power, route and switch facts without
/// changing Vial framing or the version-one battery getter.
#[cfg(any(not(feature = "receiver"), test))]
fn status_reply(
    request: &[u8; 32],
    supported: bool,
    local: BatteryStatus,
    peripheral: Option<(bool, BatteryStatus)>,
    facts: [u8; 3],
) -> Option<[u8; 32]> {
    if request[3] != 2 {
        return None;
    }
    let mut compatible = *request;
    compatible[3] = VERSION;
    let mut response = reply(&compatible, supported, local, peripheral)?;
    response[3] = 2;
    if response[8] == 0 {
        response[16..19].copy_from_slice(&facts);
    }
    Some(response)
}

#[cfg(any(not(feature = "receiver"), test))]
fn entry(status: BatteryStatus) -> [u8; 3] {
    match status {
        BatteryStatus::Unavailable => [0, 0xff, 0],
        BatteryStatus::Available {
            level: Some(101..=255),
            ..
        } => [2, 0xff, 0],
        BatteryStatus::Available {
            level,
            charge_state,
        } => [
            1,
            level.unwrap_or(0xff),
            match charge_state {
                ChargeState::Unknown => 0,
                ChargeState::Charging => 1,
                ChargeState::Discharging => 2,
            },
        ],
    }
}
#[cfg(any(not(feature = "receiver"), test))]
fn reply(
    request: &[u8; 32],
    supported: bool,
    local: BatteryStatus,
    peripheral: Option<(bool, BatteryStatus)>,
) -> Option<[u8; 32]> {
    if request[..3] != HEADER || &request[4..8] != SIGNATURE {
        return None;
    }
    let mut response = [0; 32];
    response[..3].copy_from_slice(&HEADER);
    response[3] = VERSION;
    response[4..8].copy_from_slice(SIGNATURE);
    response[9..12].copy_from_slice(&entry(BatteryStatus::Unavailable));
    response[12] = 2; // No configured peripheral by default.
    response[13..16].copy_from_slice(&entry(BatteryStatus::Unavailable));
    response[8] = if request[3] != VERSION {
        1
    } else if request[8..].iter().any(|byte| *byte != 0) {
        2
    } else if !supported {
        3
    } else {
        0
    };
    if response[8] != 0 {
        return Some(response);
    }
    response[9..12].copy_from_slice(&entry(local));
    if let Some((connected, battery)) = peripheral {
        response[12] = u8::from(connected);
        if connected {
            response[13..16].copy_from_slice(&entry(battery));
        }
    }
    Some(response)
}
/// Separate diagnostic getter; value 1's battery snapshot remains unchanged.
#[cfg(not(feature = "receiver"))]
fn diagnostic_reply(
    request: &[u8; 32],
    snapshot: Option<(i16, u64, u8, [u32; 4])>,
) -> Option<[u8; 32]> {
    const HEADER: [u8; 3] = [0x08, 0x7e, 2];
    const SIGNATURE: &[u8; 4] = b"NCAD";
    if request[..3] != HEADER || &request[4..8] != SIGNATURE {
        return None;
    }
    let mut response = [0; 32];
    response[..3].copy_from_slice(&HEADER);
    response[3] = VERSION;
    response[4..8].copy_from_slice(SIGNATURE);
    response[8] = if request[3] != VERSION {
        1
    } else if request[8..].iter().any(|byte| *byte != 0) {
        2
    } else if snapshot.is_none() {
        3
    } else {
        0
    };
    if response[8] != 0 {
        return Some(response);
    }
    let (signed, age_ms, switch_levels, registers) = snapshot?;
    response[9..11].copy_from_slice(&signed.to_le_bytes());
    response[11..15].copy_from_slice(&(age_ms.min(u64::from(u32::MAX)) as u32).to_le_bytes());
    response[15] = switch_levels & 7;
    for (bytes, value) in response[16..].chunks_exact_mut(4).zip(registers) {
        bytes.copy_from_slice(&value.to_le_bytes());
    }
    Some(response)
}

fn valid_status(p: &[u8]) -> bool {
    if p.len() != 32
        || p[..8] != [8, 0x7e, 1, 2, b'N', b'C', b'B', b'T']
        || p[8] != 0
        || p[12] > 2
        || p[16] & !31 != 0
        || p[17] > 3
        || p[18] & !15 != 0
        || p[19..].iter().any(|b| *b != 0)
    {
        return false;
    }
    let entry = |e: &[u8]| match e[0] {
        0 | 2 => e[1] == 255 && e[2] == 0,
        1 => (e[1] <= 100 || e[1] == 255) && e[2] <= 2,
        _ => false,
    };
    entry(&p[9..12]) && entry(&p[13..16])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_recovery_rejects_every_single_byte_mutation() {
        let mut r = [0; 32];
        r[..8].copy_from_slice(&[8, 0x7e, 5, 1, b'N', b'C', b'R', b'C']);
        assert!(recovery(&r));
        for i in 0..32 {
            let mut bad = r;
            bad[i] ^= 1;
            assert!(!recovery(&bad));
        }
    }
    #[test]
    fn compatible_status_marks_unknown_and_disconnect_hides_battery() {
        let mut s = Snapshot::EMPTY;
        assert!(valid_status(&s.wire()));
        assert_eq!(&s.wire()[9..16], &[0, 255, 0, 0, 0, 255, 0]);
        s.right = BatteryStatus::Available {
            level: Some(88),
            charge_state: ChargeState::Charging,
        };
        s.connected = true;
        assert_eq!(&s.wire()[12..16], &[1, 1, 88, 1]);
        s.connected = false;
        assert_eq!(&s.wire()[12..16], &[0, 0, 255, 0]);
    }
    #[test]
    fn invalid_board_reports_are_rejected() {
        let good = Snapshot::EMPTY.wire();
        for (i, v) in [
            (8, 1),
            (9, 3),
            (10, 101),
            (11, 3),
            (12, 3),
            (16, 32),
            (17, 4),
            (18, 16),
            (19, 1),
            (31, 1),
        ] {
            let mut bad = good;
            bad[i] = v;
            assert!(!valid_status(&bad));
        }
        assert!(!valid_status(&good[..31]));
    }
    #[test]
    fn status_v2_keeps_power_separate_from_route() {
        let mut s = Snapshot::EMPTY;
        s.facts = [9, 2, 15];
        assert_eq!(&s.wire()[16..19], &[9, 2, 15]);
        assert!(valid_status(&s.wire()));
    }
    #[cfg(not(feature = "receiver"))]
    #[test]
    fn half_forwarding_accepts_only_live_right_power_facts() {
        CACHE.lock(|c| c.set(Snapshot::EMPTY));
        let mut right = Snapshot::EMPTY;
        right.facts = [1, 0, 1];
        let message =
            CustomMessage::new(&pack(right.wire(), 2), CustomMessageTarget::Central).unwrap();
        accept_message(Role::Left, &message);
        assert_eq!(snapshot().facts, [0, 0, 0]);
        update(|s| {
            s.connected = true;
            s.facts = [1, 2, 1];
        });
        accept_message(Role::Left, &message);
        assert_eq!(snapshot().facts, [3, 2, 3]);
        let wrong =
            CustomMessage::new(&pack(right.wire(), 1), CustomMessageTarget::Central).unwrap();
        update(|s| s.facts = [1, 2, 1]);
        accept_message(Role::Left, &wrong);
        assert_eq!(snapshot().facts, [1, 2, 1]);
        update(|s| s.connected = false);
        accept_message(Role::Left, &message);
        assert_eq!(snapshot().facts, [1, 2, 1]);
    }
    #[cfg(not(feature = "receiver"))]
    #[test]
    fn disconnected_and_sleeping_forwarders_do_not_schedule_refresh() {
        let mut s = Snapshot::EMPTY;
        assert!(!can_forward(Role::Right, s));
        assert!(!can_forward(Role::Receiver, s));
        s.central_link = true;
        assert!(can_forward(Role::Right, s));
        s.sleeping = true;
        assert!(!can_forward(Role::Right, s));
    }
    #[test]
    fn receiver_cache_preserves_facts_and_normalizes_then_expires() {
        let at = Instant::from_ticks(100);
        let mut cache = ReceiverCache::EMPTY;
        let mut packet = Snapshot::EMPTY.wire();
        packet[9..19].copy_from_slice(&[1, 93, 2, 1, 1, 82, 1, 9, 2, 15]);
        let message = CustomMessage::new(&pack(packet, 1), CustomMessageTarget::Dongle).unwrap();
        cache.accept(&message, at);
        assert!(cache.seen.is_none());
        cache.linked = true;
        cache.accept(&message, at);
        let mut request = [0; 32];
        request[..8].copy_from_slice(&[8, 0x7e, 1, 2, b'N', b'C', b'B', b'T']);
        let full = cache.reply(&request).unwrap();
        assert_eq!(&full[9..19], &packet[9..19]);
        let wrong = CustomMessage::new(&pack(packet, 2), CustomMessageTarget::Dongle).unwrap();
        cache.accept(&wrong, at + Duration::from_secs(1));
        assert_eq!(cache.seen, Some(at));
        let wrong = CustomMessage::new(&pack(packet, 1), CustomMessageTarget::Central).unwrap();
        cache.accept(&wrong, at + Duration::from_secs(1));
        assert_eq!(cache.seen, Some(at));
        packet[9..12].copy_from_slice(&[2, 255, 0]);
        packet[12] = 2;
        packet[13..16].copy_from_slice(&[1, 82, 1]);
        cache.accept(
            &CustomMessage::new(&pack(packet, 1), CustomMessageTarget::Dongle).unwrap(),
            at,
        );
        assert_eq!(
            &cache.reply(&request).unwrap()[9..16],
            &[0, 255, 0, 0, 0, 255, 0]
        );
        let mut legacy = request;
        legacy[3] = 1;
        assert_eq!(&cache.reply(&legacy).unwrap()[16..], &[0; 16]);
        cache.expire(at + Duration::from_secs(11));
        assert!(cache.seen.is_some());
        cache.expire(at + Duration::from_secs(12));
        assert!(cache.seen.is_none());
        assert!(cache.linked);
        assert_eq!(
            &cache.reply(&request).unwrap()[9..19],
            &[0, 255, 0, 0, 0, 255, 0, 0, 0, 0]
        );
        let mut bad = request;
        bad[31] = 1;
        let error = cache.reply(&bad).unwrap();
        assert_eq!(error[8], 2);
        assert_eq!(error[12], 2);
        bad = request;
        bad[3] = 9;
        let error = cache.reply(&bad).unwrap();
        assert_eq!(error[3], 1);
        assert_eq!(error[8], 1);
        bad = request;
        bad[4] = 0;
        assert!(cache.reply(&bad).is_none());
        cache.invalidate();
        assert!(cache.seen.is_none());
    }
    #[test]
    fn malformed_readonly_getter_does_not_publish_private_facts() {
        let mut r = [0; 32];
        r[..8].copy_from_slice(&[8, 0x7e, 1, 2, b'N', b'C', b'B', b'T']);
        r[31] = 1;
        let reply = status_reply(&r, true, BatteryStatus::Unavailable, None, [31, 3, 15]).unwrap();
        assert_eq!(reply[8], 2);
        assert_eq!(&reply[16..], &[0; 16]);
    }
}

trait ReadReport {
    async fn ready(&mut self);
    async fn read_report(&mut self, buffer: &mut [u8; 32]) -> Result<usize, ()>;
}
impl<D: Driver<'static>> ReadReport for HidReaderWriter<'static, D, 32, 32> {
    async fn ready(&mut self) {
        HidReaderWriter::ready(self).await;
    }
    async fn read_report(&mut self, b: &mut [u8; 32]) -> Result<usize, ()> {
        self.read(b).await.map_err(|_| ())
    }
}
async fn next_request(hid: &mut impl ReadReport) -> Option<[u8; 32]> {
    hid.ready().await; // disabled endpoints block, preserving BLE and watchdog progress
    let mut request = [0; 32];
    if matches!(hid.read_report(&mut request).await, Ok(32)) {
        Some(request)
    } else {
        Timer::after_millis(20).await;
        None
    } // malformed/overflow input cannot spin
}
fn pairing_request(r: &[u8; 32]) -> bool {
    r[..8] == [8, 0x7e, 6, 1, b'N', b'C', b'P', b'A'] && r[8..].iter().all(|v| *v == 0)
}
#[cfg(any(feature = "left", test))]
fn pairing_allowed(role: Role, mode: u8, profile: u8) -> bool {
    role == Role::Left
        && mode == 3
        && usize::from(profile) == rmk::types::constants::NUM_BLE_PROFILE
}
#[cfg(test)]
mod read_tests {
    use super::*;
    use core::{
        future::{Future, poll_fn},
        pin::pin,
        task::Poll,
    };
    struct Disabled {
        ready_polled: bool,
        read_called: bool,
    }
    impl ReadReport for Disabled {
        async fn ready(&mut self) {
            self.ready_polled = true;
            poll_fn(|_| Poll::<()>::Pending).await;
        }
        async fn read_report(&mut self, _: &mut [u8; 32]) -> Result<usize, ()> {
            self.read_called = true;
            Err(())
        }
    }
    #[test]
    fn disabled_hid_yields_to_other_tasks_and_can_be_cancelled_without_read_spin() {
        let mut hid = Disabled {
            ready_polled: false,
            read_called: false,
        };
        let mut progressed = false;
        {
            let mut service = pin!(async {
                rmk::futures::join!(next_request(&mut hid), async {
                    progressed = true;
                });
            });
            let w = rmk::futures::task::noop_waker();
            let mut cx = core::task::Context::from_waker(&w);
            assert!(service.as_mut().poll(&mut cx).is_pending());
        }
        assert!(progressed);
        assert!(hid.ready_polled);
        assert!(!hid.read_called);
    }
    #[test]
    fn native_pairing_cannot_clear_other_roles_modes_or_host_profiles() {
        let profile = rmk::types::constants::NUM_BLE_PROFILE as u8;
        assert!(pairing_allowed(Role::Left, 3, profile));
        for role in [Role::Right, Role::Receiver] {
            assert!(!pairing_allowed(role, 3, profile));
        }
        for mode in [0, 1, 2] {
            assert!(!pairing_allowed(Role::Left, mode, profile));
        }
        assert!(!pairing_allowed(Role::Left, 3, 0));
        let mut request = [0; 32];
        request[..8].copy_from_slice(&[8, 0x7e, 6, 1, b'N', b'C', b'P', b'A']);
        assert!(pairing_request(&request));
        for i in 0..32 {
            let mut bad = request;
            bad[i] ^= 1;
            assert!(!pairing_request(&bad));
        }
    }
}

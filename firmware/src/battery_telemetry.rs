//! Optional read-only USB battery interface, built from RMK's public events.
use core::sync::atomic::{AtomicU8, Ordering};

#[cfg(feature = "receiver")]
use embassy_futures::select::{Either, select};
#[cfg(feature = "left")]
use embassy_futures::select::{Either4, select4};
use embassy_time::Timer;
use embassy_time::{Duration, Instant};
use embassy_usb::class::hid::{Config, HidBootProtocol, HidReaderWriter, HidSubclass, State};
use embassy_usb::driver::Driver;
#[cfg(feature = "left")]
use rmk::{
    custom_message,
    event::{BatteryStatusEvent, PeripheralBatteryEvent, PeripheralConnectedEvent},
    types::battery::BatteryStatus,
};
use rmk::{
    custom_message::{CustomMessage, CustomMessageTarget},
    event::{EventSubscriber, SubscribableEvent},
};
use static_cell::StaticCell;

const MAGIC: u8 = 0xb2;
const UNKNOWN: u8 = 255;
static LEFT: AtomicU8 = AtomicU8::new(UNKNOWN);
static RIGHT: AtomicU8 = AtomicU8::new(UNKNOWN);

// One-byte request, four-byte reply. This is a separate HID interface;
// the standard keyboard and Vial reports remain RMK's.
const DESCRIPTOR: &[u8] = &[
    0x06, 0x60, 0xff, 0x09, 0x62, 0xa1, 0x01, 0x15, 0x00, 0x26, 0xff, 0x00, 0x75, 0x08, 0x95, 0x01,
    0x09, 0x01, 0x91, 0x02, 0x95, 0x04, 0x09, 0x02, 0x81, 0x02, 0xc0,
];

pub fn usb<D: Driver<'static>>(
    builder: &mut embassy_usb::Builder<'static, D>,
) -> HidReaderWriter<'static, D, 1, 4> {
    static STATE: StaticCell<State> = StaticCell::new();
    HidReaderWriter::new(
        builder,
        STATE.init(State::new()),
        Config {
            report_descriptor: DESCRIPTOR,
            request_handler: None,
            poll_ms: 10,
            max_packet_size: 8,
            hid_subclass: HidSubclass::No,
            hid_boot_protocol: HidBootProtocol::None,
        },
    )
}

fn frame() -> [u8; 4] {
    [
        MAGIC,
        1,
        LEFT.load(Ordering::Relaxed),
        RIGHT.load(Ordering::Relaxed),
    ]
}

pub async fn run_usb<D: Driver<'static>>(mut hid: HidReaderWriter<'static, D, 1, 4>) -> ! {
    let mut request = [0u8; 1];
    loop {
        match hid.read(&mut request).await {
            Ok(1) if request[0] == 1 => {
                let _ = hid.write(&frame()).await;
            }
            Err(embassy_usb::class::hid::ReadError::Disabled) => hid.ready().await,
            _ => {}
        }
    }
}

#[cfg(feature = "left")]
fn level(status: BatteryStatus) -> u8 {
    match status {
        BatteryStatus::Available {
            level: Some(value), ..
        } if value <= 100 => value,
        _ => UNKNOWN,
    }
}

#[cfg(feature = "left")]
fn send_to_dongle() {
    if let Ok(message) = CustomMessage::new(&frame(), CustomMessageTarget::Dongle) {
        custom_message::send(message);
    }
}

#[cfg(feature = "left")]
pub async fn run_left() -> ! {
    let mut left = BatteryStatusEvent::subscriber();
    let mut right = PeripheralBatteryEvent::subscriber();
    let mut link = PeripheralConnectedEvent::subscriber();
    let mut last_sent = [UNKNOWN, UNKNOWN];
    let mut last_sent_at: Option<Instant> = None;
    loop {
        match select4(
            left.next_event(),
            right.next_event(),
            link.next_event(),
            Timer::after_secs(60),
        )
        .await
        {
            Either4::First(event) => LEFT.store(level(event.0), Ordering::Relaxed),
            Either4::Second(event) if event.id == 0 => {
                RIGHT.store(level(event.state.0), Ordering::Relaxed)
            }
            Either4::Third(event) if event.id == 0 && !event.connected => {
                RIGHT.store(UNKNOWN, Ordering::Relaxed)
            }
            _ => {}
        }
        let current = [LEFT.load(Ordering::Relaxed), RIGHT.load(Ordering::Relaxed)];
        // Changes go out at once. A sparse heartbeat lets the receiver expire
        // lost readings without sending frequent radio messages.
        if current != last_sent
            || last_sent_at.is_none_or(|sent| sent.elapsed() >= Duration::from_secs(60))
        {
            send_to_dongle();
            last_sent = current;
            last_sent_at = Some(Instant::now());
        }
    }
}

#[cfg(feature = "receiver")]
pub async fn run_receiver(router: &rmk::dongle::DongleRouter) -> ! {
    let mut messages = CustomMessage::subscriber();
    let mut last_message = None;
    loop {
        match select(messages.next_event(), Timer::after_millis(250)).await {
            Either::First(message)
                if router.is_connected()
                    && message.target == CustomMessageTarget::Dongle
                    && message.data.len() == 4
                    && message.data[0] == MAGIC
                    && message.data[1] == 1 =>
            {
                LEFT.store(message.data[2], Ordering::Relaxed);
                RIGHT.store(message.data[3], Ordering::Relaxed);
                last_message = Some(Instant::now());
            }
            _ => {}
        }
        if !router.is_connected()
            || last_message.is_some_and(|seen: Instant| seen.elapsed() >= Duration::from_secs(135))
        {
            LEFT.store(UNKNOWN, Ordering::Relaxed);
            RIGHT.store(UNKNOWN, Ordering::Relaxed);
            last_message = None;
        }
    }
}

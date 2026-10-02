#![no_std]
#![no_main]

#[cfg(any(
    not(any(feature = "left", feature = "right")),
    all(feature = "left", feature = "right"),
    feature = "receiver"
))]
compile_error!("The input probe requires exactly one left or right role");
#[cfg(feature = "reclaimed-softdevice")]
compile_error!("The input probe must preserve the resident SoftDevice");
#[cfg(not(feature = "usb-recovery-first"))]
compile_error!("The input probe requires the explicit usb-recovery-first feature");

#[path = "../keymap.rs"]
mod keymap;
#[path = "../scanner.rs"]
mod scanner;

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::usb::{Driver, vbus_detect::HardwareVbusDetect};
use embassy_nrf::{bind_interrupts, peripherals::TWISPI0, peripherals::USBD, twim, usb};
use embassy_time::Timer;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
// Link the existing NVIC critical-section implementation; no radio/MPSL init.
use nrf_mpsl as _;
use panic_probe as _;
use rmk::{
    KeymapData,
    config::{BehaviorConfig, DeviceConfig, PositionalConfig},
    initialize_keymap,
    keyboard::Keyboard,
    run_all,
    types::action::{Action, KeyAction},
    usb::UsbTransport,
};
use static_cell::StaticCell;

#[cfg(feature = "left")]
const KEY_COUNT: usize = 37;
#[cfg(not(feature = "left"))]
const KEY_COUNT: usize = 47;
#[cfg(feature = "left")]
const KEY_START: usize = 0;
#[cfg(not(feature = "left"))]
const KEY_START: usize = 37;
#[cfg(all(feature = "left", not(feature = "mac-keymap")))]
const PRODUCT: &str = "NocFree Input Probe Left";
#[cfg(all(not(feature = "left"), not(feature = "mac-keymap")))]
const PRODUCT: &str = "NocFree Input Probe Right";
#[cfg(all(feature = "left", feature = "mac-keymap"))]
const PRODUCT: &str = "NocFree Input Probe Left Mac";
#[cfg(all(not(feature = "left"), feature = "mac-keymap"))]
const PRODUCT: &str = "NocFree Input Probe Right Mac";
#[cfg(feature = "left")]
const PID: u16 = 0x4652;
#[cfg(not(feature = "left"))]
const PID: u16 = 0x4651;
#[cfg(all(feature = "left", not(feature = "mac-keymap")))]
const GREETING: &str = concat!(
    "NocFree input probe ",
    env!("CARGO_PKG_VERSION"),
    " left factory\r\n"
);
#[cfg(all(not(feature = "left"), not(feature = "mac-keymap")))]
const GREETING: &str = concat!(
    "NocFree input probe ",
    env!("CARGO_PKG_VERSION"),
    " right factory\r\n"
);
#[cfg(all(feature = "left", feature = "mac-keymap"))]
const GREETING: &str = concat!(
    "NocFree input probe ",
    env!("CARGO_PKG_VERSION"),
    " left factory mac-mode\r\n"
);
#[cfg(all(not(feature = "left"), feature = "mac-keymap"))]
const GREETING: &str = concat!(
    "NocFree input probe ",
    env!("CARGO_PKG_VERSION"),
    " right factory mac-mode\r\n"
);

bind_interrupts!(struct Irqs {
    USBD => usb::InterruptHandler<USBD>;
    CLOCK_POWER => usb::vbus_detect::InterruptHandler;
    TWISPI0 => twim::InterruptHandler<TWISPI0>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = embassy_nrf::config::Config::default();
    // USB requires a stable HFXO; do not rely on the bootloader leaving it running.
    config.hfclk_source = embassy_nrf::config::HfclkSource::ExternalXtal;
    let p = embassy_nrf::init(config);
    let mut twim_buffer = [0u8; 8];
    let bus = twim::Twim::new(
        p.TWISPI0,
        Irqs,
        p.P0_11,
        p.P1_09,
        twim::Config::default(),
        &mut twim_buffer,
    );
    #[cfg(feature = "left")]
    let bits = &nocfree_input::LEFT_BITS;
    #[cfg(not(feature = "left"))]
    let bits = &nocfree_input::RIGHT_BITS;
    let mut matrix = scanner::Scanner::new(bus, bits);

    // Preserve the shared electrical order; each half's local indices start at zero.
    let complete = keymap::default_keymap();
    let mut local = [[[KeyAction::No; KEY_COUNT]; 1]; 2];
    for layer in 0..2 {
        local[layer][0].copy_from_slice(&complete[layer][0][KEY_START..KEY_START + KEY_COUNT]);
    }
    // No BLE controller/profile task exists in this USB-only diagnostic.
    for action in &mut local[1][0] {
        if matches!(action, KeyAction::Single(Action::User(_))) {
            *action = rmk::a!(Transparent);
        }
    }
    // Left retains shared Fn+Escape; right Fn+0 requests the same Adafruit DFU.
    #[cfg(feature = "right")]
    {
        local[1][0][11] = rmk::kbctrl!(Bootloader);
    }
    let mut data = KeymapData::new(local);
    let mut behavior = BehaviorConfig::default();
    let positional = PositionalConfig::default();
    let keymap = initialize_keymap(&mut data, &mut behavior, &positional).await;
    let mut keyboard = Keyboard::new(&keymap);

    let driver = Driver::new(p.USBD, Irqs, HardwareVbusDetect::new(Irqs));
    let device_config = DeviceConfig {
        vid: 0x4c4b,
        pid: PID,
        manufacturer: "NocFree RMK community",
        product_name: PRODUCT,
        ..Default::default()
    };
    // Production left's dongle feature adds RMK vendor + reset-only DFU interfaces.
    // RMK's builder reserves room for CDC; no radio or application DFU writer is started.
    let mut builder = UsbTransport::builder(driver, device_config);
    static CDC_STATE: StaticCell<State> = StaticCell::new();
    let cdc = CdcAcmClass::new(builder.usb_builder(), CDC_STATE.init(State::new()), 64);
    let mut usb = builder.build();
    let (mut sender, receiver, changes) = cdc.split_with_control();
    let greeting = async {
        loop {
            sender.wait_connection().await;
            while !sender.dtr() {
                Timer::after_millis(20).await;
            }
            let _ = sender.write_packet(GREETING.as_bytes()).await;
            while sender.dtr() {
                Timer::after_millis(20).await;
            }
        }
    };
    let update_entry = async {
        loop {
            changes.control_changed().await;
            // Independent of I2C/scanner health; an explicit host request only.
            if receiver.line_coding().data_rate() == 1200 && !changes.dtr() {
                rmk::boot::jump_to_bootloader();
            }
        }
    };
    rmk::embassy_futures::join::join3(run_all!(matrix, keyboard, usb), greeting, update_entry)
        .await;
}

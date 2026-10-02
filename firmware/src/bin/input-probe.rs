#![no_std]
#![no_main]

#[cfg(any(not(feature = "right"), feature = "left", feature = "receiver"))]
compile_error!("The input probe supports only the right half");
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

bind_interrupts!(struct Irqs {
    USBD => usb::InterruptHandler<USBD>;
    CLOCK_POWER => usb::vbus_detect::InterruptHandler;
    TWISPI0 => twim::InterruptHandler<TWISPI0>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(embassy_nrf::config::Config::default());
    let mut twim_buffer = [0u8; 8];
    let bus = twim::Twim::new(
        p.TWISPI0,
        Irqs,
        p.P0_11,
        p.P1_09,
        twim::Config::default(),
        &mut twim_buffer,
    );
    let mut matrix = scanner::Scanner::new(bus, &nocfree_input::RIGHT_BITS);

    // Preserve the shared electrical order; right-local indices start at zero.
    let complete = keymap::default_keymap();
    let mut local = [[[KeyAction::No; 47]; 1]; 2];
    for layer in 0..2 {
        local[layer][0].copy_from_slice(&complete[layer][0][37..]);
    }
    // No BLE controller/profile task exists in this USB-only diagnostic.
    for action in &mut local[1][0] {
        if matches!(action, KeyAction::Single(Action::User(_))) {
            *action = rmk::a!(Transparent);
        }
    }
    // Hold right Fn, press/release main-row 0, then release Fn to enter DFU.
    local[1][0][11] = rmk::kbctrl!(Bootloader);
    let mut data = KeymapData::new(local);
    let mut behavior = BehaviorConfig::default();
    let positional = PositionalConfig::default();
    let keymap = initialize_keymap(&mut data, &mut behavior, &positional).await;
    let mut keyboard = Keyboard::new(&keymap);

    let driver = Driver::new(p.USBD, Irqs, HardwareVbusDetect::new(Irqs));
    let mut builder = UsbTransport::builder(
        driver,
        DeviceConfig {
            vid: 0x4c4b,
            pid: 0x4651,
            manufacturer: "NocFree RMK community",
            product_name: "NocFree Input Probe Right",
            ..Default::default()
        },
    );
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
            let _ = sender
                .write_packet(
                    concat!(
                        "NocFree input probe ",
                        env!("CARGO_PKG_VERSION"),
                        " right factory\r\n"
                    )
                    .as_bytes(),
                )
                .await;
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

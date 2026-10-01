#![no_std]
#![no_main]

#[cfg(any(not(feature = "right"), feature = "left", feature = "receiver"))]
compile_error!("The recovery probe currently supports only the right half");
#[cfg(feature = "reclaimed-softdevice")]
compile_error!("The recovery probe must preserve the resident SoftDevice");
#[cfg(not(feature = "usb-recovery-first"))]
compile_error!("The recovery probe requires the explicit usb-recovery-first feature");

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::usb::{Driver, vbus_detect::HardwareVbusDetect};
use embassy_nrf::{bind_interrupts, peripherals::USBD, usb};
use embassy_time::Timer;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::{Builder, Config};
// Link the package's existing NVIC critical-section implementation; no MPSL init.
use nrf_mpsl as _;
use panic_probe as _;

bind_interrupts!(struct Irqs {
    USBD => usb::InterruptHandler<USBD>;
    CLOCK_POWER => usb::vbus_detect::InterruptHandler;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(embassy_nrf::config::Config::default());
    let driver = Driver::new(p.USBD, Irqs, HardwareVbusDetect::new(Irqs));
    let mut config = Config::new(0x4c4b, 0x4650);
    config.manufacturer = Some("NocFree RMK community");
    config.product = Some("NocFree Recovery Probe Right");
    config.serial_number = Some("nocfree-recovery-right-0.1.0");
    // CDC composite descriptors accepted by Windows as well as macOS/Linux.
    config.max_packet_size_0 = 64;
    config.device_class = 0xEF;
    config.device_sub_class = 0x02;
    config.device_protocol = 0x01;
    config.composite_with_iads = true;
    let mut descriptor = [0; 256];
    let mut bos = [0; 16];
    let mut msos = [0; 16];
    let mut control = [0; 64];
    let mut state = State::new();
    let mut builder = Builder::new(
        driver,
        config,
        &mut descriptor,
        &mut bos,
        &mut msos,
        &mut control,
    );
    let cdc = CdcAcmClass::new(&mut builder, &mut state, 64);
    let mut device = builder.build();
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
                        "NocFree recovery probe ",
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
            // An explicit 1200-baud touch with DTR low requests vendor DFU.
            // USB reset restores default line coding, so reconnect alone does not reset.
            if receiver.line_coding().data_rate() == 1200 && !changes.dtr() {
                rmk::boot::jump_to_bootloader();
            }
        }
    };
    rmk::embassy_futures::join::join3(device.run(), greeting, update_entry).await;
}

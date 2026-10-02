#![no_std]
#![no_main]
#![cfg_attr(feature = "migration-runtime-probe", allow(unreachable_code))]
#![cfg_attr(
    any(
        feature = "migration-hal-probe",
        feature = "migration-hal-serial-probe",
        feature = "migration-hal-neutral-probe",
        feature = "migration-usb-build-serial-probe"
    ),
    allow(unreachable_code, unused_variables)
)]

const _: () = assert!(
    (cfg!(feature = "migration-entry-probe") as usize
        + cfg!(feature = "migration-runtime-probe") as usize
        + cfg!(feature = "migration-hal-probe") as usize
        + cfg!(feature = "migration-hal-serial-probe") as usize
        + cfg!(feature = "migration-usb-build-serial-probe") as usize
        + cfg!(feature = "migration-hal-neutral-probe") as usize
        + cfg!(feature = "migration-usb-enabled-serial-probe") as usize
        + cfg!(feature = "migration-usb-configured-serial-probe") as usize)
        <= 1,
    "Select at most one migration startup stage"
);

// This variant observes application entry, not USB or typing behavior. The runtime
// sets VTOR before calling __pre_init; this hook immediately returns to the existing
// bootloader. Assembly avoids using Rust statics before data/BSS initialization.
// nrf-pac 0.4.0: POWER.GPREGRET = 0x40000000 + 0x51c.
// cortex-m 0.7.9 SCB::sys_reset: preserve AIRCR.PRIGROUP and surround reset with DSB.
#[cfg(feature = "migration-entry-probe")]
core::arch::global_asm!(
    ".pushsection .text.__pre_init,\"ax\",%progbits",
    ".balign 2",
    ".global __pre_init",
    ".type __pre_init,%function",
    ".thumb_func",
    "__pre_init:",
    "ldr r0, =0x4000051c",
    "movs r1, #0x57",
    "str r1, [r0]",
    "dsb",
    "ldr r0, =0xe000ed0c",
    "ldr r1, [r0]",
    "ldr r2, =0x700",
    "ands r1, r2",
    "ldr r2, =0x05fa0004",
    "orrs r1, r2",
    "str r1, [r0]",
    "dsb",
    "1:",
    "b 1b",
    ".size __pre_init, .-__pre_init",
    ".ltorg",
    ".popsection",
);

#[cfg(all(
    not(feature = "migration-probe"),
    any(not(feature = "right"), feature = "left", feature = "receiver")
))]
compile_error!("The recovery probe currently supports only the right half");
#[cfg(all(not(feature = "migration-probe"), feature = "reclaimed-softdevice"))]
compile_error!("The recovery probe must preserve the resident SoftDevice");
#[cfg(all(
    feature = "migration-probe",
    any(not(feature = "left"), feature = "right", feature = "receiver")
))]
compile_error!("The migration probe requires only the left role");
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

#[cfg(not(feature = "migration-probe"))]
const PID: u16 = 0x4650;
#[cfg(feature = "migration-probe")]
const PID: u16 = 0x4653;
#[cfg(not(feature = "migration-probe"))]
const PRODUCT: &str = "NocFree Recovery Probe Right";
#[cfg(feature = "migration-probe")]
const PRODUCT: &str = "NocFree Recovery Probe Left Migration";
#[cfg(not(feature = "migration-probe"))]
const SERIAL: &str = "nocfree-recovery-right-0.1.0";
#[cfg(feature = "migration-probe")]
const SERIAL: &str = "nocfree-recovery-left-migration-0.1.0";
#[cfg(not(feature = "migration-probe"))]
const GREETING: &str = concat!(
    "NocFree recovery probe ",
    env!("CARGO_PKG_VERSION"),
    " right factory\r\n"
);
#[cfg(feature = "migration-probe")]
const GREETING: &str = concat!(
    "NocFree recovery probe ",
    env!("CARGO_PKG_VERSION"),
    " left migration\r\n"
);

bind_interrupts!(struct Irqs {
    USBD => usb::InterruptHandler<USBD>;
    CLOCK_POWER => usb::vbus_detect::InterruptHandler;
});

// These stages run after RAM initialization. The request selects the existing
// bootloader path; no logging or USB activity is required for the reset.
#[cfg(any(
    feature = "migration-runtime-probe",
    feature = "migration-hal-probe",
    feature = "migration-hal-serial-probe",
    feature = "migration-usb-build-serial-probe",
    feature = "migration-hal-neutral-probe",
    feature = "migration-usb-enabled-serial-probe",
    feature = "migration-usb-configured-serial-probe"
))]
fn return_to_bootloader(request: u32) -> ! {
    // nrf-pac 0.4.0: POWER.GPREGRET = 0x40000000 + 0x51c.
    unsafe { core::ptr::write_volatile(0x4000_051c as *mut u32, request) };
    cortex_m::asm::dsb();
    cortex_m::peripheral::SCB::sys_reset();
}

// These callbacks report framework boundaries without replacing its USB runner.
// The handler lives in main's future for the same lifetime as the built device.
#[cfg(any(
    feature = "migration-usb-enabled-serial-probe",
    feature = "migration-usb-configured-serial-probe"
))]
struct UsbStageHandler;

#[cfg(any(
    feature = "migration-usb-enabled-serial-probe",
    feature = "migration-usb-configured-serial-probe"
))]
impl embassy_usb::Handler for UsbStageHandler {
    #[cfg(feature = "migration-usb-enabled-serial-probe")]
    fn enabled(&mut self, enabled: bool) {
        if enabled {
            return_to_bootloader(0x4e);
        }
    }

    #[cfg(feature = "migration-usb-configured-serial-probe")]
    fn configured(&mut self, configured: bool) {
        if configured {
            return_to_bootloader(0x4e);
        }
    }
}

// Keep the normal probe's startup path identical; stage returns deliberately
// terminate it early, and the HAL stage deliberately leaves its peripherals unused.
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    #[cfg(feature = "migration-runtime-probe")]
    return_to_bootloader(0x57);

    let mut nrf_config = embassy_nrf::config::Config::default();
    // USB requires a stable HFXO; do not rely on the bootloader leaving it running.
    nrf_config.hfclk_source = embassy_nrf::config::HfclkSource::ExternalXtal;
    let p = embassy_nrf::init(nrf_config);
    #[cfg(feature = "migration-hal-probe")]
    return_to_bootloader(0x57);
    #[cfg(feature = "migration-hal-serial-probe")]
    return_to_bootloader(0x4e);
    #[cfg(feature = "migration-hal-neutral-probe")]
    return_to_bootloader(0x00);

    let driver = Driver::new(p.USBD, Irqs, HardwareVbusDetect::new(Irqs));
    let mut config = Config::new(0x4c4b, PID);
    config.manufacturer = Some("NocFree RMK community");
    config.product = Some(PRODUCT);
    config.serial_number = Some(SERIAL);
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
    #[cfg(any(
        feature = "migration-usb-enabled-serial-probe",
        feature = "migration-usb-configured-serial-probe"
    ))]
    let mut stage_handler = UsbStageHandler;
    let mut builder = Builder::new(
        driver,
        config,
        &mut descriptor,
        &mut bos,
        &mut msos,
        &mut control,
    );
    #[cfg(any(
        feature = "migration-usb-enabled-serial-probe",
        feature = "migration-usb-configured-serial-probe"
    ))]
    builder.handler(&mut stage_handler);
    let cdc = CdcAcmClass::new(&mut builder, &mut state, 64);
    let mut device = builder.build();
    #[cfg(feature = "migration-usb-build-serial-probe")]
    {
        // Materialize the device before this deliberately diverging stage return.
        core::hint::black_box(&mut device);
        return_to_bootloader(0x4e);
    }
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
            // An explicit 1200-baud touch with DTR low requests vendor DFU.
            // USB reset restores default line coding, so reconnect alone does not reset.
            if receiver.line_coding().data_rate() == 1200 && !changes.dtr() {
                rmk::boot::jump_to_bootloader();
            }
        }
    };
    rmk::embassy_futures::join::join3(device.run(), greeting, update_entry).await;
}

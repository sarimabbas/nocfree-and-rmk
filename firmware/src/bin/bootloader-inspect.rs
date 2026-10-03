#![no_std]
#![no_main]

#[cfg(any(
    not(feature = "bootloader-inspect"),
    not(feature = "reclaimed-softdevice"),
    not(feature = "usb-recovery-first"),
    feature = "receiver",
    all(feature = "left", feature = "right"),
    not(any(feature = "left", feature = "right"))
))]
compile_error!(
    "Inspection requires exactly left or right, reclaimed-softdevice and usb-recovery-first"
);

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::usb::{Driver, vbus_detect::HardwareVbusDetect};
use embassy_nrf::{bind_interrupts, peripherals::USBD, usb};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::{Builder, Config};
// Reuse the package's NVIC critical-section implementation, without radio init.
use nrf_mpsl as _;
use panic_probe as _;

#[cfg(feature = "left")]
const PRODUCT: &str = "NocFree Boot Inspect Left";
#[cfg(feature = "right")]
const PRODUCT: &str = "NocFree Boot Inspect Right";
#[cfg(feature = "left")]
const PID: u16 = 0x4670;
#[cfg(feature = "right")]
const PID: u16 = 0x4671;
#[cfg(feature = "left")]
const SERIAL: &str = "nocfree-boot-inspect-left-v1";
#[cfg(feature = "right")]
const SERIAL: &str = "nocfree-boot-inspect-right-v1";
#[cfg(feature = "left")]
const HEADER: &[u8] = b"BOOT_READ_V1 LEFT\n";
#[cfg(feature = "right")]
const HEADER: &[u8] = b"BOOT_READ_V1 RIGHT\n";
const REQUEST: &[u8] = b"READ_BOOT_V1";
// Exclusive ends. Include REGOUT0 at UICR+0x304; never read FICR identifiers.
const SPANS: [(u32, u32); 3] = [(0, 0x1000), (0x74000, 0x80000), (0x10001000, 0x10001308)];

bind_interrupts!(struct Irqs {
    USBD => usb::InterruptHandler<USBD>;
    CLOCK_POWER => usb::vbus_detect::InterruptHandler;
});

fn hex(word: u32, output: &mut [u8]) {
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = b"0123456789ABCDEF"[((word >> (28 - index * 4)) & 15) as usize];
    }
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut nrf_config = embassy_nrf::config::Config::default();
    nrf_config.hfclk_source = embassy_nrf::config::HfclkSource::ExternalXtal;
    // Preserve debug protection as well as reset/NFC UICR configuration.
    nrf_config.debug = embassy_nrf::config::Debug::NotConfigured;
    let p = embassy_nrf::init(nrf_config);
    let driver = Driver::new(p.USBD, Irqs, HardwareVbusDetect::new(Irqs));
    let mut config = Config::new(0x4c4b, PID);
    config.manufacturer = Some("NocFree RMK community");
    config.product = Some(PRODUCT);
    config.serial_number = Some(SERIAL);
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
    let mut cdc = CdcAcmClass::new(&mut builder, &mut state, 64);
    let mut device = builder.build();
    let inspect = async {
        let mut packet = [0; 64];
        let mut command = [0; 32];
        loop {
            cdc.wait_connection().await;
            let mut used = 0;
            let mut overflow = false;
            'connected: loop {
                let Ok(count) = cdc.read_packet(&mut packet).await else {
                    break;
                };
                for &byte in &packet[..count] {
                    if byte != b'\n' {
                        if used < command.len() {
                            command[used] = byte;
                            used += 1;
                        } else {
                            overflow = true;
                        }
                        continue;
                    }
                    let valid = !overflow && &command[..used] == REQUEST;
                    used = 0;
                    overflow = false;
                    if !valid {
                        if cdc.write_packet(b"ERR REQUEST\n").await.is_err() {
                            break 'connected;
                        }
                        continue;
                    }
                    if cdc.write_packet(HEADER).await.is_err() {
                        break 'connected;
                    }
                    for (start, end) in SPANS {
                        for address in (start..end).step_by(4) {
                            // Fixed, aligned, readable flash/UICR only. No write path.
                            let word = unsafe { core::ptr::read_volatile(address as *const u32) };
                            let mut line = [0; 18];
                            hex(address, &mut line[..8]);
                            line[8] = b':';
                            hex(word, &mut line[9..17]);
                            line[17] = b'\n';
                            if cdc.write_packet(&line).await.is_err() {
                                break 'connected;
                            }
                        }
                    }
                    if cdc.write_packet(b"END BOOT_READ_V1\n").await.is_err() {
                        break 'connected;
                    }
                }
            }
        }
    };
    rmk::embassy_futures::join::join(device.run(), inspect).await;
}

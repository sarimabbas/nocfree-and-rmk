#![no_std]
#![no_main]
#[cfg(feature = "application-recovery-shim")]
mod startup_recovery;
#[cfg(feature = "usb-rescue-startup")]
mod usb_rescue;
#[cfg(feature = "usb-rescue-diagnostic")]
mod usb_rescue_trace;
#[cfg(all(
    feature = "usb-rescue-diagnostic",
    feature = "startup-recovery-diagnostic"
))]
compile_error!("Select only one startup manufacturer diagnostic");
#[cfg(not(any(feature = "left", feature = "right", feature = "receiver")))]
compile_error!("Select exactly one role: left, right, receiver");
#[cfg(any(
    all(feature = "left", feature = "right"),
    all(feature = "left", feature = "receiver"),
    all(feature = "right", feature = "receiver")
))]
compile_error!("Roles are mutually exclusive");
#[cfg(all(feature = "backlight-active-low", feature = "backlight-active-high"))]
compile_error!("Select only one backlight polarity");
#[cfg(all(
    feature = "backlight",
    not(feature = "receiver"),
    not(any(feature = "backlight-active-low", feature = "backlight-active-high"))
))]
compile_error!("Backlight output requires an explicit board polarity");
#[cfg(not(feature = "receiver"))]
mod battery;
#[cfg(feature = "left")]
mod keymap;
#[cfg(not(feature = "receiver"))]
mod scanner;
use defmt::unwrap;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::peripherals::{RNG, TWISPI0, USBD};
#[cfg(not(feature = "right"))]
use embassy_nrf::usb::{Driver, vbus_detect::HardwareVbusDetect};
use embassy_nrf::{bind_interrupts, rng, saadc, twim, usb};
use nrf_mpsl::Flash;
use nrf_sdc::{self as sdc, mpsl};
use panic_probe as _;
use rmk::{config::StorageConfig, run_all};
use static_cell::StaticCell;
bind_interrupts!(struct Irqs {
    USBD => usb::InterruptHandler<USBD>;
    SAADC => saadc::InterruptHandler;
    RNG => rng::InterruptHandler<RNG>;
    TWISPI0 => twim::InterruptHandler<TWISPI0>;
    EGU0_SWI0 => mpsl::LowPrioInterruptHandler;
    CLOCK_POWER => mpsl::ClockInterruptHandler, usb::vbus_detect::InterruptHandler;
    RADIO => mpsl::HighPrioInterruptHandler;
    TIMER0 => mpsl::HighPrioInterruptHandler;
    RTC0 => mpsl::HighPrioInterruptHandler;
});
#[embassy_executor::task]
async fn mpsl_task(mpsl: &'static mpsl::MultiprotocolServiceLayer<'static>) -> ! {
    mpsl.run().await
}
fn ble_addr() -> [u8; 6] {
    let ficr = embassy_nrf::pac::FICR;
    let addr = ((ficr.deviceaddr(1).read() as u64) << 32)
        | ficr.deviceaddr(0).read() as u64
        | 0xc000_0000_0000;
    unwrap!(addr.to_le_bytes()[..6].try_into())
}
#[embassy_executor::main]
async fn main(spawner: Spawner) {
    #[cfg(feature = "usb-rescue-diagnostic")]
    usb_rescue_trace::initialize(usb_rescue::diagnostic_snapshot());
    #[allow(unused_mut)]
    let mut p = embassy_nrf::init(embassy_nrf::config::Config::default());
    #[cfg(feature = "usb-rescue-startup")]
    usb_rescue::run(p.USBD.reborrow(), Irqs).await;
    let mpsl_p =
        mpsl::Peripherals::new(p.RTC0, p.TIMER0, p.TEMP, p.PPI_CH19, p.PPI_CH30, p.PPI_CH31);
    let lfclk = mpsl::raw::mpsl_clock_lfclk_cfg_t {
        source: mpsl::raw::MPSL_CLOCK_LF_SRC_RC as u8,
        rc_ctiv: mpsl::raw::MPSL_RECOMMENDED_RC_CTIV as u8,
        rc_temp_ctiv: mpsl::raw::MPSL_RECOMMENDED_RC_TEMP_CTIV as u8,
        accuracy_ppm: 500,
        skip_wait_lfclk_started: false,
    };
    static MPSL: StaticCell<mpsl::MultiprotocolServiceLayer> = StaticCell::new();
    static SESSION: StaticCell<mpsl::SessionMem<1>> = StaticCell::new();
    let mpsl = MPSL.init(unwrap!(mpsl::MultiprotocolServiceLayer::with_timeslots(
        mpsl_p,
        Irqs,
        lfclk,
        SESSION.init(mpsl::SessionMem::new())
    )));
    spawner.spawn(unwrap!(mpsl_task(mpsl)));
    let sdc_p = sdc::Peripherals::new(
        p.PPI_CH17, p.PPI_CH18, p.PPI_CH20, p.PPI_CH21, p.PPI_CH22, p.PPI_CH23, p.PPI_CH24,
        p.PPI_CH25, p.PPI_CH26, p.PPI_CH27, p.PPI_CH28, p.PPI_CH29,
    );
    let mut rng = rng::Rng::new(p.RNG, Irqs);
    let mut mem = sdc::Mem::<15472>::new();
    let builder = sdc::Builder::new().unwrap();
    #[cfg(feature = "left")]
    let builder = builder
        .support_scan()
        .support_central()
        .support_adv()
        .support_peripheral()
        .support_dle_central()
        .support_dle_peripheral()
        .support_phy_update_central()
        .support_phy_update_peripheral()
        .support_le_2m_phy()
        .central_count(1)
        .unwrap()
        .peripheral_count(1)
        .unwrap();
    #[cfg(feature = "right")]
    let builder = builder
        .support_adv()
        .support_peripheral()
        .support_dle_peripheral()
        .support_phy_update_peripheral()
        .support_le_2m_phy()
        .peripheral_count(1)
        .unwrap();
    #[cfg(feature = "receiver")]
    let builder = builder
        .support_scan()
        .support_central()
        .support_dle_central()
        .support_phy_update_central()
        .support_le_2m_phy()
        .central_count(1)
        .unwrap();
    let sdc = unwrap!(
        builder
            .buffer_cfg(251, 251, 3, 3)
            .unwrap()
            .build(sdc_p, &mut rng, mpsl, &mut mem)
    );
    let flash = Flash::take(mpsl, p.NVMC);
    let storage_config = StorageConfig {
        start_addr: 0x65000,
        num_sectors: 8,
        ..Default::default()
    };
    #[cfg(not(feature = "right"))]
    let driver = Driver::new(p.USBD, Irqs, HardwareVbusDetect::new(Irqs));
    #[cfg(not(feature = "right"))]
    let device_config = rmk::config::DeviceConfig {
        manufacturer: {
            #[cfg(feature = "usb-rescue-diagnostic")]
            {
                usb_rescue_trace::manufacturer()
            }
            #[cfg(all(
                feature = "startup-recovery-diagnostic",
                not(feature = "usb-rescue-diagnostic")
            ))]
            {
                startup_recovery::diagnostic_manufacturer()
            }
            #[cfg(not(any(
                feature = "startup-recovery-diagnostic",
                feature = "usb-rescue-diagnostic"
            )))]
            {
                "NocFree RMK community"
            }
        },
        product_name: if cfg!(feature = "receiver") {
            "NocFree AND RMK Receiver"
        } else {
            "NocFree RMK"
        },
        ..Default::default()
    };
    #[cfg(not(feature = "receiver"))]
    {
        #[cfg(feature = "backlight")]
        let backlight = {
            use embassy_nrf::{
                gpio::Level,
                pwm::{DutyCycle, Prescaler, SimpleConfig, SimplePwm},
            };
            let active_low = cfg!(feature = "backlight-active-low");
            // P0.20 is vendor-published. Polarity remains an explicit trial choice.
            // Factory applications request 400 Hz: 8 MHz / 20000, up counting.
            // See docs/research/backlight-pwm-diagnosis.md; physical output is unmeasured.
            let mut config = SimpleConfig::default();
            config.prescaler = Prescaler::Div2;
            config.max_duty = 20000;
            config.ch0_idle_level = if active_low { Level::High } else { Level::Low };
            static DUTIES: StaticCell<[DutyCycle; 4]> = StaticCell::new();
            let pwm = SimplePwm::new_1ch(p.PWM0, p.P0_20, &config)
                .with_static_duty_buffer(DUTIES.init([DutyCycle::normal(0); 4]));
            rmk::backlight::NrfPwm::new(pwm, active_low)
        };
        use embassy_nrf::gpio::{Level, Output, OutputDrive};
        use embassy_nrf::saadc::Input as _;
        use rmk::input_device::battery::BatteryProcessor;
        #[cfg(feature = "left")]
        let battery_enable = Output::new(p.P0_05, Level::Low, OutputDrive::Standard);
        #[cfg(feature = "right")]
        let battery_enable = Output::new(p.P0_31, Level::Low, OutputDrive::Standard);
        let adc = saadc::Saadc::new(
            p.SAADC,
            Irqs,
            saadc::Config::default(),
            [saadc::ChannelConfig::single_ended(p.P0_04.degrade_saadc())],
        );
        adc.calibrate().await;
        let mut battery_adc = battery::Battery::new(adc, battery_enable);
        // Factory conversion uses 130/100; capacity remains a voltage estimate.
        let mut battery = BatteryProcessor::new(100, 130);
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
        let mut matrix = scanner::Scanner::new(bus, &nocfree_input::LEFT_BITS);
        #[cfg(feature = "right")]
        let mut matrix = scanner::Scanner::new(bus, &nocfree_input::RIGHT_BITS);
        #[cfg(feature = "right")]
        {
            let mut storage = rmk::storage::new_storage_without_keymap(flash, storage_config).await;
            let keyboard_tasks = rmk::futures::future::join(
                run_all!(matrix, battery_adc, battery, storage),
                rmk::split::peripheral::run_rmk_split_peripheral(0, sdc, ble_addr()),
            );
            #[cfg(feature = "backlight")]
            rmk::futures::future::join(keyboard_tasks, rmk::backlight::run(backlight, false)).await;
            #[cfg(not(feature = "backlight"))]
            keyboard_tasks.await;
        }
        #[cfg(feature = "left")]
        {
            use rmk::{
                KeymapData,
                ble::BleTransport,
                config::{BehaviorConfig, PositionalConfig, RmkConfig},
                initialize_keymap_and_storage,
                keyboard::Keyboard,
                split::PeripheralMatrixConfig,
                usb::UsbTransport,
            };
            let config = RmkConfig {
                device_config,
                storage_config,
                ..Default::default()
            };
            let mut data = KeymapData::new(keymap::default_keymap());
            let mut behavior = BehaviorConfig::default();
            let positional = PositionalConfig::default();
            let (keymap, mut storage) = initialize_keymap_and_storage(
                &mut data,
                flash,
                &storage_config,
                &mut behavior,
                &positional,
            )
            .await;
            let mut keyboard = Keyboard::new(&keymap);

            let host_service = rmk::host::HostService::new(&keymap, &config);
            let mut usb = UsbTransport::new(driver, device_config).with_host_service(&host_service);
            let ble = BleTransport::new(
                sdc,
                ble_addr(),
                config,
                [PeripheralMatrixConfig {
                    rows: 1,
                    cols: 47,
                    row_offset: 0,
                    col_offset: 37,
                }],
            );
            let mut ble = ble.with_host_service(&host_service);
            let keyboard_tasks =
                run_all!(matrix, battery_adc, battery, keyboard, storage, usb, ble);
            #[cfg(feature = "backlight")]
            rmk::futures::future::join(keyboard_tasks, rmk::backlight::run(backlight, true)).await;
            #[cfg(not(feature = "backlight"))]
            keyboard_tasks.await;
        }
    }
    #[cfg(feature = "receiver")]
    {
        use rmk::{
            dongle::{Dongle, DongleRouter},
            usb::UsbTransport,
        };
        let mut storage = rmk::storage::new_storage_without_keymap(flash, storage_config).await;
        let router = DongleRouter::new();
        let mut dongle = Dongle::new(sdc, ble_addr(), &router);
        let mut usb = UsbTransport::new(driver, device_config).with_dongle_router(&router);
        run_all!(storage, dongle, usb).await;
    }
}

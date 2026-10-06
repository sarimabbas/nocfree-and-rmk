#![no_std]
#![no_main]
#[cfg(feature = "startup-watchdog")]
mod watchdog_recovery;
#[cfg(all(
    feature = "startup-watchdog",
    any(
        all(not(feature = "receiver"), not(feature = "reclaimed-softdevice")),
        all(feature = "right", not(feature = "runtime-recovery"))
    )
))]
compile_error!("Startup watchdog requires the production role layout");
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
#[cfg(all(feature = "status-led", not(feature = "left")))]
compile_error!("Status LED pin mapping is documented only for LEFT");
#[cfg(not(feature = "receiver"))]
mod battery;
#[cfg(feature = "backlight")]
mod board_backlight;
#[cfg(feature = "backlight")]
mod board_pwm;
#[cfg(feature = "status-led")]
mod board_status_led;
mod companion;
#[cfg(feature = "left")]
mod keymap;
#[cfg(feature = "left")]
mod mode_switch;
#[cfg(not(feature = "receiver"))]
mod scanner;
mod usb_adapter;
#[cfg(feature = "left")]
mod vial;
use defmt::unwrap;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::peripherals::{RNG, TWISPI0, USBD};
#[cfg(any(not(feature = "right"), feature = "runtime-recovery"))]
use embassy_nrf::usb::{Driver, vbus_detect::HardwareVbusDetect};
use embassy_nrf::{bind_interrupts, rng, saadc, twim, usb};
use nrf_mpsl::Flash;
use nrf_sdc::{self as sdc, mpsl};
use panic_probe as _;
use rmk::{config::StorageConfig, core_traits::Runnable, run_all};
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
    #[allow(unused_mut)]
    let mut hal_config = embassy_nrf::config::Config::default();
    hal_config.debug = embassy_nrf::config::Debug::NotConfigured;
    let p = embassy_nrf::init(hal_config);
    #[cfg(feature = "startup-watchdog")]
    let mut watchdog_runner = rmk::watchdog::Nrf52Watchdog::default_runner(p.WDT);
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
        layout_id: if cfg!(feature = "receiver") {
            None
        } else if cfg!(feature = "layout-iso") {
            Some(2)
        } else if cfg!(feature = "layout-jis") {
            Some(4)
        } else if cfg!(feature = "layout-kr") {
            Some(3)
        } else {
            None
        },
        ..Default::default()
    };
    #[cfg(any(not(feature = "right"), feature = "runtime-recovery"))]
    let driver =
        usb_adapter::Nrf52833Driver(Driver::new(p.USBD, Irqs, HardwareVbusDetect::new(Irqs)));
    #[cfg(any(not(feature = "right"), feature = "runtime-recovery"))]
    let device_config = rmk::config::DeviceConfig {
        manufacturer: concat!("NocFree RMK;fw=", env!("CARGO_PKG_VERSION")),
        product_name: if cfg!(feature = "receiver") {
            "NocFree RMK Receiver"
        } else if cfg!(feature = "right") {
            "NocFree RMK Right"
        } else {
            "NocFree RMK"
        },
        vid: 0x4c4b,
        pid: if cfg!(feature = "receiver") {
            0x4644
        } else if cfg!(feature = "right") {
            0x4671
        } else {
            0x4643
        },
        ..Default::default()
    };
    #[cfg(not(feature = "receiver"))]
    {
        #[cfg(feature = "backlight")]
        let backlight = board_pwm::new(p.PWM0, p.P0_20);
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
        #[cfg(feature = "status-led")]
        let mut status_led =
            if unsafe { core::ptr::read_volatile(0x1000120c as *const u32) } & 1 == 0 {
                Some(board_status_led::StatusLed::new(
                    Output::new(p.P0_10, Level::High, OutputDrive::Standard),
                    true,
                ))
            } else {
                defmt::warn!("Status LED unavailable: NFC owns pin");
                None
            };
        #[cfg(feature = "left")]
        let mut mode_switch = mode_switch::ModeSwitch::new(
            embassy_nrf::gpio::Input::new(p.P0_15, embassy_nrf::gpio::Pull::Up),
            embassy_nrf::gpio::Input::new(p.P0_17, embassy_nrf::gpio::Pull::Up),
        );
        // Provisional left calibration from the owner's 4.17 V observation;
        // this is an effective scale, not a measured resistor ratio or capacity.
        #[cfg(feature = "left")]
        let mut battery = BatteryProcessor::new(100, 150);
        // Provisional right scale follows the same factory normalization;
        // physical divider calibration and capacity remain unverified.
        #[cfg(feature = "right")]
        let mut battery = BatteryProcessor::new(100, 150);
        let mut twim_buffer = [0u8; 8];
        let bus = twim::Twim::new(
            p.TWISPI0,
            Irqs,
            p.P0_11,
            p.P1_09,
            twim::Config::default(),
            &mut twim_buffer,
        );
        #[cfg(all(feature = "left", not(feature = "async-scanner")))]
        let mut matrix = scanner::Scanner::new(bus, &nocfree_input::LEFT_BITS);
        #[cfg(all(feature = "right", not(feature = "async-scanner")))]
        let mut matrix = scanner::Scanner::new(bus, &nocfree_input::RIGHT_BITS);
        #[cfg(all(feature = "left", feature = "async-scanner"))]
        let mut matrix = scanner::Scanner::new_with_interrupt(
            bus,
            &nocfree_input::LEFT_BITS,
            embassy_nrf::gpio::Input::new(p.P0_31, embassy_nrf::gpio::Pull::Up),
        );
        #[cfg(all(feature = "right", feature = "async-scanner"))]
        let mut matrix = scanner::Scanner::new_with_interrupt(
            bus,
            &nocfree_input::RIGHT_BITS,
            embassy_nrf::gpio::Input::new(p.P0_05, embassy_nrf::gpio::Pull::Up),
        );
        #[cfg(feature = "right")]
        {
            let mut storage = rmk::storage::new_storage_without_keymap(flash, storage_config).await;
            #[cfg(feature = "runtime-recovery")]
            let mut recovery_builder = right_usb_builder(driver, device_config);
            #[cfg(feature = "runtime-recovery")]
            let mut companion_service =
                companion::attach(&mut recovery_builder, companion::Role::Right);
            #[cfg(feature = "runtime-recovery")]
            let mut recovery_usb = recovery_builder.build();
            #[cfg(feature = "runtime-recovery")]
            let local_tasks = run_all!(matrix, battery_adc, battery, storage, watchdog_runner);
            #[cfg(not(feature = "runtime-recovery"))]
            let local_tasks = run_all!(matrix, battery_adc, battery, storage);
            let keyboard_tasks = rmk::futures::future::join(
                local_tasks,
                rmk::split::peripheral::run_rmk_split_peripheral(0, sdc, ble_addr()),
            );
            #[cfg(feature = "runtime-recovery")]
            let keyboard_tasks = embassy_futures::join::join3(
                companion_service.run(),
                recovery_usb.run(),
                keyboard_tasks,
            );
            #[cfg(feature = "backlight")]
            embassy_futures::join::join3(
                board_backlight::run_right(),
                board_pwm::run(backlight),
                keyboard_tasks,
            )
            .await;
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
                vial_config: vial::config(),
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
            let mut usb_builder = UsbTransport::builder(driver, device_config);
            let mut companion_service =
                companion::attach(usb_builder.usb_builder(), companion::Role::Left);
            let mut usb = usb_builder.build().with_host_service(&host_service);
            let ble = BleTransport::new(
                sdc,
                ble_addr(),
                config,
                [PeripheralMatrixConfig {
                    rows: 1,
                    cols: nocfree_input::layout::RIGHT_COUNT as u8,
                    row_offset: 0,
                    col_offset: nocfree_input::layout::LEFT_COUNT as u8,
                }],
            );
            let mut ble = ble.with_host_service(&host_service);
            #[cfg(feature = "startup-watchdog")]
            let keyboard_tasks = run_all!(
                mode_switch,
                matrix,
                battery_adc,
                battery,
                keyboard,
                storage,
                usb,
                ble,
                watchdog_runner
            );
            #[cfg(not(feature = "startup-watchdog"))]
            let keyboard_tasks = run_all!(
                mode_switch,
                matrix,
                battery_adc,
                battery,
                keyboard,
                storage,
                usb,
                ble
            );
            #[cfg(feature = "status-led")]
            let keyboard_tasks = rmk::futures::future::join(keyboard_tasks, async {
                if let Some(led) = &mut status_led {
                    led.run().await;
                }
                core::future::pending::<()>().await
            });
            let keyboard_tasks =
                embassy_futures::join::join(companion_service.run(), keyboard_tasks);
            #[cfg(feature = "backlight")]
            embassy_futures::join::join3(
                board_backlight::run_left(),
                board_pwm::run(backlight),
                keyboard_tasks,
            )
            .await;
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
        let mut usb_builder = UsbTransport::builder(driver, device_config);
        let mut companion_service =
            companion::attach(usb_builder.usb_builder(), companion::Role::Receiver);
        let mut usb = usb_builder.build().with_dongle_router(&router);
        #[cfg(feature = "startup-watchdog")]
        run_all!(storage, dongle, usb, companion_service, watchdog_runner).await;
        #[cfg(not(feature = "startup-watchdog"))]
        run_all!(storage, dongle, usb, companion_service).await;
    }
}

#[cfg(all(feature = "right", feature = "runtime-recovery"))]
fn right_usb_builder<D: embassy_usb::driver::Driver<'static>>(
    driver: D,
    device: rmk::config::DeviceConfig<'static>,
) -> embassy_usb::Builder<'static, D> {
    static CONFIG: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS: StaticCell<[u8; 32]> = StaticCell::new();
    static MSOS: StaticCell<[u8; 32]> = StaticCell::new();
    static CONTROL: StaticCell<[u8; 64]> = StaticCell::new();
    let mut config = embassy_usb::Config::new(device.vid, device.pid);
    config.manufacturer = Some(device.manufacturer);
    config.product = Some(device.product_name);
    config.serial_number = Some(right_serial_number());
    config.max_packet_size_0 = 64;
    config.max_power = 450;
    embassy_usb::Builder::new(
        driver,
        config,
        CONFIG.init([0; 256]),
        BOS.init([0; 32]),
        MSOS.init([0; 32]),
        CONTROL.init([0; 64]),
    )
}

#[cfg(all(feature = "right", feature = "runtime-recovery"))]
fn right_serial_number() -> &'static str {
    static SERIAL: StaticCell<[u8; 6]> = StaticCell::new();
    let ficr = embassy_nrf::pac::FICR;
    let id = (u64::from(ficr.deviceid(1).read()) << 32) | u64::from(ficr.deviceid(0).read());
    let bytes = SERIAL.init(core::array::from_fn(|index| {
        b"0123456789abcdef"[((id >> (60 - index * 4)) & 15) as usize]
    }));
    core::str::from_utf8(bytes).unwrap()
}

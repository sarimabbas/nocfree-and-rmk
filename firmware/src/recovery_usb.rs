//! RIGHT's USB exposes only Embassy's standard runtime recovery interface.
//! Key input and the split connection remain RMK's responsibility.
use embassy_time::Duration;
use embassy_usb::class::dfu::app_mode::{self, DfuState};
use embassy_usb::class::dfu::consts::DfuAttributes;
use embassy_usb::driver::Driver;
use embassy_usb::{Builder, Config, UsbDevice, msos};
use static_cell::StaticCell;

struct Recovery;
impl app_mode::Handler for Recovery {
    fn enter_dfu(&mut self) {
        rmk::boot::jump_to_bootloader();
    }
}

pub fn new<D: Driver<'static>>(
    driver: D,
    device: rmk::config::DeviceConfig<'static>,
) -> UsbDevice<'static, D> {
    static CONFIG: StaticCell<[u8; 128]> = StaticCell::new();
    static BOS: StaticCell<[u8; 64]> = StaticCell::new();
    static MSOS: StaticCell<[u8; 128]> = StaticCell::new();
    static CONTROL: StaticCell<[u8; 64]> = StaticCell::new();
    static STATE: StaticCell<DfuState<Recovery>> = StaticCell::new();
    let mut config = Config::new(device.vid, device.pid);
    config.manufacturer = Some(device.manufacturer);
    config.product = Some(device.product_name);
    config.serial_number = Some(device.serial_number);
    let mut builder = Builder::new(
        driver,
        config,
        CONFIG.init([0; 128]),
        BOS.init([0; 64]),
        MSOS.init([0; 128]),
        CONTROL.init([0; 64]),
    );
    builder.msos_descriptor(msos::windows_version::WIN8_1, 0x52);
    let state = STATE.init(DfuState::new(
        Recovery,
        DfuAttributes::CAN_DOWNLOAD | DfuAttributes::WILL_DETACH,
        Duration::from_millis(1000),
    ));
    app_mode::usb_dfu(&mut builder, state, |function| {
        function.msos_feature(msos::CompatibleIdFeatureDescriptor::new("WINUSB", ""));
    });
    builder.build()
}

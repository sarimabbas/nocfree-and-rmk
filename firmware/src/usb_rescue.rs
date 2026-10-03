//! Experimental USB stage after HAL initialization, before MPSL/RMK.
//! A cable connection alone does not restart a battery-powered application.
use embassy_nrf::{Peri, interrupt, pac, peripherals::USBD, usb};
use embassy_time::{Duration, Timer};
use embassy_usb::class::dfu::app_mode::{DfuState, Handler, usb_dfu};
use embassy_usb::class::dfu::consts::DfuAttributes;
use embassy_usb::{Builder, Config, msos};
#[path = "usb_rescue_scope.rs"]
mod scope;
use scope::RequestScope;

// MPSL has not started yet. Preserve any preceding stage's outstanding request;
// release only the HFXO request made by this stage, including timeout paths.
struct StageClock(bool);
impl StageClock {
    async fn acquire() -> Option<Self> {
        let clock = Self(!pac::CLOCK.hfclkrun().read().status());
        if clock.0 {
            pac::CLOCK.events_hfclkstarted().write_value(0);
            pac::CLOCK.tasks_hfclkstart().write_value(1);
        }
        for _ in 0..100 {
            let state = pac::CLOCK.hfclkstat().read();
            if state.state() && state.src().to_bits() == 1 {
                return Some(clock);
            }
            Timer::after_millis(1).await;
        }
        None
    }
}
impl Drop for StageClock {
    fn drop(&mut self) {
        if self.0 {
            pac::CLOCK.tasks_hfclkstop().write_value(1);
            pac::CLOCK.events_hfclkstarted().write_value(0);
        }
    }
}

struct PollingVbus;
impl usb::vbus_detect::VbusDetect for PollingVbus {
    fn is_usb_detected(&self) -> bool {
        pac::POWER.usbregstatus().read().vbusdetect()
    }
    async fn wait_power_ready(&mut self) -> Result<(), ()> {
        loop {
            let status = pac::POWER.usbregstatus().read();
            if !status.vbusdetect() {
                return Err(());
            }
            if status.outputrdy() {
                return Ok(());
            }
            Timer::after_millis(1).await;
        }
    }
}
struct FactoryRecovery;
impl Handler for FactoryRecovery {
    fn enter_dfu(&mut self) {
        pac::POWER
            .gpregret()
            .write_value(pac::power::regs::Gpregret(0x57));
        cortex_m::asm::dsb();
        cortex_m::peripheral::SCB::sys_reset();
    }
}

pub async fn run(
    peripheral: Peri<'_, USBD>,
    irq: impl interrupt::typelevel::Binding<interrupt::typelevel::USBD, usb::InterruptHandler<USBD>>
    + 'static,
) {
    if !pac::POWER.usbregstatus().read().vbusdetect() {
        return;
    }
    let Some(clock) = StageClock::acquire().await else {
        return;
    };
    let driver = usb::Driver::new(peripheral, irq, PollingVbus);
    let pid = if cfg!(feature = "left") {
        0x4660
    } else if cfg!(feature = "right") {
        0x4661
    } else {
        0x4662
    };
    let mut config = Config::new(0x4c4b, pid);
    config.product = Some("NocFree USB rescue EXPERIMENT");
    config.manufacturer = Some("RMK community");
    config.max_packet_size_0 = 64;
    let mut descriptor = [0; 128];
    let mut bos = [0; 64];
    let mut msos_buffer = [0; 128];
    let mut control = [0; 64];
    let mut state = DfuState::new(
        FactoryRecovery,
        DfuAttributes::WILL_DETACH,
        Duration::from_millis(1000),
    );
    let mut request_scope = RequestScope;
    let mut builder = Builder::new(
        driver,
        config,
        &mut descriptor,
        &mut bos,
        &mut msos_buffer,
        &mut control,
    );
    builder.handler(&mut request_scope);
    builder.msos_descriptor(msos::windows_version::WIN8_1, 0x20);
    usb_dfu(&mut builder, &mut state, |function| {
        function.msos_feature(msos::CompatibleIdFeatureDescriptor::new("WINUSB", ""));
    });
    let mut device = builder.build();
    let _ = rmk::embassy_futures::select::select(device.run(), Timer::after_secs(2)).await;
    // nRF USB driver DMA functions synchronously wait for END before yielding;
    // after cancellation no transfer future owns an in-flight stack buffer.
    device.disable().await;
    pac::USBD.usbpullup().write(|w| w.set_connect(false));
    pac::USBD
        .intenclr()
        .write_value(pac::usbd::regs::Int(u32::MAX));
    cortex_m::peripheral::NVIC::mask(pac::Interrupt::USBD);
    pac::USBD.shorts().write_value(pac::usbd::regs::Shorts(0));
    pac::USBD.epinen().write_value(pac::usbd::regs::Epinen(0));
    pac::USBD.epouten().write_value(pac::usbd::regs::Epouten(0));
    for event in [
        pac::USBD.events_usbreset(),
        pac::USBD.events_started(),
        pac::USBD.events_ep0datadone(),
        pac::USBD.events_endisoin(),
        pac::USBD.events_endisoout(),
        pac::USBD.events_sof(),
        pac::USBD.events_usbevent(),
        pac::USBD.events_ep0setup(),
        pac::USBD.events_epdata(),
    ] {
        event.write_value(0);
    }
    for index in 0..8 {
        pac::USBD.events_endepin(index).write_value(0);
        pac::USBD.events_endepout(index).write_value(0);
    }
    pac::USBD
        .eventcause()
        .write_value(pac::USBD.eventcause().read());
    pac::USBD
        .epdatastatus()
        .write_value(pac::USBD.epdatastatus().read());
    cortex_m::peripheral::NVIC::unpend(pac::Interrupt::USBD);
    drop(device);
    drop(clock);
    // Give the host a visible disconnect before RMK's production descriptors.
    Timer::after_millis(25).await;
}

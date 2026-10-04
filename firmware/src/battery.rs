use embassy_nrf::{gpio::Output, saadc::Saadc};
use embassy_time::Timer;
use rmk::{
    core_traits::Runnable,
    event::{BatteryAdcEvent, publish_event_async},
};
/// Powers the factory divider only around each ADC measurement.
pub struct Battery<'a> {
    adc: Saadc<'a, 1>,
    enable: Output<'a>,
}
impl<'a> Battery<'a> {
    pub fn new(adc: Saadc<'a, 1>, mut enable: Output<'a>) -> Self {
        enable.set_low();
        Self { adc, enable }
    }
}
struct EnabledDivider<'a, 'd>(&'a mut Output<'d>);
impl Drop for EnabledDivider<'_, '_> {
    fn drop(&mut self) {
        self.0.set_low();
    }
}
impl Runnable for Battery<'_> {
    async fn run(&mut self) -> ! {
        #[cfg(feature = "battery-adc-diagnostic")]
        let mut sequence = 0u32;
        loop {
            self.enable.set_high();
            let divider = EnabledDivider(&mut self.enable);
            Timer::after_millis(10).await;
            let mut sample = [0];
            self.adc.sample(&mut sample).await;
            drop(divider);
            #[cfg(feature = "left")]
            {
                let adc = embassy_nrf::pac::SAADC;
                rmk::input_device::battery::record_adc_diagnostic(
                    sample[0],
                    [
                        adc.resolution().read().0,
                        adc.oversample().read().0,
                        adc.ch(0).config().read().0,
                        adc.ch(0).pselp().read().0,
                    ],
                    crate::mode_switch::levels(),
                );
            }
            #[cfg(feature = "battery-adc-diagnostic")]
            {
                sequence = sequence.wrapping_add(1);
                let adc = embassy_nrf::pac::SAADC;
                // Error priority keeps the optional CDC diagnostic within flash limits;
                // this tagged sample is evidence, not an ADC failure.
                log::error!(
                    "battery_adc seq={} signed={} resolution={:#x} oversample={:#x} channel_config={:#x} pselp={:#x}",
                    sequence,
                    sample[0],
                    adc.resolution().read().0,
                    adc.oversample().read().0,
                    adc.ch(0).config().read().0,
                    adc.ch(0).pselp().read().0
                );
            }
            publish_event_async(BatteryAdcEvent(sample[0].max(0) as u16)).await;
            Timer::after_secs(30).await;
        }
    }
}

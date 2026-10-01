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
        loop {
            self.enable.set_high();
            let divider = EnabledDivider(&mut self.enable);
            Timer::after_millis(10).await;
            let mut sample = [0];
            self.adc.sample(&mut sample).await;
            drop(divider);
            publish_event_async(BatteryAdcEvent(sample[0].max(0) as u16)).await;
            Timer::after_secs(30).await;
        }
    }
}

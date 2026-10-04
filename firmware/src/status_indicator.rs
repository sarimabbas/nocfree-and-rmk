//! LEFT shared charge/status output for the opt-in optical mixing trial.

use core::convert::Infallible;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::{Peri, pac, peripherals::P0_09};
use embedded_hal::digital::{ErrorType, OutputPin};

pub struct RedIndicator<'d>(Output<'d>);

impl<'d> RedIndicator<'d> {
    pub fn new(pin: Peri<'d, P0_09>) -> Self {
        // Logical high disconnects the output transistor; it never drives high.
        Self(Output::new(
            pin,
            Level::High,
            OutputDrive::Standard0Disconnect1,
        ))
    }
}

impl ErrorType for RedIndicator<'_> {
    type Error = Infallible;
}

impl OutputPin for RedIndicator<'_> {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        // Physical VBUS is authoritative, even before USB enumeration/events.
        if pac::POWER.usbregstatus().read().vbusdetect() {
            self.0.set_high();
        } else {
            self.0.set_low();
        }
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.0.set_high();
        Ok(())
    }
}

use embassy_usb_driver::{
    ControlPipe, Driver, EndpointAddress, EndpointAllocError, EndpointError, EndpointType,
};

// Use only with nRF52833: hardware completes SET_ADDRESS and its status stage.
pub struct Nrf52833Driver<D>(pub D);
pub struct HardwareAddressPipe<P>(pub P);
impl<'a, D: Driver<'a>> Driver<'a> for Nrf52833Driver<D> {
    type EndpointOut = D::EndpointOut;
    type EndpointIn = D::EndpointIn;
    type ControlPipe = HardwareAddressPipe<D::ControlPipe>;
    type Bus = D::Bus;
    fn alloc_endpoint_out(
        &mut self,
        typ: EndpointType,
        addr: Option<EndpointAddress>,
        size: u16,
        interval: u8,
    ) -> Result<Self::EndpointOut, EndpointAllocError> {
        self.0.alloc_endpoint_out(typ, addr, size, interval)
    }
    fn alloc_endpoint_in(
        &mut self,
        typ: EndpointType,
        addr: Option<EndpointAddress>,
        size: u16,
        interval: u8,
    ) -> Result<Self::EndpointIn, EndpointAllocError> {
        self.0.alloc_endpoint_in(typ, addr, size, interval)
    }
    fn start(self, size: u16) -> (Self::Bus, Self::ControlPipe) {
        let (bus, pipe) = self.0.start(size);
        (bus, HardwareAddressPipe(pipe))
    }
}
impl<P: ControlPipe> ControlPipe for HardwareAddressPipe<P> {
    fn max_packet_size(&self) -> usize {
        self.0.max_packet_size()
    }
    async fn setup(&mut self) -> [u8; 8] {
        self.0.setup().await
    }
    async fn data_out(
        &mut self,
        buf: &mut [u8],
        first: bool,
        last: bool,
    ) -> Result<usize, EndpointError> {
        self.0.data_out(buf, first, last).await
    }
    async fn data_in(&mut self, data: &[u8], first: bool, last: bool) -> Result<(), EndpointError> {
        self.0.data_in(data, first, last).await
    }
    async fn accept(&mut self) {
        self.0.accept().await
    }
    async fn reject(&mut self) {
        self.0.reject().await
    }
    async fn accept_set_address(&mut self, _addr: u8) {}
}
#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    #[derive(Default)]
    struct Probe {
        accept: usize,
        reject: usize,
        address: usize,
        incoming: usize,
        outgoing: usize,
    }
    impl ControlPipe for Probe {
        fn max_packet_size(&self) -> usize {
            64
        }
        async fn setup(&mut self) -> [u8; 8] {
            [3; 8]
        }
        async fn data_out(
            &mut self,
            buf: &mut [u8],
            first: bool,
            last: bool,
        ) -> Result<usize, EndpointError> {
            assert!(first && last);
            self.outgoing += 1;
            buf[0] = 42;
            Ok(1)
        }
        async fn data_in(
            &mut self,
            data: &[u8],
            first: bool,
            last: bool,
        ) -> Result<(), EndpointError> {
            assert_eq!(data, [7]);
            assert!(first && last);
            self.incoming += 1;
            Err(EndpointError::Disabled)
        }
        async fn accept(&mut self) {
            self.accept += 1;
        }
        async fn reject(&mut self) {
            self.reject += 1;
        }
        async fn accept_set_address(&mut self, _: u8) {
            self.address += 1;
        }
    }
    #[test]
    fn hardware_address_never_triggers_software_status_but_other_controls_delegate() {
        block_on(async {
            let mut pipe = HardwareAddressPipe(Probe::default());
            for addr in [0, 1, 127] {
                pipe.accept_set_address(addr).await;
            }
            assert_eq!((pipe.0.accept, pipe.0.address), (0, 0));
            assert_eq!(pipe.max_packet_size(), 64);
            assert_eq!(pipe.setup().await, [3; 8]);
            let mut buf = [0; 8];
            assert_eq!(pipe.data_out(&mut buf, true, true).await, Ok(1));
            assert_eq!(buf[0], 42);
            assert_eq!(
                pipe.data_in(&[7], true, true).await,
                Err(EndpointError::Disabled)
            );
            pipe.accept().await;
            pipe.reject().await;
            assert_eq!(
                (
                    pipe.0.accept,
                    pipe.0.reject,
                    pipe.0.incoming,
                    pipe.0.outgoing
                ),
                (1, 1, 1, 1)
            );
        });
    }
}

#![no_std]

use embedded_hal_async::i2c::I2c;

/// Six independent input ports, packed in factory expander order.
/// A complete snapshot is returned only if every expander read succeeds.
pub struct Inputs<I> {
    bus: I,
    extra_port: bool,
}
pub const ADDRESSES: [u8; 3] = [0x20, 0x22, 0x24];
pub mod layout;
pub use layout::{LEFT_BITS, RIGHT_BITS};

impl<I: I2c> Inputs<I> {
    pub fn new(bus: I) -> Self {
        Self {
            bus,
            extra_port: false,
        }
    }
    /// KR right has four additional inputs on expander 0x21 port 0.
    pub fn with_extra_port(bus: I) -> Self {
        Self {
            bus,
            extra_port: true,
        }
    }
    pub async fn initialize(&mut self) -> Result<(), I::Error> {
        for address in ADDRESSES {
            // No output pins. Explicitly clear polarity inversion after a warm reset.
            self.bus.write(address, &[6, 0xff, 0xff]).await?;
            self.bus.write(address, &[4, 0, 0]).await?;
        }
        if self.extra_port {
            self.bus.write(0x21, &[6, 0xff, 0xff]).await?;
            self.bus.write(0x21, &[4, 0, 0]).await?;
        }
        Ok(())
    }
    pub async fn snapshot(&mut self) -> Result<u64, I::Error> {
        self.read_snapshot(false).await
    }
    /// PCA9555 INT erratum: leave each command pointer away from input port 0
    /// before another slave's read ACK can clear this device's interrupt.
    /// Register 2 is selected only; no output register data is written.
    pub async fn snapshot_for_interrupt(&mut self) -> Result<u64, I::Error> {
        self.read_snapshot(true).await
    }
    async fn read_snapshot(&mut self, park_pointer: bool) -> Result<u64, I::Error> {
        let mut pressed = 0;
        for (index, address) in ADDRESSES.into_iter().enumerate() {
            let mut ports = [0; 2];
            self.bus.write_read(address, &[0], &mut ports).await?;
            if park_pointer {
                self.bus.write(address, &[2]).await?;
            }
            pressed |= ((!u16::from_le_bytes(ports)) as u64) << (16 * index);
        }
        if self.extra_port {
            let mut port = [0];
            self.bus.write_read(0x21, &[0], &mut port).await?;
            if park_pointer {
                self.bus.write(0x21, &[2]).await?;
            }
            pressed |= ((!port[0]) as u64) << 48;
        }
        Ok(pressed)
    }
}

pub fn key_pressed(snapshot: u64, bit: u8) -> bool {
    snapshot & (1 << bit) != 0
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use embedded_hal::i2c::{ErrorKind, ErrorType};
    use embedded_hal_async::i2c::Operation;
    use futures::executor::block_on;
    use std::vec::Vec;
    struct Bus {
        ports: [u16; 3],
        extra: u8,
        fail: Option<u8>,
        reads: usize,
        writes: Vec<(u8, Vec<u8>)>,
    }
    impl ErrorType for Bus {
        type Error = ErrorKind;
    }
    impl I2c for Bus {
        async fn transaction(
            &mut self,
            address: u8,
            operations: &mut [Operation<'_>],
        ) -> Result<(), ErrorKind> {
            if self.fail == Some(address) {
                return Err(ErrorKind::Other);
            }
            match operations {
                [Operation::Write(bytes)] => self.writes.push((address, bytes.to_vec())),
                [Operation::Write([0]), Operation::Read(buf)] if address == 0x21 => {
                    assert_eq!(buf.len(), 1);
                    buf[0] = self.extra;
                    self.reads += 1;
                }
                [Operation::Write([0]), Operation::Read(buf)] => {
                    assert_eq!(buf.len(), 2);
                    let index = ADDRESSES.iter().position(|&a| a == address).unwrap();
                    buf.copy_from_slice(&self.ports[index].to_le_bytes());
                    self.reads += 1;
                }
                _ => panic!("unexpected bus transaction"),
            }
            Ok(())
        }
    }
    fn bus(ports: [u16; 3]) -> Bus {
        Bus {
            ports,
            extra: 0,
            fail: None,
            reads: 0,
            writes: Vec::new(),
        }
    }
    #[test]
    fn initializes_only_inputs_without_output_drive() {
        let mut inputs = Inputs::new(bus([0xffff; 3]));
        block_on(inputs.initialize()).unwrap();
        let expected: Vec<_> = ADDRESSES
            .into_iter()
            .flat_map(|a| [(a, std::vec![6, 255, 255]), (a, std::vec![4, 0, 0])])
            .collect();
        assert_eq!(inputs.bus.writes, expected);
    }
    #[test]
    fn active_low_reads_all_six_ports_in_order() {
        let mut inputs = Inputs::new(bus([0xfffe, 0x7fff, 0xfeff]));
        let snapshot = block_on(inputs.snapshot()).unwrap();
        assert_eq!(snapshot, (1 << 0) | (1 << 31) | (1 << 40));
        assert_eq!(inputs.bus.reads, 3);
    }
    #[test]
    fn all_keys_can_be_pressed_simultaneously() {
        let mut inputs = Inputs::with_extra_port(bus([0; 3]));
        let snapshot = block_on(inputs.snapshot()).unwrap();
        assert!(
            LEFT_BITS
                .iter()
                .chain(RIGHT_BITS.iter())
                .all(|&b| key_pressed(snapshot, b))
        );
    }
    #[test]
    fn failure_never_returns_partial_or_fabricated_snapshot() {
        let mut b = bus([0; 3]);
        b.fail = Some(0x22);
        let mut inputs = Inputs::new(b);
        assert_eq!(block_on(inputs.snapshot()), Err(ErrorKind::Other));
        inputs.bus.fail = None;
        assert_eq!(block_on(inputs.snapshot()).unwrap(), 0xffff_ffff_ffff);
    }
    #[test]
    fn every_selected_input_produces_one_press_then_releases() {
        for &bit in LEFT_BITS.iter().chain(RIGHT_BITS.iter()) {
            let mut b = bus([0xffff; 3]);
            b.extra = 0xff;
            if bit < 48 {
                b.ports[bit as usize / 16] &= !(1 << (bit % 16));
            } else {
                b.extra &= !(1 << (bit - 48));
            }
            let mut inputs = if bit < 48 {
                Inputs::new(b)
            } else {
                Inputs::with_extra_port(b)
            };
            assert_eq!(block_on(inputs.snapshot()), Ok(1_u64 << bit));
            inputs.bus.ports = [0xffff; 3];
            inputs.bus.extra = 0xff;
            assert_eq!(block_on(inputs.snapshot()), Ok(0));
        }
    }
    #[test]
    fn kr_extra_port_is_read_only_for_the_right_scanner() {
        let mut inputs = Inputs::with_extra_port(bus([0xffff; 3]));
        block_on(inputs.initialize()).unwrap();
        assert_eq!(
            &inputs.bus.writes[6..],
            &[(0x21, std::vec![6, 255, 255]), (0x21, std::vec![4, 0, 0])]
        );
        inputs.bus.extra = 0xf0;
        assert_eq!(block_on(inputs.snapshot_for_interrupt()), Ok(0xf_u64 << 48));
        assert_eq!(inputs.bus.writes.last(), Some(&(0x21, std::vec![2])));
        inputs.bus.fail = Some(0x21);
        assert_eq!(block_on(inputs.snapshot()), Err(ErrorKind::Other));
        inputs.bus.fail = None;
        inputs.bus.extra = 0xff;
        assert_eq!(block_on(inputs.snapshot()), Ok(0));
    }
    #[test]
    fn physical_layout_positions_keep_their_host_usages() {
        #[cfg(feature = "layout-iso")]
        {
            assert_eq!(layout::USAGES[27], 0x64); // Extra key beside left Shift.
            assert_eq!((layout::LEFT_FN, layout::RIGHT_FN), (33, 80));
        }
        #[cfg(feature = "layout-jis")]
        {
            assert_eq!(layout::USAGES[51], 0x89); // Yen.
            assert_eq!(layout::USAGES[74], 0x87); // Ro.
            assert_eq!(layout::USAGES[78], 0x8a); // Convert.
            assert_eq!((layout::LEFT_FN, layout::RIGHT_FN), (35, 80));
        }
        #[cfg(feature = "layout-kr")]
        {
            assert_eq!(&RIGHT_BITS[46..], &[48, 49, 50, 51]);
            assert_eq!(&layout::USAGES[85..], &[0x4a, 0x4c, 0x4b, 0x4e]);
            assert_eq!((layout::LEFT_FN, layout::RIGHT_FN), (34, 80));
        }
        #[cfg(not(any(feature = "layout-iso", feature = "layout-jis", feature = "layout-kr")))]
        {
            assert_eq!(layout::USAGES[37], 0x40);
            assert_eq!(layout::USAGES[48], 0x27);
            assert_eq!((layout::LEFT_FN, layout::RIGHT_FN), (32, 79));
        }
    }
    #[test]
    fn selected_mapping_has_no_duplicate_or_out_of_range_bits() {
        for bits in [&LEFT_BITS[..], &RIGHT_BITS[..]] {
            for (i, &bit) in bits.iter().enumerate() {
                assert!(bit < 56);
                assert!(!bits[..i].contains(&bit));
            }
        }
        assert_eq!(layout::USAGES[layout::LEFT_SHIFT], 0xe1);
        assert_eq!(layout::USAGES[layout::LEFT_FN], 0);
        assert_eq!(layout::USAGES[layout::RIGHT_FN], 0);
        let mut sorted = layout::VISUAL_TO_RAW;
        sorted.sort_unstable();
        assert!(sorted.iter().enumerate().all(|(i, &raw)| raw == i));
    }
}

/// Vendor-published active-low position inputs. Both asserted is an invalid/transient state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchPosition {
    Wired,
    Bluetooth,
    Receiver,
}
pub fn switch_position(bluetooth_high: bool, receiver_high: bool) -> Option<SwitchPosition> {
    match (bluetooth_high, receiver_high) {
        (true, true) => Some(SwitchPosition::Wired),
        (false, true) => Some(SwitchPosition::Bluetooth),
        (true, false) => Some(SwitchPosition::Receiver),
        (false, false) => None,
    }
}
#[cfg(test)]
mod switch_tests {
    use super::*;
    #[test]
    fn selector_truth_table_rejects_overlapping_contacts() {
        assert_eq!(switch_position(true, true), Some(SwitchPosition::Wired));
        assert_eq!(
            switch_position(false, true),
            Some(SwitchPosition::Bluetooth)
        );
        assert_eq!(switch_position(true, false), Some(SwitchPosition::Receiver));
        assert_eq!(switch_position(false, false), None);
    }
}

/// Board-scanner idle arming. Key state and pending debounce remain RMK-owned.
#[derive(Default)]
pub struct IdleGate {
    scan_quiet: bool,
}
impl IdleGate {
    pub fn begin_scan(&mut self, complete: bool) {
        self.scan_quiet = complete;
    }
    pub fn observe_key(
        &mut self,
        raw_pressed: bool,
        registered_pressed: bool,
        debounce_pending: bool,
    ) {
        self.scan_quiet &= !raw_pressed && !registered_pressed && !debounce_pending;
    }
    /// A complete released scan with no pending debounce can arm the interrupt.
    pub fn can_wait(&self) -> bool {
        self.scan_quiet
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdleWake {
    AlreadyLow,
    Signalled,
}

/// Back off only repeated quiet/low IRQs, never a first press during arming.
#[derive(Default)]
pub struct IdleRetry {
    previous_low: bool,
}
impl IdleRetry {
    pub fn active(&mut self) {
        self.previous_low = false;
    }
    pub fn back_off<E>(&mut self, wake: Result<IdleWake, E>) -> bool {
        match wake {
            Ok(IdleWake::AlreadyLow) => {
                let repeated = self.previous_low;
                self.previous_low = true;
                repeated
            }
            Ok(IdleWake::Signalled) => {
                self.previous_low = false;
                false
            }
            Err(_) => {
                self.previous_low = false;
                true
            }
        }
    }
}

/// Level-sensitive arming catches a press between the final scan and awaiting.
/// An already-low (possibly stuck or unused-bit) interrupt is retried by the
/// scanner after a bounded delay rather than repeatedly completing a busy loop.
pub async fn wait_for_interrupt<
    P: embedded_hal::digital::InputPin + embedded_hal_async::digital::Wait,
>(
    pin: &mut P,
) -> Result<IdleWake, P::Error> {
    if pin.is_low()? {
        return Ok(IdleWake::AlreadyLow);
    }
    pin.wait_for_low().await?;
    Ok(IdleWake::Signalled)
}

#[cfg(test)]
mod idle_tests {
    extern crate std;
    use super::*;
    use core::future::Future;
    use core::task::{Context, Poll};
    use embedded_hal::digital::{ErrorKind, ErrorType, InputPin};
    use embedded_hal_async::digital::Wait;
    use futures::executor::block_on;

    #[test]
    fn released_complete_scan_can_arm_immediately() {
        let mut gate = IdleGate::default();
        gate.begin_scan(true);
        for _ in 0..47 {
            gate.observe_key(false, false, false);
        }
        assert!(gate.can_wait());
    }
    #[test]
    fn raw_press_never_waits_before_debounce_commits() {
        let mut gate = IdleGate::default();
        gate.begin_scan(true);
        gate.observe_key(true, false, false);
        assert!(!gate.can_wait());
    }
    #[test]
    fn release_keeps_scanning_until_registered_release_commits() {
        let mut gate = IdleGate::default();
        gate.begin_scan(true);
        gate.observe_key(false, true, true);
        assert!(!gate.can_wait());
        gate.begin_scan(true);
        gate.observe_key(false, false, false);
        assert!(gate.can_wait());
    }
    #[test]
    fn any_pending_debounce_prevents_wait_even_without_pressed_state() {
        let mut gate = IdleGate::default();
        gate.begin_scan(true);
        gate.observe_key(false, false, true);
        assert!(!gate.can_wait());
    }
    #[test]
    fn simultaneous_inputs_cannot_be_overwritten_by_later_quiet_keys() {
        let mut gate = IdleGate::default();
        gate.begin_scan(true);
        gate.observe_key(true, true, false);
        gate.observe_key(true, false, true);
        for _ in 0..45 {
            gate.observe_key(false, false, false);
        }
        assert!(!gate.can_wait());
    }
    #[test]
    fn failed_scan_never_arms_or_fabricates_a_release() {
        let mut gate = IdleGate::default();
        gate.begin_scan(true);
        assert!(gate.can_wait());
        gate.begin_scan(false);
        gate.observe_key(false, false, false);
        assert!(!gate.can_wait());
    }
    struct Pin {
        low: bool,
        press_on_arm: bool,
        fail: bool,
        waits: usize,
    }
    impl ErrorType for Pin {
        type Error = ErrorKind;
    }
    impl InputPin for Pin {
        fn is_high(&mut self) -> Result<bool, Self::Error> {
            self.is_low().map(|low| !low)
        }
        fn is_low(&mut self) -> Result<bool, Self::Error> {
            if self.fail {
                Err(ErrorKind::Other)
            } else {
                Ok(self.low)
            }
        }
    }
    impl Wait for Pin {
        async fn wait_for_low(&mut self) -> Result<(), Self::Error> {
            self.waits += 1;
            if self.press_on_arm {
                self.low = true;
            }
            if self.low {
                Ok(())
            } else {
                core::future::pending().await
            }
        }
        async fn wait_for_high(&mut self) -> Result<(), Self::Error> {
            core::future::pending().await
        }
        async fn wait_for_rising_edge(&mut self) -> Result<(), Self::Error> {
            core::future::pending().await
        }
        async fn wait_for_falling_edge(&mut self) -> Result<(), Self::Error> {
            core::future::pending().await
        }
        async fn wait_for_any_edge(&mut self) -> Result<(), Self::Error> {
            core::future::pending().await
        }
    }
    fn pin() -> Pin {
        Pin {
            low: false,
            press_on_arm: false,
            fail: false,
            waits: 0,
        }
    }
    #[test]
    fn press_between_snapshot_and_level_check_is_not_lost() {
        let mut pin = pin();
        pin.low = true;
        assert_eq!(
            block_on(wait_for_interrupt(&mut pin)),
            Ok(IdleWake::AlreadyLow)
        );
        assert_eq!(pin.waits, 0);
    }
    #[test]
    fn press_between_level_check_and_arm_is_not_lost() {
        let mut pin = pin();
        pin.press_on_arm = true;
        assert_eq!(
            block_on(wait_for_interrupt(&mut pin)),
            Ok(IdleWake::Signalled)
        );
        assert_eq!(pin.waits, 1);
    }
    #[test]
    fn stuck_low_and_unused_input_interrupt_request_delayed_rescan() {
        let mut pin = pin();
        pin.low = true;
        for _ in 0..3 {
            assert_eq!(
                block_on(wait_for_interrupt(&mut pin)),
                Ok(IdleWake::AlreadyLow)
            );
        }
        assert_eq!(pin.waits, 0);
    }
    #[test]
    fn pin_error_cannot_park_the_scanner() {
        let mut pin = pin();
        pin.fail = true;
        assert_eq!(
            block_on(wait_for_interrupt(&mut pin)),
            Err(ErrorKind::Other)
        );
        assert_eq!(pin.waits, 0);
    }
    #[test]
    fn first_low_press_is_immediate_but_repeated_quiet_low_backs_off() {
        let mut retry = IdleRetry::default();
        assert!(!retry.back_off::<()>(Ok(IdleWake::AlreadyLow)));
        assert!(retry.back_off::<()>(Ok(IdleWake::AlreadyLow)));
        retry.active();
        assert!(!retry.back_off::<()>(Ok(IdleWake::AlreadyLow)));
        assert!(!retry.back_off::<()>(Ok(IdleWake::Signalled)));
        assert!(!retry.back_off::<()>(Ok(IdleWake::AlreadyLow)));
        assert!(retry.back_off(Err(())));
    }

    #[test]
    fn healthy_high_interrupt_waits_instead_of_polling() {
        let mut pin = pin();
        let mut future = std::boxed::Box::pin(wait_for_interrupt(&mut pin));
        let mut cx = Context::from_waker(futures::task::noop_waker_ref());
        assert!(matches!(future.as_mut().poll(&mut cx), Poll::Pending));
        drop(future);
        assert_eq!(pin.waits, 1);
    }
}

#[cfg(test)]
mod interrupt_snapshot_tests {
    extern crate std;
    use super::*;
    use embedded_hal::i2c::{ErrorKind, ErrorType};
    use embedded_hal_async::i2c::Operation;
    use futures::executor::block_on;
    use std::vec::Vec;
    struct Bus {
        trace: Vec<(u8, u8)>,
        fail_park: Option<u8>,
    }
    impl ErrorType for Bus {
        type Error = ErrorKind;
    }
    impl I2c for Bus {
        async fn transaction(
            &mut self,
            address: u8,
            ops: &mut [Operation<'_>],
        ) -> Result<(), Self::Error> {
            match ops {
                [Operation::Write([0]), Operation::Read(bytes)] => {
                    self.trace.push((address, 0));
                    bytes.copy_from_slice(&[0xff; 2]);
                }
                [Operation::Write([2])] => {
                    self.trace.push((address, 2));
                    if self.fail_park == Some(address) {
                        return Err(ErrorKind::Other);
                    }
                }
                _ => panic!("unexpected operation"),
            }
            Ok(())
        }
    }
    #[test]
    fn interrupt_snapshot_parks_each_pointer_before_next_slave_ack() {
        let mut inputs = Inputs::new(Bus {
            trace: Vec::new(),
            fail_park: None,
        });
        assert_eq!(block_on(inputs.snapshot_for_interrupt()), Ok(0));
        assert_eq!(
            inputs.bus.trace,
            ADDRESSES
                .into_iter()
                .flat_map(|a| [(a, 0), (a, 2)])
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn pointer_park_failure_cannot_return_a_complete_snapshot() {
        let mut inputs = Inputs::new(Bus {
            trace: Vec::new(),
            fail_park: Some(0x22),
        });
        assert_eq!(
            block_on(inputs.snapshot_for_interrupt()),
            Err(ErrorKind::Other)
        );
        assert_eq!(
            inputs.bus.trace,
            std::vec![(0x20, 0), (0x20, 2), (0x22, 0), (0x22, 2)]
        );
    }
    #[test]
    fn legacy_polling_scan_keeps_its_original_transactions() {
        let mut inputs = Inputs::new(Bus {
            trace: Vec::new(),
            fail_park: None,
        });
        assert_eq!(block_on(inputs.snapshot()), Ok(0));
        assert_eq!(
            inputs.bus.trace,
            ADDRESSES.into_iter().map(|a| (a, 0)).collect::<Vec<_>>()
        );
    }
}

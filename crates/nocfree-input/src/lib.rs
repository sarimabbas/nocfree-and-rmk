#![no_std]

use embedded_hal_async::i2c::I2c;

/// Six independent input ports, packed in factory expander order.
/// A complete snapshot is returned only if every expander read succeeds.
pub struct Inputs<I> {
    bus: I,
}
pub const ADDRESSES: [u8; 3] = [0x20, 0x22, 0x24];
pub const LEFT_BITS: [u8; 37] = [
    0, 1, 2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13, 14, 16, 17, 18, 19, 20, 21, 24, 25, 26, 27, 28, 29,
    32, 33, 34, 35, 36, 37, 40, 41, 42, 43, 44,
];
pub const RIGHT_BITS: [u8; 47] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46,
];

impl<I: I2c> Inputs<I> {
    pub fn new(bus: I) -> Self {
        Self { bus }
    }
    pub async fn initialize(&mut self) -> Result<(), I::Error> {
        for address in ADDRESSES {
            // No output pins. Explicitly clear polarity inversion after a warm reset.
            self.bus.write(address, &[6, 0xff, 0xff]).await?;
            self.bus.write(address, &[4, 0, 0]).await?;
        }
        Ok(())
    }
    pub async fn snapshot(&mut self) -> Result<u64, I::Error> {
        let mut pressed = 0;
        for (index, address) in ADDRESSES.into_iter().enumerate() {
            let mut ports = [0; 2];
            self.bus.write_read(address, &[0], &mut ports).await?;
            pressed |= ((!u16::from_le_bytes(ports)) as u64) << (16 * index);
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
        let mut inputs = Inputs::new(bus([0; 3]));
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
    fn ansi_mapping_has_no_duplicate_or_out_of_range_bits() {
        for bits in [&LEFT_BITS[..], &RIGHT_BITS[..]] {
            for (i, &bit) in bits.iter().enumerate() {
                assert!(bit < 48);
                assert!(!bits[..i].contains(&bit));
            }
        }
        assert!(!LEFT_BITS.contains(&7));
        assert!(!LEFT_BITS.contains(&15));
        assert!(!RIGHT_BITS.contains(&47));
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

//! Fault injection at the real electrical input interface. These do not test
//! RMK debounce, BLE queues, or host HID delivery; those remain separate gates.
use embedded_hal::i2c::{ErrorKind, ErrorType};
use embedded_hal_async::i2c::{I2c, Operation};
use futures::executor::block_on;
use nocfree_input::{ADDRESSES, Inputs};
use std::collections::VecDeque;

struct Bus {
    reads: VecDeque<(u8, Result<[u8; 2], ErrorKind>)>,
}
impl ErrorType for Bus {
    type Error = ErrorKind;
}
impl I2c for Bus {
    async fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        let (expected_address, result) = self.reads.pop_front().expect("unexpected read");
        assert_eq!(address, expected_address);
        match operations {
            [Operation::Write([0]), Operation::Read(buffer)] => {
                buffer.copy_from_slice(&result?);
                Ok(())
            }
            _ => panic!("snapshot must read both input ports in one transaction"),
        }
    }
}

fn full_scan(queue: &mut VecDeque<(u8, Result<[u8; 2], ErrorKind>)>, ports: [u16; 3]) {
    queue.extend(
        ADDRESSES
            .into_iter()
            .zip(ports)
            .map(|(address, port)| (address, Ok(port.to_le_bytes()))),
    );
}

#[test]
fn every_expander_failure_discards_scan_then_recovery_reads_fresh_state() {
    for failed in 0..3 {
        let mut reads = VecDeque::new();
        full_scan(&mut reads, [0xfffe, 0xffff, 0xffff]);
        // A partial scan contains new presses, then fails. It must produce no
        // snapshot, and the next attempt must start again at the first port.
        for (index, address) in ADDRESSES.into_iter().enumerate().take(failed + 1) {
            reads.push_back((
                address,
                if index == failed {
                    Err(ErrorKind::Other)
                } else {
                    Ok([0, 0])
                },
            ));
        }
        full_scan(&mut reads, [0xffff, 0xffff, 0xfeff]);
        let mut inputs = Inputs::new(Bus { reads });
        assert_eq!(block_on(inputs.snapshot()), Ok(1));
        assert_eq!(block_on(inputs.snapshot()), Err(ErrorKind::Other));
        assert_eq!(block_on(inputs.snapshot()), Ok(1 << 40));
    }
}

#[test]
fn repeated_cross_port_press_release_snapshots_have_no_stale_or_duplicate_bits() {
    let mut reads = VecDeque::new();
    let mut expected = Vec::new();
    for bit in 0..48 {
        let mut ports = [0xffff; 3];
        ports[bit / 16] &= !(1 << (bit % 16));
        full_scan(&mut reads, ports);
        expected.push(1_u64 << bit);
        full_scan(&mut reads, [0xffff; 3]);
        expected.push(0);
    }
    let mut inputs = Inputs::new(Bus { reads });
    for snapshot in expected {
        assert_eq!(block_on(inputs.snapshot()), Ok(snapshot));
    }
}

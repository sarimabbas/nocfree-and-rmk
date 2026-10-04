//! Temporary RAM-only stage evidence, exposed through the existing manufacturer string.
use core::sync::atomic::{AtomicU32, Ordering};

#[derive(Clone, Copy)]
pub struct Snapshot {
    pub usb: u32,
    pub run: u32,
    pub stat: u32,
}
#[derive(Clone, Copy)]
pub enum Phase {
    PreHal = 0,
    PostHal = 1,
    PostClock = 2,
}
#[derive(Clone, Copy)]
pub enum Outcome {
    Entered = b'I' as isize,
    VbusAbsent = b'V' as isize,
    ClockTimeout = b'C' as isize,
    StageElapsed = b'E' as isize,
}
static WORDS: [AtomicU32; 9] = [const { AtomicU32::new(0) }; 9];
static REASON: AtomicU32 = AtomicU32::new(Outcome::Entered as u32);
static EVENTS: AtomicU32 = AtomicU32::new(0);

pub fn initialize(snapshot: Snapshot) {
    for word in &WORDS {
        word.store(0, Ordering::Relaxed);
    }
    EVENTS.store(0, Ordering::Relaxed);
    outcome(Outcome::Entered);
    record(Phase::PreHal, snapshot);
}
pub fn record(phase: Phase, snapshot: Snapshot) {
    let start = phase as usize * 3;
    for (offset, value) in [snapshot.usb, snapshot.run, snapshot.stat]
        .into_iter()
        .enumerate()
    {
        WORDS[start + offset].store(value, Ordering::Relaxed);
    }
}
pub fn outcome(reason: Outcome) {
    REASON.store(reason as u32, Ordering::Relaxed);
}
pub fn reset_seen() {
    EVENTS.fetch_or(1, Ordering::Relaxed);
}
pub fn configured_seen() {
    EVENTS.fetch_or(2, Ordering::Relaxed);
}
#[cfg(any(not(feature = "right"), test))]
fn snapshot(phase: usize) -> Snapshot {
    Snapshot {
        usb: WORDS[phase * 3].load(Ordering::Relaxed),
        run: WORDS[phase * 3 + 1].load(Ordering::Relaxed),
        stat: WORDS[phase * 3 + 2].load(Ordering::Relaxed),
    }
}
// nRF52833 fields: USB VBUS/OUTPUTRDY [0:1], HFCLKRUN.STATUS [0],
// HFCLKSTAT.SRC [0], HFCLKSTAT.STATE [16]. Raw words are retained above.
#[cfg(any(not(feature = "right"), test))]
fn packed(value: Snapshot) -> u8 {
    ((value.usb & 3)
        | ((value.run & 1) << 2)
        | (((value.stat >> 16) & 1) << 3)
        | ((value.stat & 1) << 4)) as u8
}
#[cfg(any(not(feature = "right"), test))]
fn encoded() -> [u8; 27] {
    let mut text = *b"rescue:I i=00 a=00 c=00 e=0";
    text[7] = REASON.load(Ordering::Relaxed) as u8;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for (phase, offset) in [11, 16, 21].into_iter().enumerate() {
        let bits = packed(snapshot(phase));
        text[offset] = HEX[(bits >> 4) as usize];
        text[offset + 1] = HEX[(bits & 15) as usize];
    }
    text[26] = HEX[(EVENTS.load(Ordering::Relaxed) & 3) as usize];
    text
}
#[cfg(all(not(test), not(feature = "right")))]
pub fn manufacturer() -> &'static str {
    // Called once by normal RMK configuration after the stage has returned.
    static TEXT: static_cell::StaticCell<[u8; 27]> = static_cell::StaticCell::new();
    core::str::from_utf8(TEXT.init(encoded())).expect("diagnostic string is ASCII")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_trace_latches_events_and_encodes_all_branches_without_truncation() {
        initialize(Snapshot {
            usb: 3,
            run: 1,
            stat: 0x10001,
        });
        record(
            Phase::PostHal,
            Snapshot {
                usb: 1,
                run: 0,
                stat: 0,
            },
        );
        record(
            Phase::PostClock,
            Snapshot {
                usb: 3,
                run: 1,
                stat: 0x10001,
            },
        );
        reset_seen();
        configured_seen();
        reset_seen();
        for reason in [
            Outcome::Entered,
            Outcome::VbusAbsent,
            Outcome::ClockTimeout,
            Outcome::StageElapsed,
        ] {
            outcome(reason);
            let text = encoded();
            assert_eq!(text.len(), 27);
            assert!(text.is_ascii());
            assert_eq!(text[7], reason as u8);
            assert_eq!(&text[9..], b"i=1f a=01 c=1f e=3");
        }
        initialize(Snapshot {
            usb: 0,
            run: 0,
            stat: 0,
        });
        assert_eq!(&encoded(), b"rescue:I i=00 a=00 c=00 e=0");
        // Reserved bits cannot masquerade as documented register fields.
        assert_eq!(
            packed(Snapshot {
                usb: !3,
                run: !1,
                stat: !0x10001
            }),
            0
        );
        for bits in 0..32u8 {
            let registers = Snapshot {
                usb: (bits & 3) as u32,
                run: ((bits >> 2) & 1) as u32,
                stat: (((bits >> 3) & 1) as u32) << 16 | ((bits >> 4) & 1) as u32,
            };
            assert_eq!(packed(registers), bits);
        }
    }
}

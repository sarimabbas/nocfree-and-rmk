//! One PWM owner; immutable RAM sequences, bounded DMA waits.
use embassy_nrf::{
    gpio::Level,
    pwm::{Config, Prescaler, SequenceConfig, SequencePwm, SingleSequenceMode, SingleSequencer},
};
use embassy_time::{Duration, Timer, with_timeout};
use static_cell::StaticCell;
pub fn new(
    pwm: embassy_nrf::Peri<'static, embassy_nrf::peripherals::PWM0>,
    pin: embassy_nrf::Peri<'static, embassy_nrf::peripherals::P0_20>,
) -> SequencePwm<'static> {
    let mut config = Config::default();
    config.prescaler = Prescaler::Div2;
    config.max_duty = 20000;
    config.ch0_idle_level = if cfg!(feature = "backlight-active-low") {
        Level::High
    } else {
        Level::Low
    };
    SequencePwm::new_1ch(pwm, pin, config).unwrap()
}
pub async fn run(mut pwm: SequencePwm<'static>) -> ! {
    static WORDS: StaticCell<[[u16; 1]; 16]> = StaticCell::new();
    let words: &'static [[u16; 1]; 16] = WORDS.init(core::array::from_fn(|level| {
        [(20000 * level as u32 / 15) as u16
            | if cfg!(feature = "backlight-active-low") {
                0
            } else {
                0x8000
            }]
    }));
    let mut changes = crate::board_backlight::output_receiver();
    let mut level = changes.changed().await;
    loop {
        let mut loaded = pwm.event_seq_end();
        loaded.clear();
        let mut stopped = pwm.event_stopped();
        // Sole playback owner triggers STOP only after DMA load.
        let mut stop = unsafe { pwm.task_stop() };
        let sequence = SingleSequencer::new(
            &mut pwm,
            &words[usize::from(level.min(15))],
            SequenceConfig::default(),
        );
        if sequence.start(SingleSequenceMode::Infinite).is_err() {
            sequence.stop();
        } else {
            let completed = with_timeout(Duration::from_millis(20), async {
                while !loaded.is_triggered() {
                    Timer::after_micros(100).await;
                }
            })
            .await
            .is_ok();
            if !completed {
                defmt::warn!("PWM load timeout");
                sequence.stop();
            } else if level == 0 {
                stopped.clear();
                stop.trigger();
                if with_timeout(Duration::from_millis(20), async {
                    while !stopped.is_triggered() {
                        Timer::after_micros(100).await;
                    }
                })
                .await
                .is_err()
                {
                    defmt::warn!("PWM stop timeout");
                }
                sequence.stop();
            }
        }
        // Retain intermediate PWM and its borrow until the next output request.
        level = changes.changed().await;
        drop(sequence);
    }
}

use core::num::NonZero;

use esp_idf_hal::gpio::{AnyIOPin, InterruptType, PinDriver, Pull};
use esp_idf_hal::task::notification::Notification;

use std::time::{Duration, Instant};

const DEBOUNCE_DELAY: Duration = Duration::from_millis(50);

pub fn button_task<F1, F2>(
    button: AnyIOPin,
    f_press: Option<F1>,
    f_long_press: Option<F2>,
    long_press: Option<Duration>,
) -> anyhow::Result<()>
where
    F1: Fn(),
    F2: Fn(),
{
    let mut button = PinDriver::input(button)?;
    button.set_pull(Pull::Up)?;
    button.set_interrupt_type(InterruptType::AnyEdge)?;

    let mut timer: Option<Instant> = None;

    loop {
        let notification = Notification::new();
        let waker = notification.notifier();

        // register interrupt callback
        unsafe {
            button
                .subscribe_nonstatic(move || {
                    waker.notify(NonZero::new(1).unwrap());
                })
                .unwrap();
        }

        // enable interrupt
        button.enable_interrupt()?;

        // wait for notification
        notification.wait_any();

        match timer {
            None => {
                // Press detected - start timer
                timer = Some(Instant::now());
            }
            Some(start) => {
                let delay = Instant::now() - start;
                if delay < DEBOUNCE_DELAY {
                    // log::info!(">> Debounce: {:.2}", delay.as_secs_f32());
                } else if long_press.is_none() || delay < long_press.unwrap() {
                    // log::info!(">> Short Press: {:.2}", delay.as_secs_f32());
                    if let Some(ref f) = f_press {
                        f();
                    }
                } else {
                    // log::info!(">> Long Press: {:.2}", delay.as_secs_f32());
                    if let Some(ref f) = f_long_press {
                        f();
                    }
                }
                timer = None;
            }
        }
    }
}

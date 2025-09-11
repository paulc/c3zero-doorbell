use esp_idf_hal::ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver};
use esp_idf_svc::hal::gpio::AnyOutputPin;
use esp_idf_svc::hal::prelude::Hertz;

use std::sync::mpsc;
use std::time::Duration;

type LedcTimer = esp_idf_svc::hal::ledc::TIMER0;
type LedcChannel = esp_idf_svc::hal::ledc::CHANNEL0;

pub enum PwmMessage {
    Enable,
    Disable,
    SetDuty(f32),
    SetFrequency(Hertz),
}

pub fn pwm_task(
    pwm_pin: AnyOutputPin,
    ledc_timer: LedcTimer,
    ledc_channel: LedcChannel,
    timer_config: TimerConfig,
    pwm_rx: mpsc::Receiver<PwmMessage>,
) -> anyhow::Result<()> {
    let mut timer_driver = LedcTimerDriver::new(ledc_timer, &timer_config)?;
    let mut driver = LedcDriver::new(ledc_channel, &timer_driver, pwm_pin)
        .map_err(|e| anyhow::anyhow!("LedcDriver Error: {e:?}"))?;
    let max_duty = driver.get_max_duty();
    log::info!("LEDC Max Duty: {max_duty}");
    driver.disable()?;
    driver.set_duty(max_duty / 2)?;
    loop {
        match pwm_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(PwmMessage::Enable) => driver.enable()?,
            Ok(PwmMessage::Disable) => driver.disable()?,
            Ok(PwmMessage::SetDuty(d)) => driver.set_duty((d * max_duty as f32) as u32)?,
            Ok(PwmMessage::SetFrequency(f)) => timer_driver.set_frequency(f)?,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(e) => log::error!("pwm_rx error: {e}"),
        }
    }
}

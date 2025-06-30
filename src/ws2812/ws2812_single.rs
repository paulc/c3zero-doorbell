use anyhow::Result;
use esp_idf_hal::rmt::{config::TransmitConfig, FixedLengthSignal, PinState, Pulse, TxRmtDriver};
use std::time::Duration;

use crate::ws2812::{Rgb, RgbLayout};
use crate::ws2812::{Ws2812RmtChannel, T0H, T0L, T1H, T1L};

// Simplified driver for single WS2812 on RMT Channel 0 - typically for onboard LED
// (avoids thread lifetime complications when used with Status)
pub struct Ws2812RmtSingle<'a> {
    tx: esp_idf_hal::rmt::TxRmtDriver<'a>,
    format: RgbLayout,
}

impl Ws2812RmtSingle<'_> {
    pub fn new(
        led: esp_idf_hal::gpio::AnyOutputPin,
        channel: Ws2812RmtChannel,
        format: RgbLayout,
    ) -> Result<Self> {
        let config = TransmitConfig::new().clock_divider(1);
        let tx = TxRmtDriver::new(channel, led, &config)?;
        Ok(Self { tx, format })
    }

    pub fn set(&mut self, rgb: Rgb) -> Result<()> {
        let colour: u32 = rgb.to_u32(self.format);
        let ticks_hz = self.tx.counter_clock()?;
        let (t0h, t0l, t1h, t1l) = (
            Pulse::new_with_duration(ticks_hz, PinState::High, &Duration::from_nanos(T0H))?,
            Pulse::new_with_duration(ticks_hz, PinState::Low, &Duration::from_nanos(T0L))?,
            Pulse::new_with_duration(ticks_hz, PinState::High, &Duration::from_nanos(T1H))?,
            Pulse::new_with_duration(ticks_hz, PinState::Low, &Duration::from_nanos(T1L))?,
        );
        let mut signal = FixedLengthSignal::<24>::new();
        for i in (0..24).rev() {
            if (colour >> i) & 1 == 0 {
                signal.set(23 - i, &(t0h, t0l))?;
            } else {
                signal.set(23 - i, &(t1h, t1l))?;
            }
        }
        self.tx.start_blocking(&signal)?;
        Ok(())
    }
}

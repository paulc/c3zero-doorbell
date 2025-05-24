use anyhow::Result;
use esp_idf_hal::rmt::{config::TransmitConfig, FixedLengthSignal, PinState, Pulse, TxRmtDriver};
use std::time::Duration;

use crate::rgb::{Rgb, RgbLayout};

// ws2812 timings
const T0H: u64 = 400;
const T0L: u64 = 850;
const T1H: u64 = 800;
const T1L: u64 = 450;
const RESET: u64 = 50000;

pub type Ws2812RmtChannel = esp_idf_hal::rmt::CHANNEL0;

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

pub struct Ws2812Rmt<'a> {
    tx: esp_idf_hal::rmt::TxRmtDriver<'a>,
    signal: esp_idf_hal::rmt::VariableLengthSignal,
    format: RgbLayout,
}

impl<'a> Ws2812Rmt<'a> {
    // Expects configured RMT channel
    //
    // let led = peripherals.pins.gpio10.downgrade_output();
    // let channel = peripherals.rmt.channel0;
    // let config = TransmitConfig::new().clock_divider(1);
    // let tx = TxRmtDriver::new(channel, led, &config)?;
    //
    pub fn new(tx: TxRmtDriver<'a>, n: usize, format: RgbLayout) -> Self {
        // 2 pulses / led + reset
        let signal = esp_idf_hal::rmt::VariableLengthSignal::with_capacity(2 * n + 1);
        Self { tx, signal, format }
    }
    pub fn set<T>(&mut self, colours: T) -> Result<()>
    where
        T: IntoIterator<Item = Rgb>,
    {
        self.signal.clear();
        let ticks_hz = self.tx.counter_clock()?;
        let (t0h, t0l, t1h, t1l, reset) = (
            Pulse::new_with_duration(ticks_hz, PinState::High, &Duration::from_nanos(T0H))?,
            Pulse::new_with_duration(ticks_hz, PinState::Low, &Duration::from_nanos(T0L))?,
            Pulse::new_with_duration(ticks_hz, PinState::High, &Duration::from_nanos(T1H))?,
            Pulse::new_with_duration(ticks_hz, PinState::Low, &Duration::from_nanos(T1L))?,
            Pulse::new_with_duration(ticks_hz, PinState::Low, &Duration::from_nanos(RESET))?,
        );
        for rgb in colours {
            let colour: u32 = rgb.to_u32(self.format);
            for i in (0..24).rev() {
                if (colour >> i) & 1 == 0 {
                    self.signal.push([&t0h, &t0l])?;
                } else {
                    self.signal.push([&t1h, &t1l])?;
                }
            }
        }
        self.signal.push([&reset])?;
        self.tx.start_blocking(&self.signal)?;
        Ok(())
    }
}

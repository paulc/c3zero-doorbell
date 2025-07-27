use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use doorbell::ws2812::{colour, Rgb, Ws2812RmtSingle};

pub enum LedMessage {
    Ring(bool),
    Flash(Rgb),
}

pub fn led_task(mut led: Ws2812RmtSingle, led_rx: mpsc::Receiver<LedMessage>) {
    let mut ring = false;
    let mut timeout: Option<u8> = None;
    let mut on = false;
    loop {
        match led_rx.try_recv() {
            Ok(LedMessage::Ring(v)) => {
                log::info!(">> led_rx: {v}");
                if v {
                    ring = true;
                    timeout = None; // Reset timeout if necessary
                } else {
                    // Keep flashing for timeout cycles
                    timeout = Some(5);
                }
            }
            Ok(LedMessage::Flash(c)) => {
                // Only flash if not ringing
                if !ring {
                    led.set(c).unwrap();
                    led.set(colour::OFF).unwrap();
                }
            }
            Err(_e) => {}
        }
        // log::info!("ring={ring} timeout={timeout:?} on={on}");
        led.set(if ring && on { colour::RED } else { colour::OFF })
            .unwrap();
        on = !on;

        timeout = match timeout {
            Some(0) => {
                ring = false;
                None
            }
            Some(n) => Some(n - 1),
            None => None,
        };
        thread::sleep(Duration::from_millis(200));
    }
}

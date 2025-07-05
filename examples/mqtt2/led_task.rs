use std::sync::mpsc;
use std::time::Duration;

use doorbell::rgb;
use doorbell::ws2812;

pub fn led_task(mut led: ws2812::Ws2812RmtSingle, led_rx: mpsc::Receiver<bool>) {
    let mut ring = false;
    let mut timeout: Option<u8> = None;
    let mut on = false;
    loop {
        match led_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(v) => {
                log::info!(">> led_rx: {v}");
                if v {
                    ring = true;
                    timeout = None; // Reset timeout if necessary
                } else {
                    // Keep flashing for timeout cycles
                    timeout = Some(5);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(e) => log::error!("led_rx error: {e}"),
        }
        // log::info!("ring={ring} timeout={timeout:?} on={on}");
        led.set(if ring && on { rgb::RED } else { rgb::OFF })
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
    }
}

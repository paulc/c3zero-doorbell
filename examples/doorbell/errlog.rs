#![allow(dead_code)]

use doorbell::mqtt::StaticMqttManager;
use std::sync::atomic::{AtomicBool, Ordering};

pub static MQTT_DEBUG: AtomicBool = AtomicBool::new(true);
pub static MQTT_DEBUG_TOPIC: &str = "doorbell/debug";

use std::sync::{Mutex, OnceLock};

const ERROR_LOG_LENGTH: usize = 20;
static ERROR_LOG: OnceLock<Mutex<fifo::FixedFifo<String>>> = OnceLock::new();

pub fn errorlog_init(max_size: usize) -> anyhow::Result<()> {
    ERROR_LOG
        .set(Mutex::new(fifo::FixedFifo::new(max_size)))
        .map_err(|_| anyhow::anyhow!("ERROR_LOG invalid state"))
}

pub fn errorlog_push(msg: String) {
    // Ignore errors
    if let Some(mut log) = ERROR_LOG.get().and_then(|log| log.lock().ok()) {
        log.push(msg)
    }
}

pub fn errorlog_pop() -> Option<String> {
    ERROR_LOG
        .get()
        .and_then(|log| log.lock().ok())
        .and_then(|mut log| log.pop())
}

mod fifo {

    use std::collections::VecDeque;

    pub struct FixedFifo<T> {
        buffer: VecDeque<T>,
        max_size: usize,
    }

    impl<T> FixedFifo<T> {
        pub fn new(max_size: usize) -> Self {
            Self {
                buffer: VecDeque::with_capacity(max_size),
                max_size,
            }
        }

        pub fn push(&mut self, value: T) {
            if self.buffer.len() >= self.max_size {
                self.buffer.pop_front(); // Remove oldest element
            }
            self.buffer.push_back(value);
        }

        pub fn pop(&mut self) -> Option<T> {
            self.buffer.pop_front()
        }

        pub fn len(&self) -> usize {
            self.buffer.len()
        }

        pub fn is_empty(&self) -> bool {
            self.buffer.is_empty()
        }

        pub fn is_full(&self) -> bool {
            self.buffer.len() == self.max_size
        }

        pub fn front(&self) -> Option<&T> {
            self.buffer.front()
        }

        pub fn back(&self) -> Option<&T> {
            self.buffer.back()
        }
    }
}

pub fn mqtt_debug(msg: &str) {
    if MQTT_DEBUG.load(Ordering::Relaxed) {
        let _ = StaticMqttManager::publish(MQTT_DEBUG_TOPIC, msg.as_bytes(), false);
    }
}

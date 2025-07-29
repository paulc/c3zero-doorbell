use doorbell::mqtt::StaticMqttManager;
use std::sync::atomic::{AtomicBool, Ordering};

pub static MQTT_DEBUG: AtomicBool = AtomicBool::new(true);
pub static MQTT_DEBUG_TOPIC: &str = "doorbell/debug";

pub fn mqtt_debug(msg: &str) {
    if MQTT_DEBUG.load(Ordering::Relaxed) {
        let _ = StaticMqttManager::publish(MQTT_DEBUG_TOPIC, msg.as_bytes(), false);
    }
}

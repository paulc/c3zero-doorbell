use std::sync::mpsc;

use doorbell::mqtt::MqttManager;

pub fn _mqtt_handler_cb<F>(callback: F) -> anyhow::Result<MqttManager>
where
    F: Fn(&str, &[u8]) + 'static + Send + Sync,
{
    let url = "mqtt://192.168.60.1:1883";
    let client_id = "mqtt-alarm";
    let topics = vec!["doorbell/ring", "test/+"];

    if let Ok(mut mqtt) = MqttManager::new_cb(url, client_id, callback) {
        for t in topics {
            mqtt.subscribe(t)?
        }
        Ok(mqtt)
    } else {
        Err(anyhow::anyhow!("MQTT Connection Error"))
    }
}

pub fn mqtt_handler_chan(tx: mpsc::Sender<(String, Vec<u8>)>) -> anyhow::Result<MqttManager> {
    let url = "mqtt://192.168.60.1:1883";
    let client_id = "mqtt-alarm";
    let topics = vec!["doorbell/ring", "test/+"];

    if let Ok(mut mqtt) = MqttManager::new_chan(url, client_id, tx) {
        for t in topics {
            mqtt.subscribe(t)?
        }
        Ok(mqtt)
    } else {
        Err(anyhow::anyhow!("MQTT Connection Error"))
    }
}

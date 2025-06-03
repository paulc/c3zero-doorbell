use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::wifi::{
    AccessPointConfiguration, AccessPointInfo, AuthMethod, Configuration, EspWifi,
};
use heapless::String;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Mutex;

use crate::nvs::NVStore;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct APConfig {
    pub ssid: String<32>,
    pub password: String<64>,
}

impl APConfig {
    pub fn new(ssid: &str, password: &str) -> anyhow::Result<Self> {
        Ok(APConfig {
            ssid: ssid
                .try_into()
                .map_err(|_| anyhow::anyhow!("Failed to create SSID"))?,
            password: password
                .try_into()
                .map_err(|_| anyhow::anyhow!("Failed to create PW"))?,
        })
    }
}

pub struct APStore(());

impl APStore {
    pub fn get_aps() -> anyhow::Result<impl Iterator<Item = APConfig>> {
        let out = NVStore::get::<HashMap<heapless::String<32>, APConfig>>("aps")?
            .unwrap_or(HashMap::new());
        Ok(out.into_values())
    }
    pub fn get_ap(ssid: &heapless::String<32>) -> anyhow::Result<Option<APConfig>> {
        let out = NVStore::get::<HashMap<heapless::String<32>, APConfig>>("aps")?
            .unwrap_or(HashMap::new());
        Ok(out.get(ssid).cloned())
    }
    pub fn get_ap_str(ssid: &str) -> anyhow::Result<Option<APConfig>> {
        let ssid =
            heapless::String::<32>::try_from(ssid).map_err(|_| anyhow::anyhow!("Invaled SSID"))?;
        APStore::get_ap(&ssid)
    }
    pub fn add_ap(ap: &APConfig) -> anyhow::Result<()> {
        let mut aps = NVStore::get::<HashMap<heapless::String<32>, APConfig>>("aps")?
            .unwrap_or(HashMap::new());
        aps.insert(ap.ssid.clone(), ap.clone());
        NVStore::set("aps", &aps)?;
        Ok(())
    }
    pub fn delete_ap(ssid: &str) -> anyhow::Result<()> {
        let mut aps = NVStore::get::<HashMap<heapless::String<32>, APConfig>>("aps")?
            .unwrap_or(HashMap::new());
        let ssid_owned: heapless::String<32> = ssid
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invaled SSID"))?;
        aps.remove(&ssid_owned);
        NVStore::set("aps", &aps)?;
        Ok(())
    }
}

pub static WIFI_SCAN: Mutex<Vec<AccessPointInfo>> = Mutex::new(Vec::new());

pub fn wifi_init(wifi: &mut EspWifi) -> anyhow::Result<()> {
    // Start WiFi initially with default config for scan
    let config = Configuration::Client(esp_idf_svc::wifi::ClientConfiguration {
        ..Default::default()
    });
    wifi.set_configuration(&config)?;
    wifi.start()?;
    Ok(())
}

pub fn wifi_scan(wifi: &mut EspWifi) -> anyhow::Result<()> {
    log::info!("Starting WiFi scan...");
    // Note that scan will disable WiFi connection
    let scan = wifi
        .scan()?
        .into_iter()
        .inspect(|ap| {
            log::info!(
                "SSID: {:?}, Channel: {}, RSSI: {}, Auth: {:?}",
                ap.ssid,
                ap.channel,
                ap.signal_strength,
                ap.auth_method,
            )
        })
        .collect::<Vec<_>>();
    let mut aps = WIFI_SCAN.lock().unwrap();
    *aps = scan;
    Ok(())
}

pub fn connect_wifi(
    wifi: &mut EspWifi,
    config: &APConfig,
    timeout_ms: u32,
) -> anyhow::Result<bool> {
    const SLEEP_MS: u32 = 500;
    let sta_config = Configuration::Client(esp_idf_svc::wifi::ClientConfiguration {
        ssid: config.ssid.clone(),
        password: config.password.clone(),
        ..Default::default()
    });

    wifi.set_configuration(&sta_config)?;
    wifi.start()?;
    wifi.connect()?;

    let mut timer = 0;
    loop {
        match wifi.is_up()? {
            true => break,
            false => {
                log::info!(
                    "Connecting: {} [{}ms] {}",
                    config.ssid,
                    timer,
                    match wifi.is_connected()? {
                        true => "<connected>",
                        false => "",
                    }
                );
                FreeRtos::delay_ms(SLEEP_MS);
                timer += SLEEP_MS;
                if timer >= timeout_ms {
                    wifi.stop()?;
                    return Ok(false);
                }
            }
        }
    }
    log::info!(
        "Connected:  {} {:?}",
        config.ssid,
        wifi.sta_netif().get_ip_info()?
    );
    Ok(true)
}

pub fn start_access_point(wifi: &mut EspWifi) -> anyhow::Result<()> {
    let ssid: heapless::String<32> =
        heapless::String::from_str("ESP32C3-AP").map_err(|_| anyhow::anyhow!("SSID too long"))?;
    let password: heapless::String<64> =
        heapless::String::from_str("password").map_err(|_| anyhow::anyhow!("PW too long"))?;

    let ap_config = AccessPointConfiguration {
        ssid,
        password,
        channel: 1,
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    };

    wifi.set_configuration(&Configuration::AccessPoint(ap_config))?;
    wifi.start()?;

    log::info!("Access Point started. Connect to ESP32C3-AP with password 'password'");

    Ok(())
}

pub fn find_known_aps() -> Vec<APConfig> {
    let mut known = Vec::new();
    let mut seen = Vec::new(); // We can see same SSID on multiple bands
    {
        // Only lock mutex in block
        let aps = WIFI_SCAN.lock().unwrap();
        for ap in aps.iter() {
            if !seen.contains(&ap.ssid.as_str()) {
                // Check if we have configuration in NVS
                if let Ok(Some(config)) = APStore::get_ap(&ap.ssid) {
                    log::info!("Found AP config: {}", ap.ssid.as_str());
                    known.push(config);
                }
                seen.push(ap.ssid.as_str());
            }
        }
    }
    known
}

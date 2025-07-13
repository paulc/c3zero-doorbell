use esp_idf_svc::http::Method;
use esp_idf_svc::ipv4::IpInfo;
use esp_idf_svc::wifi::{
    AccessPointConfiguration, AccessPointInfo, AuthMethod, Configuration, EspWifi,
};

use std::sync::Mutex;
use std::time::Duration;

pub mod apstore;
pub mod web;

// Exports
pub use apstore::{APConfig, APStore};

// Static scan results
pub static WIFI_SCAN: Mutex<Vec<AccessPointInfo>> = Mutex::new(Vec::new());

pub struct WifiManager<'a> {
    wifi: EspWifi<'a>,
    visible: Option<Vec<AccessPointInfo>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum WifiState {
    NotConnected,
    Station(APConfig, IpInfo),
    AP(APConfig, IpInfo),
}

impl std::fmt::Display for WifiState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            WifiState::NotConnected => write!(f, "Not Connected"),
            WifiState::Station(config, ip_info) => {
                write!(f, "Station Mode: SSID: {}, IP: {}", config.ssid, ip_info.ip)
            }
            WifiState::AP(config, ip_info) => {
                write!(
                    f,
                    "Access Point Mode: SSID: {}, IP: {}",
                    config.ssid, ip_info.ip
                )
            }
        }
    }
}

impl WifiState {
    pub fn display_fields(&self) -> Vec<(String, String)> {
        let key: [&str; 3] = ["Wifi State", "SSID", "IP Address"];
        let value: [&str; 3] = match self {
            WifiState::NotConnected => ["Not Connected", "N/A", "N/A"],
            WifiState::Station(ap, ip) => {
                ["Connected (Station)", ap.ssid.as_ref(), &ip.ip.to_string()]
            }
            WifiState::AP(ap, ip) => ["Access Point", ap.ssid.as_ref(), &ip.ip.to_string()],
        };
        key.into_iter()
            .map(|s| s.to_string())
            .zip(value.into_iter().map(|s| s.to_string()))
            .collect::<Vec<_>>()
    }
}

const SLEEP_MS: u64 = 500;

impl<'a> WifiManager<'a> {
    pub fn new(wifi: EspWifi<'a>) -> anyhow::Result<Self> {
        Ok(Self {
            wifi,
            visible: None,
        })
    }

    pub fn add_handlers(
        &self,
        server: &mut crate::web::WebServer,
        navbar: crate::web::NavBar<'static>,
    ) -> anyhow::Result<()> {
        server.add_handler("/wifi", Method::Get, web::wifi_handler(navbar))?;
        server.add_handler("/wifi/delete/*", Method::Get, web::ap_delete_handler())?;
        server.add_handler("/wifi/add", Method::Post, web::ap_add_handler())?;
        Ok(())
    }

    pub fn is_connected(&self) -> anyhow::Result<bool> {
        self.wifi
            .is_connected()
            .map_err(|e| anyhow::anyhow!("WiFi Error: {e}"))
    }

    pub fn scan(&mut self) -> anyhow::Result<()> {
        let config = Configuration::Client(esp_idf_svc::wifi::ClientConfiguration {
            ..Default::default()
        });
        self.wifi.set_configuration(&config)?;
        self.wifi.start()?;
        self.visible = Some(
            self.wifi
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
                .collect::<Vec<_>>(),
        );
        // Sort by signal_strength
        if let Some(aps) = self.visible.as_mut() {
            aps.sort_by(|a, b| b.signal_strength.cmp(&a.signal_strength))
        }
        // Save to WIFI_SCAN static (for web handler)
        let mut wifi_scan = WIFI_SCAN.lock().unwrap();
        *wifi_scan = self.visible.clone().unwrap(); // We know that visible is Some
        Ok(())
    }

    pub fn try_connect(
        &mut self,
        known: &[APConfig],
        local: Option<APConfig>,
        timeout_ms: u64,
    ) -> anyhow::Result<WifiState> {
        // Check strongest signal first
        let visible = self.visible.clone(); // Avoid borrow
        if let Some(ref visible) = visible {
            for ap in visible.iter().filter_map(|visible_ap| {
                known
                    .iter()
                    .find(|&known_ap| visible_ap.ssid == known_ap.ssid)
            }) {
                if let Ok(WifiState::Station(ap, ip)) = self.connect_sta(ap, timeout_ms) {
                    return Ok(WifiState::Station(ap, ip));
                }
            }
        };
        // Unable to connect - if ap provided start in AP mode
        if let Some(local) = local {
            self.start_ap(&local)
        } else {
            Ok(WifiState::NotConnected)
        }
    }

    pub fn connect_sta(&mut self, config: &APConfig, timeout_ms: u64) -> anyhow::Result<WifiState> {
        let sta_config = Configuration::Client(esp_idf_svc::wifi::ClientConfiguration {
            ssid: config.ssid.clone(),
            password: config.password.clone(),
            ..Default::default()
        });

        self.wifi.set_configuration(&sta_config)?;
        self.wifi.start()?;
        self.wifi.connect()?;

        let mut timer = 0;
        loop {
            match self.wifi.is_up()? {
                true => break,
                false => {
                    log::info!(
                        "Connecting: {} [{}ms] {}",
                        config.ssid,
                        timer,
                        match self.wifi.is_connected()? {
                            true => "<connected>",
                            false => "",
                        }
                    );
                    std::thread::sleep(Duration::from_millis(SLEEP_MS));
                    timer += SLEEP_MS;
                    if timer >= timeout_ms {
                        self.wifi.stop()?;
                        return Ok(WifiState::NotConnected);
                    }
                }
            }
        }
        let ip_info = self.wifi.sta_netif().get_ip_info()?;
        Ok(WifiState::Station(config.clone(), ip_info))
    }

    pub fn start_ap(&mut self, config: &APConfig) -> anyhow::Result<WifiState> {
        let ap_config = if config.password.is_empty() {
            AccessPointConfiguration {
                ssid: config.ssid.clone(),
                channel: 1,
                auth_method: AuthMethod::None,
                ..Default::default()
            }
        } else {
            AccessPointConfiguration {
                ssid: config.ssid.clone(),
                password: config.password.clone(),
                channel: 1,
                auth_method: AuthMethod::WPA2Personal,
                ..Default::default()
            }
        };

        self.wifi
            .set_configuration(&Configuration::AccessPoint(ap_config))?;
        self.wifi.start()?;

        let ip_info = self.wifi.ap_netif().get_ip_info()?;

        log::info!("Access Point started: {config:?}");
        log::info!("IpInfo: {ip_info:?}");

        Ok(WifiState::AP(config.clone(), ip_info))
    }
}

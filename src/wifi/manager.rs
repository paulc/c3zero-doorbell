use esp_idf_svc::http::Method;
use esp_idf_svc::ipv4::IpInfo;
use esp_idf_svc::wifi::{
    AccessPointConfiguration, AccessPointInfo, AuthMethod, Configuration, EspWifi,
};

use std::time::Duration;

use crate::wifi::web;
use crate::wifi::APConfig;

pub struct WifiManager<'a> {
    wifi: &'a mut EspWifi<'a>,
}

const SLEEP_MS: u64 = 500;

impl<'a> WifiManager<'a> {
    pub fn new(wifi: &'a mut EspWifi<'a>) -> anyhow::Result<Self> {
        Ok(Self { wifi })
    }

    pub fn add_handlers(&self, server: &mut crate::web::WebServer) -> anyhow::Result<()> {
        server.add_handler("/wifi", Method::Get, web::handle_wifi)?;
        server.add_handler("/wifi/delete/*", Method::Get, web::handle_ap_delete)?;
        server.add_handler("/wifi/add", Method::Post, web::handle_ap_add)?;
        Ok(())
    }

    pub fn scan(&mut self) -> anyhow::Result<impl Iterator<Item = AccessPointInfo>> {
        let config = Configuration::Client(esp_idf_svc::wifi::ClientConfiguration {
            ..Default::default()
        });
        self.wifi.set_configuration(&config)?;
        self.wifi.start()?;
        Ok(self.wifi.scan()?.into_iter().inspect(|ap| {
            log::info!(
                "SSID: {:?}, Channel: {}, RSSI: {}, Auth: {:?}",
                ap.ssid,
                ap.channel,
                ap.signal_strength,
                ap.auth_method,
            )
        }))
    }

    pub fn known_aps(visible: &Vec<AccessPointInfo>, known: &Vec<APConfig>) -> Vec<APConfig> {
        known
            .iter()
            .filter(|ap| visible.iter().any(|v| v.ssid == ap.ssid))
            .cloned()
            .collect::<Vec<_>>()
    }

    pub fn connect_sta(
        &mut self,
        config: &APConfig,
        timeout_ms: u64,
    ) -> anyhow::Result<Option<IpInfo>> {
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
                        return Ok(None);
                    }
                }
            }
        }
        let ip_info = self.wifi.sta_netif().get_ip_info()?;
        Ok(Some(ip_info))
    }

    pub fn start_ap(
        &mut self,
        ssid: heapless::String<32>,
        password: Option<heapless::String<64>>,
    ) -> anyhow::Result<Option<IpInfo>> {
        let ap_config = if let Some(ref password) = password {
            AccessPointConfiguration {
                ssid: ssid.clone(),
                password: password.clone(),
                channel: 1,
                auth_method: AuthMethod::WPA2Personal,
                ..Default::default()
            }
        } else {
            AccessPointConfiguration {
                ssid: ssid.clone(),
                channel: 1,
                auth_method: AuthMethod::None,
                ..Default::default()
            }
        };

        self.wifi
            .set_configuration(&Configuration::AccessPoint(ap_config))?;
        self.wifi.start()?;

        let ip_info = self.wifi.ap_netif().get_ip_info()?;

        if let Some(ref password) = password {
            log::info!("Access Point started: SSID={ssid} / Password={password}",)
        } else {
            log::info!("Access Point started: SSID={ssid}",)
        };
        log::info!("IpInfo: {ip_info:?}");

        Ok(Some(ip_info))
    }
}

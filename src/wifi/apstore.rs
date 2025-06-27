use crate::nvs::NVStore;
use heapless::String;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

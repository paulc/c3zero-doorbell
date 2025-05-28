use esp_idf_svc::nvs::{EspDefaultNvs, EspNvs, EspNvsPartition, NvsDefault};
use heapless::String;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use crate::hash::hash_ssid;
use crate::wifi::APConfig;

#[derive(Serialize, Deserialize, Debug)]
pub struct KnownAPs(pub Vec<String<32>>);

static KNOWN_APS_LEN: usize = 2048;
static SAVED_AP_LEN: usize = 256;
pub static KNOWN_APS: Mutex<KnownAPs> = Mutex::new(KnownAPs(Vec::new()));
pub static NVS: Mutex<Option<EspNvs<NvsDefault>>> = Mutex::new(None);

pub struct APStore(());

impl APStore {
    // Initialise static NVS
    pub fn init(nvs_default_partition: EspNvsPartition<NvsDefault>) -> anyhow::Result<()> {
        let nvs = EspDefaultNvs::new(nvs_default_partition, "ap_store", true)?;
        {
            // Initialise KNOWN_APS static
            let mut known_aps: KnownAPs = KnownAPs(Vec::new());
            let mut data = [0_u8; KNOWN_APS_LEN];
            if let Ok(Some(data)) = nvs.get_raw("KNOWN_APS", &mut data) {
                known_aps = serde_json::from_slice(data)?;
            }
            log::info!("KNOWN_APS >> {:?}", known_aps);
            let mut aps = KNOWN_APS.lock().unwrap();
            *aps = known_aps;
        }
        // Initialise NVS static
        let mut nvs_static = NVS.lock().unwrap();
        *nvs_static = Some(nvs);
        Ok(())
    }

    pub fn test_nvs(key: &str, value: &[u8]) -> anyhow::Result<()> {
        let mut nvs = NVS.lock().unwrap();
        let nvs = nvs.as_mut().ok_or(anyhow::anyhow!("NVS not initialized"))?;

        log::info!("Setting NVS: {}", key);
        nvs.set_raw(key, value)?;

        log::info!("Getting NVS: {}", key);
        let mut data = [0_u8; 1024];
        if let Ok(Some(data)) = nvs.get_raw(key, &mut data) {
            log::info!("Found Key: {} len={} data={:?}", key, data.len(), data);
        }

        log::info!("Removing Key: {}", key);
        nvs.remove(key)?;
        Ok(())
    }

    pub fn add_ap(c: &APConfig) -> anyhow::Result<()> {
        let mut nvs = NVS.lock().unwrap();
        let nvs = nvs.as_mut().ok_or(anyhow::anyhow!("NVS not initialized"))?;

        // Hash SSID in case it us >16 bytes (NVS key limit)
        let k = hash_ssid(c.ssid.as_str());
        let v = serde_json::to_vec(&c)?;

        // If there is an existing config we overwrite
        log::info!("Setting NVS Config: {} [{}]", c.ssid, k.as_str());
        nvs.set_raw(k.as_str(), v.as_slice())
            .map_err(|e| anyhow::anyhow!("Error setting NVS config: {} [{}]", c.ssid, e))?;

        // Add to KNOWN_APS key
        let mut known_aps: KnownAPs = KnownAPs(Vec::new());
        let mut data = [0_u8; KNOWN_APS_LEN];
        if let Ok(Some(data)) = nvs.get_raw("KNOWN_APS", &mut data) {
            known_aps = serde_json::from_slice(data)?;
        }
        log::info!(
            ">>> KNOWN_APS: {:?} {}",
            known_aps,
            known_aps.0.contains(&c.ssid)
        );
        // Update existing value and save back to NVS
        if !known_aps.0.contains(&c.ssid) {
            known_aps.0.push(c.ssid.clone());
            let known_aps = serde_json::to_vec(&known_aps)?;
            nvs.set_raw("KNOWN_APS", known_aps.as_slice())
                .map_err(|e| anyhow::anyhow!("Error updating KNOWN_APS: [{}]", e))?;
        }

        // Update KNOWN_APS static
        let mut aps = KNOWN_APS.lock().unwrap();
        *aps = known_aps;
        Ok(())
    }

    pub fn delete_ap(ssid: &str) -> anyhow::Result<()> {
        let mut nvs = NVS.lock().unwrap();
        let nvs = nvs.as_mut().ok_or(anyhow::anyhow!("NVS not initialized"))?;

        log::info!("Deleting SSID: {}", ssid);
        let k = hash_ssid(ssid);
        nvs.remove(k.as_str())?;

        // Remove from KNOWN_APS key
        let mut known_aps: KnownAPs = KnownAPs(Vec::new());
        let mut data = [0_u8; KNOWN_APS_LEN];
        if let Ok(Some(data)) = nvs.get_raw("KNOWN_APS", &mut data) {
            known_aps = serde_json::from_slice(data)?;
        }

        // Update existing value and save back to NVS
        if let Some(index) = known_aps.0.iter().position(|x| x == ssid) {
            known_aps.0.remove(index); // Remove the item at the found index
            let known_aps = serde_json::to_vec(&known_aps)?;
            nvs.set_raw("KNOWN_APS", known_aps.as_slice())
                .map_err(|e| anyhow::anyhow!("Error updating KNOWN_APS: [{}]", e))?;
        }
        log::info!("Updating KNOWN_APS: {:?}", known_aps);

        // Update KNOWN_APS static
        let mut aps = KNOWN_APS.lock().unwrap();
        *aps = known_aps;
        Ok(())
    }
    pub fn get_ap_config(ssid: &str) -> anyhow::Result<Option<APConfig>> {
        let nvs = NVS.lock().unwrap();
        let nvs = nvs.as_ref().ok_or(anyhow::anyhow!("NVS not initialized"))?;
        let mut data = [0_u8; SAVED_AP_LEN];
        if let Ok(Some(data)) = nvs.get_raw(hash_ssid(ssid).as_str(), &mut data) {
            let config: APConfig = serde_json::from_slice(data)?;
            log::info!("Found Wifi Config: {}", ssid);
            Ok(Some(config))
        } else {
            Ok(None)
        }
    }
    pub fn get_known_aps() -> anyhow::Result<Vec<String<32>>> {
        let nvs = NVS.lock().unwrap();
        let nvs = nvs.as_ref().ok_or(anyhow::anyhow!("NVS not initialized"))?;
        let mut data = [0_u8; KNOWN_APS_LEN];
        if let Ok(Some(data)) = nvs.get_raw("KNOWN_APS", &mut data) {
            Ok(serde_json::from_slice(data)?)
        } else {
            Err(anyhow::anyhow!("Error retreiving KNOWN_APS"))
        }
    }
}

use esp_idf_svc::nvs::{EspDefaultNvs, EspNvs, EspNvsPartition, NvsDefault};
use heapless::String;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

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
            log::info!("KNOWN_APS >> {known_aps:?}");
            let mut aps = KNOWN_APS.lock().unwrap();
            *aps = known_aps;
        }
        // Initialise NVS static
        let mut nvs_static = NVS.lock().unwrap();
        *nvs_static = Some(nvs);
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

        log::info!("Deleting SSID: {ssid}");
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
        log::info!("Updating KNOWN_APS: {known_aps:?}");

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
            log::info!("Found Wifi Config: {ssid}");
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

// We can only use 15 byte string as NVS key
pub fn hash_ssid(ssid: &str) -> heapless::String<15> {
    const FNV_OFFSET: u64 = 14695981039346656037;
    const FNV_PRIME: u64 = 1099511628211;
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

    // FNV1 64bit hash
    let mut hash = FNV_OFFSET;
    ssid.as_bytes().iter().for_each(|b| {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    });

    // Convert to hex - only use bottom 60bits
    let mut buffer: heapless::Vec<u8, 15> = heapless::Vec::new();
    for _ in 0..15 {
        let nibble = (hash & 0xF) as usize;
        // Buffer is 16 bytes long so dont need to check
        unsafe { buffer.push_unchecked(HEX_CHARS[nibble]) };
        hash >>= 4;
    }
    // We know this is valid UTF8
    heapless::String::from_utf8(buffer).unwrap()
}

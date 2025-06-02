use esp_idf_svc::nvs::{EspDefaultNvs, EspNvs, EspNvsPartition, NvsDefault};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::sync::Mutex;

pub static NV_STORE: Mutex<Option<EspNvs<NvsDefault>>> = Mutex::new(None);
const NV_STORE_MAX: usize = 2048; // Maximum size for serialised data

pub struct NVStore(());

impl NVStore {
    pub fn init(nvs_partition: EspNvsPartition<NvsDefault>, namespace: &str) -> anyhow::Result<()> {
        // Initialise static NVS
        let mut nvs = NV_STORE.lock().unwrap();
        *nvs = Some(EspDefaultNvs::new(nvs_partition, namespace, true)?);
        Ok(())
    }

    pub fn get<T>(key: &str) -> anyhow::Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        let nvs = NV_STORE.lock().unwrap();
        let nvs = nvs
            .as_ref()
            .ok_or(anyhow::anyhow!("NV_STORE not initialized"))?;
        let mut buf = [0_u8; NV_STORE_MAX];
        if let Some(data) = nvs.get_raw(key, &mut buf)? {
            Ok(Some(serde_json::from_slice(data)?))
        } else {
            Ok(None)
        }
    }

    pub fn set<T>(key: &str, value: &T) -> anyhow::Result<()>
    where
        T: Serialize,
    {
        let mut nvs = NV_STORE.lock().unwrap();
        let nvs = nvs
            .as_mut()
            .ok_or(anyhow::anyhow!("NV_STORE not initialized"))?;
        let data = serde_json::to_vec(value)?;
        nvs.set_raw(key, data.as_slice())
            .map_err(|e| anyhow::anyhow!("Error updating key {key}: [{}]", e))?;
        Ok(())
    }
}

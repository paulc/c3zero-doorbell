use doorbell::wifi::APConfig;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::prelude::*;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::nvs::{EspDefaultNvs, EspNvs, EspNvsPartition, NvsDefault};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;

pub static NV_STORE: Mutex<Option<EspNvs<NvsDefault>>> = Mutex::new(None);
const NV_STORE_MAX: usize = 2048;

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

    pub fn set<T>(key: &str, value: T) -> anyhow::Result<()>
    where
        T: Serialize,
    {
        let mut nvs = NV_STORE.lock().unwrap();
        let nvs = nvs
            .as_mut()
            .ok_or(anyhow::anyhow!("NV_STORE not initialized"))?;
        let data = serde_json::to_vec(&value)?;
        nvs.set_raw(key, data.as_slice())
            .map_err(|e| anyhow::anyhow!("Error updating key {key}: [{}]", e))?;
        Ok(())
    }
}

pub struct APStore(());

impl APStore {
    pub fn get_aps() -> anyhow::Result<impl Iterator<Item = APConfig>> {
        let out = NVStore::get::<HashMap<heapless::String<32>, APConfig>>("aps")?
            .unwrap_or(HashMap::new());
        Ok(out.into_values())
    }
    pub fn get_ap(ssid: &str) -> anyhow::Result<Option<APConfig>> {
        let out = NVStore::get::<HashMap<heapless::String<32>, APConfig>>("aps")?
            .unwrap_or(HashMap::new());
        let ssid =
            heapless::String::<32>::try_from(ssid).map_err(|_| anyhow::anyhow!("Invaled SSID"))?;
        Ok(out.get(&ssid).cloned())
    }
    pub fn add_ap(ap: APConfig) -> anyhow::Result<()> {
        let mut aps = NVStore::get::<HashMap<heapless::String<32>, APConfig>>("aps")?
            .unwrap_or(HashMap::new());
        aps.insert(ap.ssid.clone(), ap);
        NVStore::set("aps", aps)?;
        Ok(())
    }
    pub fn delete_ap(ssid: &str) -> anyhow::Result<()> {
        let mut aps = NVStore::get::<HashMap<heapless::String<32>, APConfig>>("aps")?
            .unwrap_or(HashMap::new());
        let ssid_owned: heapless::String<32> = ssid
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invaled SSID"))?;
        aps.remove(&ssid_owned);
        NVStore::set("aps", aps)?;
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Starting...");

    // Initialise peripherals
    let _peripherals = Peripherals::take()?;
    let nvs_default_partition = EspDefaultNvsPartition::take()?;
    NVStore::init(nvs_default_partition.clone(), "DOORBELL")?;

    let mut aps: HashMap<heapless::String<32>, APConfig> =
        NVStore::get("aps")?.unwrap_or(HashMap::new());
    println!("{:?}", aps);

    aps.insert(
        "TEST"
            .try_into()
            .map_err(|_| anyhow::anyhow!("Failed to create SSID"))?,
        APConfig::new("TEST", "ABCD")?,
    );

    NVStore::set("aps", aps)?;

    let mut aps: HashMap<heapless::String<32>, APConfig> =
        NVStore::get("aps")?.unwrap_or(HashMap::new());
    println!("{:?}", aps);

    aps.insert(
        "TEST"
            .try_into()
            .map_err(|_| anyhow::anyhow!("Failed to create SSID"))?,
        APConfig::new("TEST", "XYZ")?,
    );

    NVStore::set("aps", aps)?;

    let aps: HashMap<heapless::String<32>, APConfig> =
        NVStore::get("aps")?.unwrap_or(HashMap::new());
    println!("{:?}", aps);

    /*
        let nvs_default_partition = EspDefaultNvsPartition::take()?;

        APStore::init(nvs_default_partition.clone(), "DOORBELL")?;

        println!("{:?}", APStore::get_aps()?);
        APStore::add_ap(APConfig::new("TEST1", "ABCD")?)?;
        APStore::add_ap(APConfig::new("TEST2", "ABCD")?)?;
        println!("{:?}", APStore::get_aps()?);
        APStore::delete_ap("TEST1")?;
        println!("{:?}", APStore::get_aps()?);
    */

    loop {
        FreeRtos::delay_ms(1000); // Delay for 100 milliseconds
    }
}

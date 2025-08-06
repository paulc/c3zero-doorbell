use esp_idf_hal::task::watchdog::{TWDTConfig, TWDTDriver, TWDT};
use std::time::Duration;

pub fn init(p: TWDT, duration: Duration) -> anyhow::Result<()> {
    let twdt_config = TWDTConfig {
        duration,
        panic_on_trigger: true,
        subscribed_idle_tasks: enumset::enum_set!(esp_idf_hal::cpu::Core::Core0),
    };
    TWDTDriver::new(p, &twdt_config)?;
    Ok(())
}
pub fn subscribe() -> anyhow::Result<()> {
    esp_idf_sys::esp!(unsafe { esp_idf_sys::esp_task_wdt_add(core::ptr::null_mut()) })
        .map_err(|e| anyhow::anyhow!("esp_task_wdt_add: {e}"))
}
pub fn feed() -> anyhow::Result<()> {
    esp_idf_sys::esp!(unsafe { esp_idf_sys::esp_task_wdt_reset() })
        .map_err(|e| anyhow::anyhow!("esp_task_wdt_reset: {e}"))
}

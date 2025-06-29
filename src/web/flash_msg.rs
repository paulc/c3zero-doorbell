use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct FlashMsg<'a> {
    pub level: &'a str,
    pub message: &'a str,
}

impl<'a> FlashMsg<'a> {
    pub fn cookie(level: &'a str, message: &'a str) -> anyhow::Result<String> {
        Ok(format!(
            "flash_msg={}; path=/",
            urlencoding::encode(&serde_json::to_string(&FlashMsg { level, message })?)
        ))
    }
}

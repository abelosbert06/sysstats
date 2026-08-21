use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const DEFAULT_API_URL: &str = "https://backend-api.krequiem.workers.dev";
const DEFAULT_SYNC_INTERVAL_SECS: u64 = 3;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub device_token: Option<String>,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub api_url: String,
    pub sync_interval_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            device_token: None,
            device_id: None,
            device_name: None,
            api_url: DEFAULT_API_URL.to_string(),
            sync_interval_secs: DEFAULT_SYNC_INTERVAL_SECS,
        }
    }
}

pub fn get_config_dir() -> PathBuf {
    if let Some(mut path) = dirs::config_dir() {
        path.push("sys_stats");
        let _ = fs::create_dir_all(&path);
        path
    } else {
        PathBuf::from("/etc/sys_stats")
    }
}

pub fn get_config_path() -> PathBuf {
    let mut path = get_config_dir();
    path.push("config.json");
    path
}

pub fn load_config() -> Config {
    let path = get_config_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str::<Config>(&content) {
                return cfg;
            }
        }
    }
    Config::default()
}

pub fn save_config(config: &Config) -> Result<(), String> {
    let path = get_config_path();
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn set_token(token: &str) -> Result<(), String> {
    let mut cfg = load_config();
    cfg.device_token = Some(token.trim().to_string());
    save_config(&cfg)
}

#[allow(dead_code)]
pub fn get_token() -> Option<String> {
    load_config().device_token.filter(|t| !t.is_empty())
}

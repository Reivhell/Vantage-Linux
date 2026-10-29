use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

pub(crate) const CONFIG_DIR: &str = ".config/vantage-linux";
pub(crate) const HELPER_PATH: &str = "/usr/local/bin/vantage-helper";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub refresh_interval_ms: u64,
    pub restore_profile_on_login: bool,
    #[serde(default)]
    pub last_thermal_mode: String,
    pub start_minimized: bool,
    pub window_width: i32,
    pub window_height: i32,
    #[serde(default)]
    pub auto_power_mode: bool,
    #[serde(default = "default_ac_preset")]
    pub ac_preset: String,
    #[serde(default = "default_battery_preset")]
    pub battery_preset: String,
    #[serde(default)]
    pub keyboard_backlight: Option<bool>,
    #[serde(default)]
    pub touchpad: Option<bool>,
    #[serde(default)]
    pub camera: Option<bool>,
    #[serde(default)]
    pub fn_lock: Option<bool>,
    #[serde(default)]
    pub win_key: Option<bool>,
    #[serde(default)]
    pub usb_charging: Option<bool>,
    #[serde(default)]
    pub vantage_key: Option<bool>,
    #[serde(default)]
    pub overdrive: Option<bool>,
}

fn default_ac_preset() -> String {
    "balanced".into()
}

fn default_battery_preset() -> String {
    "quiet".into()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            refresh_interval_ms: 1000,
            restore_profile_on_login: false,
            last_thermal_mode: "balanced".into(),
            start_minimized: false,
            window_width: 1280,
            window_height: 850,
            auto_power_mode: false,
            ac_preset: default_ac_preset(),
            battery_preset: default_battery_preset(),
            keyboard_backlight: Some(true),
            touchpad: Some(true),
            camera: Some(true),
            fn_lock: Some(false),
            win_key: Some(true),
            usb_charging: Some(false),
            vantage_key: Some(true),
            overdrive: Some(false),
        }
    }
}

pub(crate) fn home_dir() -> PathBuf {
    std::env::var("HOME").map(PathBuf::from).unwrap_or_default()
}

pub(crate) fn config_path() -> PathBuf {
    home_dir().join(CONFIG_DIR).join("config.toml")
}

static CONFIG_CACHE: OnceLock<Mutex<ConfigCache>> = OnceLock::new();

struct ConfigCache {
    config: AppConfig,
    mtime: Option<SystemTime>,
}

fn config_cache() -> &'static Mutex<ConfigCache> {
    CONFIG_CACHE.get_or_init(|| {
        Mutex::new(ConfigCache {
            config: AppConfig::default(),
            mtime: None,
        })
    })
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    let meta = fs::metadata(&path).ok();
    let mtime = meta.and_then(|m| m.modified().ok());

    let mut guard = config_cache().lock().unwrap();
    if guard.mtime == mtime {
        return guard.config.clone();
    }

    let config = fs::read_to_string(&path)
        .ok()
        .and_then(|text| toml::from_str::<AppConfig>(&text).ok())
        .unwrap_or_default();
    guard.mtime = mtime;
    guard.config = config.clone();
    config
}

pub fn save_config(cfg: &AppConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string(cfg).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| e.to_string())?;
    // Invalidate cache so next load re-reads from disk.
    if let Ok(mut guard) = config_cache().lock() {
        guard.mtime = fs::metadata(&path).ok().and_then(|m| m.modified().ok());
        guard.config = cfg.clone();
    }
    Ok(())
}

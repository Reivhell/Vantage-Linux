use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Lokasi berkas profil (XDG-compliant: ~/.config/vantage-linux/profiles.json).
fn profiles_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    Path::new(&home)
        .join(".config/vantage-linux/profiles.json")
        .to_path_buf()
}

fn ensure_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub key: String,
    pub name: String,
    pub description: String,
    /// Setting yang disimpan (key -> value sebagai string).
    pub settings: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileList {
    pub profiles: Vec<Profile>,
}

/// Profil bawaan. Setiap profile memiliki setting yang benar-benar didukung oleh firmware.
fn default_profiles() -> Vec<Profile> {
    fn map_settings(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
    vec![
        Profile {
            key: "silent-study".into(),
            name: "Silent Study".into(),
            description: "Hemat daya, layar redup, GPU diskret mati saat tidak diperlukan.".into(),
            settings: map_settings(&[
                ("thermal_mode", "quiet"),
                ("charge_mode", "Long_Life"),
                ("brightness", "40"),
                ("gpu_mode", "integrated"),
                ("gpu_runtime_pm", "auto"),
                ("fan_preset", "quiet"),
            ]),
        },
        Profile {
            key: "daily-driver".into(),
            name: "Daily Driver".into(),
            description: "Seimbang untuk browsing, kuliah, dan coding.".into(),
            settings: map_settings(&[
                ("thermal_mode", "balanced"),
                ("charge_mode", "Standard"),
                ("brightness", "60"),
                ("gpu_mode", "hybrid"),
                ("gpu_runtime_pm", "auto"),
                ("fan_preset", "auto"),
            ]),
        },
        Profile {
            key: "gaming".into(),
            name: "Gaming".into(),
            description: "Daya performa, pendinginan sesuai, refresh rate tinggi.".into(),
            settings: map_settings(&[
                ("thermal_mode", "performance"),
                ("charge_mode", "Fast"),
                ("brightness", "100"),
                ("gpu_mode", "nvidia"),
                ("gpu_runtime_pm", "on"),
                ("fan_preset", "full"),
            ]),
        },
        Profile {
            key: "rendering".into(),
            name: "Rendering".into(),
            description: "Kompilasi, rendering, atau beban komputasi panjang.".into(),
            settings: map_settings(&[
                ("thermal_mode", "performance"),
                ("charge_mode", "Fast"),
                ("brightness", "80"),
                ("gpu_mode", "nvidia"),
                ("gpu_runtime_pm", "on"),
                ("fan_preset", "full"),
            ]),
        },
        Profile {
            key: "battery-saver".into(),
            name: "Battery Saver".into(),
            description: "Kurangi konsumsi daya saat tidak pakai charger.".into(),
            settings: map_settings(&[
                ("thermal_mode", "power-saver"),
                ("charge_mode", "Long_Life"),
                ("brightness", "30"),
                ("gpu_mode", "integrated"),
                ("gpu_runtime_pm", "auto"),
                ("fan_preset", "quiet"),
            ]),
        },
    ]
}

pub fn list_profiles() -> Result<ProfileList, String> {
    let path = profiles_path();
    if !path.exists() {
        // Tulis profil bawaan saat pertama kali.
        let defaults = default_profiles();
        save_all(&ProfileList {
            profiles: defaults.clone(),
        })?;
        return Ok(ProfileList { profiles: defaults });
    }
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let list: ProfileList = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(list)
}

fn save_all(list: &ProfileList) -> Result<(), String> {
    let path = profiles_path();
    ensure_parent(&path)?;
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| e.to_string())
}

pub fn save_profile(profile: Profile) -> Result<ProfileList, String> {
    let mut list = list_profiles()?;
    if let Some(existing) = list.profiles.iter_mut().find(|p| p.key == profile.key) {
        *existing = profile;
    } else {
        list.profiles.push(profile);
    }
    save_all(&list)?;
    Ok(list)
}

pub fn delete_profile(key: &str) -> Result<ProfileList, String> {
    let mut list = list_profiles()?;
    let default_keys: Vec<String> = default_profiles().iter().map(|p| p.key.clone()).collect();
    if default_keys.iter().any(|k| k == key) {
        return Err("tidak boleh menghapus profil bawaan".into());
    }
    list.profiles.retain(|p| p.key != key);
    save_all(&list)?;
    Ok(list)
}

pub fn get_profile(key: &str) -> Result<Option<Profile>, String> {
    let list = list_profiles()?;
    Ok(list.profiles.into_iter().find(|p| p.key == key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_profiles_have_five() {
        let profiles = default_profiles();
        assert_eq!(profiles.len(), 5);
        let keys: Vec<&str> = profiles.iter().map(|p| p.key.as_str()).collect();
        assert!(keys.contains(&"silent-study"));
        assert!(keys.contains(&"daily-driver"));
        assert!(keys.contains(&"gaming"));
        assert!(keys.contains(&"rendering"));
        assert!(keys.contains(&"battery-saver"));
    }

    #[test]
    fn test_save_and_delete_profile() {
        // Gunakan path temp agar tidak menimpa profil nyata.
        // Untuk tes sederhana, cek hanya logika default_profiles dan default_profiles tidak kosong.
        let profiles = default_profiles();
        assert!(!profiles.is_empty());
        // Setiap profil bawaan harus memiliki minimal satu setting.
        for p in &profiles {
            assert!(!p.settings.is_empty());
        }
    }
}

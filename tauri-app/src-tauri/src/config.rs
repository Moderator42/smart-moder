use crate::types::{Config, ServerInfo, ServerLinks};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const ARIZONA_PC: std::ops::RangeInclusive<u32> = 1..=32;
const ARIZONA_MOBILE: std::ops::RangeInclusive<u32> = 101..=103;
const ARIZONA_VC: [u32; 1] = [200];
const RODINA_PC: std::ops::RangeInclusive<u32> = 301..=307;
const RODINA_MOBILE: std::ops::RangeInclusive<u32> = 401..=402;

pub fn project_for_server(server_num: u32) -> &'static str {
    if RODINA_PC.contains(&server_num) || RODINA_MOBILE.contains(&server_num) {
        "rodina"
    } else {
        "arizona"
    }
}

pub fn base_dir() -> PathBuf {
    if cfg!(debug_assertions) {
        return std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            return parent.to_path_buf();
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn app_config_dir() -> PathBuf {
    if cfg!(debug_assertions) {
        return base_dir();
    }

    if let Some(dir) = dirs::config_dir() {
        return dir.join("Smart Config Editor");
    }

    base_dir()
}

pub fn config_path() -> PathBuf {
    app_config_dir().join("config.json")
}

pub fn get_servers() -> Vec<ServerInfo> {
    let mut list = Vec::new();
    for id in ARIZONA_PC.clone() {
        list.push(ServerInfo { id, project: "arizona".to_string() });
    }
    for id in ARIZONA_MOBILE.clone() {
        list.push(ServerInfo { id, project: "arizona".to_string() });
    }
    for id in ARIZONA_VC {
        list.push(ServerInfo { id, project: "arizona".to_string() });
    }
    for id in RODINA_PC.clone() {
        list.push(ServerInfo { id, project: "rodina".to_string() });
    }
    for id in RODINA_MOBILE.clone() {
        list.push(ServerInfo { id, project: "rodina".to_string() });
    }
    list
}

fn default_servers() -> HashMap<String, ServerLinks> {
    let mut map = HashMap::new();
    for info in get_servers() {
        map.insert(
            info.id.to_string(),
            ServerLinks::default(),
        );
    }
    map
}

pub fn load_config() -> Result<Config> {
    let path = config_path();
    if !path.exists() {
        let mut cfg = Config::default();
        fill_config_defaults(&mut cfg);
        save_config(&cfg)?;
        return Ok(cfg);
    }

    let raw = fs::read_to_string(&path)?;
    let mut cfg: Config = serde_json::from_str(&raw)
        .map_err(|e| anyhow!("Config JSON error: {e}"))?;

    fill_config_defaults(&mut cfg);

    Ok(cfg)
}

pub fn fill_config_defaults(cfg: &mut Config) {
    if cfg.servers.is_empty() {
        cfg.servers = default_servers();
    } else {
        let defaults = default_servers();
        for (key, value) in defaults {
            cfg.servers.entry(key).or_insert(value);
        }
    }

    if cfg.ai.provider.is_empty() {
        cfg.ai.provider = "gemini".to_string();
    }
    if cfg.ai.backend_url.is_empty() {
        cfg.ai.backend_url = "https://api.smart.moder42.tech".to_string();
    }
    if cfg.ai.openai_model.is_empty() {
        cfg.ai.openai_model = "gpt-4.1-mini".to_string();
    }
    if cfg.ai.gemini_model.is_empty() {
        cfg.ai.gemini_model = "gemini-2.0-flash".to_string();
    }

    if cfg.ai.openai_api_keys.is_empty() {
        if !cfg.ai.openai_api_key.is_empty() {
            cfg.ai.openai_api_keys.push(cfg.ai.openai_api_key.clone());
        } else {
            cfg.ai.openai_api_keys = vec![String::new()];
        }
    }
    if cfg.ai.gemini_api_keys.is_empty() {
        if !cfg.ai.gemini_api_key.is_empty() {
            cfg.ai.gemini_api_keys.push(cfg.ai.gemini_api_key.clone());
        } else {
            cfg.ai.gemini_api_keys = vec![String::new()];
        }
    }

    for links in cfg.servers.values_mut() {
        links.normalize();
    }
}

pub fn save_config(cfg: &Config) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(cfg)?;
    fs::write(&path, data)?;
    Ok(())
}

pub fn output_dir(cfg: &Config) -> PathBuf {
    if cfg.output_dir.trim().is_empty() {
        return app_config_dir();
    }
    expand_tilde(&cfg.output_dir)
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    Path::new(path).to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_config_defaults_promotes_legacy_keys_and_servers() {
        let mut cfg = Config::default();
        cfg.ai.openai_api_key = "legacy-openai".to_string();
        cfg.ai.gemini_api_key = "legacy-gemini".to_string();

        fill_config_defaults(&mut cfg);

        assert_eq!(cfg.ai.provider, "gemini");
        assert_eq!(cfg.ai.openai_model, "gpt-4.1-mini");
        assert_eq!(cfg.ai.gemini_model, "gemini-2.0-flash");
        assert_eq!(cfg.ai.backend_url, "https://api.smart.moder42.tech");
        assert_eq!(cfg.ai.openai_api_keys, vec!["legacy-openai".to_string()]);
        assert_eq!(cfg.ai.gemini_api_keys, vec!["legacy-gemini".to_string()]);
        assert!(cfg.servers.contains_key("1"));
        assert!(cfg.servers.contains_key("401"));
    }

    #[test]
    fn output_dir_uses_base_dir_when_empty() {
        let cfg = Config::default();
        assert_eq!(output_dir(&cfg), app_config_dir());
    }
}

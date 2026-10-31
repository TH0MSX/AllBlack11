//! Reading and writing `config.json` for every part of the app that needs it.

use crate::config::{bundled_default_json, parse_config, Config};

use super::wide::{config_path, write_atomic};

pub fn load() -> Config {
    if let Ok(text) = std::fs::read_to_string(config_path()) {
        if let Ok(config) = parse_config(&text) {
            return config;
        }
    }
    parse_config(bundled_default_json()).expect("bundled config")
}

pub fn store(config: &Config) -> Result<(), String> {
    let text = serde_json::to_string_pretty(config).map_err(|err| err.to_string())?;
    write_atomic(&config_path(), &format!("{text}\n"))
}

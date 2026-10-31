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

pub fn load_notebook() -> crate::Notebook {
    std::fs::read_to_string(super::wide::notebook_path())
        .ok()
        .and_then(|text| crate::Notebook::parse(&text).ok())
        .unwrap_or_default()
}

pub fn save_notebook(book: &crate::Notebook) -> Result<(), String> {
    write_atomic(
        &super::wide::notebook_path(),
        &format!("{}\n", book.to_json()),
    )
}

pub fn load_accents() -> crate::Accents {
    std::fs::read_to_string(super::wide::accents_path())
        .ok()
        .and_then(|text| crate::Accents::parse(&text).ok())
        .unwrap_or_default()
}

pub fn save_accents(accents: &crate::Accents) -> Result<(), String> {
    write_atomic(
        &super::wide::accents_path(),
        &format!("{}\n", accents.to_json()),
    )
}

pub fn load_lookbook() -> crate::LookBook {
    std::fs::read_to_string(super::wide::lookbook_path())
        .ok()
        .and_then(|text| crate::LookBook::parse(&text).ok())
        .unwrap_or_default()
}

pub fn save_lookbook(book: &crate::LookBook) -> Result<(), String> {
    write_atomic(
        &super::wide::lookbook_path(),
        &format!("{}\n", book.to_json()),
    )
}

/// Keep a copy of the config that last parsed and applied. Never overwrites a good copy with
/// identical text, so the file's age stays meaningful.
pub fn save_lkg(text: &str) {
    let path = super::wide::lkg_path();
    if std::fs::read_to_string(&path).is_ok_and(|existing| existing == text) {
        return;
    }
    if let Err(err) = write_atomic(&path, text) {
        super::wide::log_line("warn", &format!("could not save config.lkg.json: {err}"));
    }
}

pub fn load_lkg() -> Option<String> {
    std::fs::read_to_string(super::wide::lkg_path()).ok()
}

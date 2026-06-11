use crate::config;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunHistoryEntry {
    pub id: String,
    pub created_at: u64,
    pub user: String,
    pub server: String,
    pub mode: String,
    pub ai_provider: String,
    pub status: String,
    pub limit_charged: bool,
    pub file_saved: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiffReport {
    pub created_at: u64,
    pub server: String,
    pub mode: String,
    pub added: Vec<String>,
    pub changed: Vec<String>,
    pub removed: Vec<String>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn history_path() -> std::path::PathBuf {
    config::app_config_dir().join("history.json")
}

fn last_diff_path() -> std::path::PathBuf {
    config::app_config_dir().join("last_diff.json")
}

pub fn timestamp() -> String {
    now().to_string()
}

pub fn append(entry: RunHistoryEntry) -> Result<()> {
    let mut items = load().unwrap_or_default();
    items.insert(0, entry);
    items.truncate(25);

    let path = history_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(&items)?)?;
    Ok(())
}

pub fn load() -> Result<Vec<RunHistoryEntry>> {
    let path = history_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

pub fn save_diff(mut diff: DiffReport) -> Result<()> {
    diff.created_at = now();
    let path = last_diff_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(&diff)?)?;
    Ok(())
}

pub fn load_last_diff() -> Result<Option<DiffReport>> {
    let path = last_diff_path();
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw).ok())
}

pub fn entry(
    server: u32,
    mode: &str,
    provider: &str,
    status: &str,
    limit_charged: bool,
    file_saved: bool,
    message: impl Into<String>,
) -> RunHistoryEntry {
    let ts = now();
    RunHistoryEntry {
        id: format!("{}-{}-{}", ts, server, mode),
        created_at: ts,
        user: std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "local-user".to_string()),
        server: server.to_string(),
        mode: mode.to_uppercase(),
        ai_provider: provider.to_string(),
        status: status.to_string(),
        limit_charged,
        file_saved,
        message: message.into(),
    }
}

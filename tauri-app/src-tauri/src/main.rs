#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod types;
mod config;
mod forum;
mod ai;
mod merge;
mod update;

use types::{Config, ServerInfo};

#[tauri::command]
async fn load_config() -> Result<Config, String> {
    config::load_config().map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_config(config: Config) -> Result<(), String> {
    config::save_config(&config).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_config_path() -> Result<String, String> {
    Ok(config::config_path().to_string_lossy().to_string())
}

#[tauri::command]
async fn get_servers() -> Result<Vec<ServerInfo>, String> {
    Ok(config::get_servers())
}

#[tauri::command]
async fn run_update(
    window: tauri::Window,
    server_num: u32,
    mode: String,
    skip_login: bool,
) -> Result<(), String> {
    update::run_update(&window, server_num, mode, skip_login)
        .await
        .map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            load_config,
            save_config,
            get_config_path,
            get_servers,
            run_update
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

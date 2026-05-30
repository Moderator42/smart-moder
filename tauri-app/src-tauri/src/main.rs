#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod types;
mod config;
mod forum;
mod ai;
mod merge;
mod update;

use types::{Config, ServerInfo};

#[cfg(target_os = "linux")]
fn set_linux_runtime_env() {
    // Определяем сессию: Wayland или X11
    let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .unwrap_or_default()
            .to_lowercase()
            == "wayland";

    if is_wayland {
        // --- Wayland ---
        std::env::set_var("GDK_BACKEND", "wayland");
        std::env::set_var("EGL_PLATFORM", "wayland");
        // WebKit на Wayland: отключаем dmabuf-рендерер (он ломает EGL на многих дистрах)
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    } else {
        // --- X11 ---
        std::env::set_var("GDK_BACKEND", "x11");
        std::env::set_var("EGL_PLATFORM", "x11");
        std::env::set_var("GSK_RENDERER", "cairo");
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        // Программный рендеринг как fallback на X11 без нормального EGL
        std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");
        std::env::set_var("MESA_GL_VERSION_OVERRIDE", "3.3");
        std::env::set_var("MESA_GLSL_VERSION_OVERRIDE", "330");
    }

    // Общие фиксы для обеих сессий: убираем sandbox WebKit (причина EGL_BAD_PARAMETER)
    std::env::set_var("WEBKIT_FORCE_SANDBOX", "0");
    std::env::set_var("WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS", "1");
}

#[cfg(not(target_os = "linux"))]
fn set_linux_runtime_env() {}

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
    set_linux_runtime_env();

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

use crate::config;
use crate::forum;
use anyhow::{anyhow, Result};
use encoding_rs::WINDOWS_1251;
use reqwest::StatusCode;
use std::fs;
use std::io::Write;
use std::path::Path;
// use std::path::PathBuf;
use tauri::{Emitter, Window};

fn log(window: &Window, line: String) -> Result<()> {
    window.emit("log", line).map_err(|e| anyhow!("{e}"))
}

fn github_raw_url(mode: &str, server_num: u32) -> String {
    let num_str = if server_num < 100 { format!("{:02}", server_num) } else { server_num.to_string() };
    let base = "https://raw.githubusercontent.com/MTGMODS/arizona-helper/main";
    if mode == "uk" {
        format!("{}/SmartUK/{}/SmartUK.json", base, num_str)
    } else {
        format!("{}/SmartPDD/{}/SmartPDD.json", base, num_str)
    }
}

fn backup_file(path: &Path) -> Result<()> {
    if path.exists() {
        let backup = path.with_extension("backup.json");
        fs::copy(path, &backup)?;
    }
    Ok(())
}

fn write_cp1251(path: &Path, data: &str) -> Result<()> {
    let (encoded, _, _) = WINDOWS_1251.encode(data);
    let mut file = fs::File::create(path)?;
    file.write_all(&encoded)?;
    Ok(())
}

fn normalize_reason(reason: &str) -> String {
    reason.trim().to_uppercase().replace("  ", " ")
}

fn diff_summary(old: &[crate::types::Chapter], new: &[crate::types::Chapter], mode: &str) -> (Vec<String>, Vec<String>) {
    use std::collections::HashMap;

    let mut old_items: HashMap<String, crate::types::Item> = HashMap::new();
    let mut new_items: HashMap<String, crate::types::Item> = HashMap::new();

    for chapter in old {
        for item in &chapter.item {
            if !item.reason.is_empty() {
                old_items.insert(normalize_reason(&item.reason), item.clone());
            }
        }
    }
    for chapter in new {
        for item in &chapter.item {
            if !item.reason.is_empty() {
                new_items.insert(normalize_reason(&item.reason), item.clone());
            }
        }
    }

    let key = if mode == "uk" { "lvl" } else { "amount" };
    let mut added = Vec::new();
    let mut changed = Vec::new();

    for reason in new_items.keys() {
        if !old_items.contains_key(reason) {
            added.push(reason.clone());
        } else {
            let old_item = &old_items[reason];
            let new_item = &new_items[reason];
            let old_val = if key == "lvl" {
                old_item.lvl.clone().unwrap_or_default()
            } else {
                old_item.amount.clone().unwrap_or_default()
            };
            let new_val = if key == "lvl" {
                new_item.lvl.clone().unwrap_or_default()
            } else {
                new_item.amount.clone().unwrap_or_default()
            };
            if old_val.trim() != new_val.trim() {
                changed.push(reason.clone());
            }
        }
    }

    (added, changed)
}

fn format_changelog(mode: &str, added: &[String], changed: &[String], server_num: u32) -> String {
    let fmt = |lst: &[String]| -> String {
        lst.iter()
            .map(|r| {
                r.to_lowercase()
                    .replace("ук", "УК")
                    .replace("ак", "АК")
                    .replace("пдд", "ПДД")
            })
            .collect::<Vec<_>>()
            .join(", ")
    };

    if mode == "uk" {
        let mut lines = vec!["УК:".to_string(), "Проведена сверка УК с актуальной инфой,  ".to_string()];
        if !added.is_empty() {
            lines.push(format!("Добавлены пропущенные главы и статьи: {},  ", fmt(added)));
        }
        if !changed.is_empty() {
            lines.push(format!("Обновлены статьи: {},  ", fmt(changed)));
        }
        lines.push(format!("Убраны расхождения между кодом и актуальным УК {} сервера", server_num));
        lines.join("\n")
    } else {
        let mut lines = vec![
            "ДК/АК:".to_string(),
            "Проведена сверка ДК/АК с актуальной инфой,  ".to_string(),
            "ДК приведен только к штрафным статьям, убраны пункты без штрафов,  ".to_string(),
            "АК добавлен и приведен к актуальной редакции,  ".to_string(),
        ];
        if !added.is_empty() {
            lines.push(format!("Добавлены статьи: {},  ", fmt(added)));
        }
        if !changed.is_empty() {
            lines.push(format!("Исправлены суммы штрафов и тексты статей ({}),  ", fmt(&changed[..changed.len().min(10)])));
        }
        lines.push(format!("Убраны расхождения между кодом и актуальными ДК/АК {} сервера", server_num));
        lines.join("\n")
    }
}

pub async fn run_update(
    window: &Window,
    server_num: u32,
    mode: String,
    skip_login: bool,
) -> Result<()> {
    let cfg = config::load_config()?;

    if mode == "both" {
        log(window, format!("Start: server {server_num}, mode both"))?;
        run_update_mode(window, &cfg, server_num, "uk", skip_login).await?;
        run_update_mode(window, &cfg, server_num, "pdd", skip_login).await?;
        log(window, format!("Completed: server {server_num}, mode both"))?;
        return Ok(());
    }

    run_update_mode(window, &cfg, server_num, &mode, skip_login).await
}

async fn run_update_mode(
    window: &Window,
    cfg: &crate::types::Config,
    server_num: u32,
    mode: &str,
    skip_login: bool,
) -> Result<()> {
    let output_root = config::output_dir(cfg);
    let server_dir = output_root.join(server_num.to_string());
    fs::create_dir_all(&server_dir)?;

    log(window, format!("Start: server {server_num}, mode {mode}"))?;

    let srv_key = server_num.to_string();
    let links = cfg
        .servers
        .get(&srv_key)
        .ok_or_else(|| anyhow!("No server links for {}", srv_key))?
        .clone();

    let project = config::project_for_server(server_num);
    let (login, password) = if project == "rodina" {
        (&cfg.rodina.login, &cfg.rodina.password)
    } else {
        (&cfg.arizona.login, &cfg.arizona.password)
    };

    // Fetch forum text (static + headless fallback)
    log(window, "Fetching forum page...".to_string())?;
    let forum_text = match forum::fetch_with_fallback(&links, mode, login, password, skip_login).await {
        Ok(t) => t,
        Err(e) => {
            log(window, format!("Forum fetch failed: {e}"))?;
            String::new()
        }
    };

    let forum_path = server_dir.join(format!("forum_raw_{}.txt", mode));
    fs::write(&forum_path, forum_text.as_bytes())?;
    log(window, format!("Saved forum text -> {}", forum_path.display()))?;

    let url = github_raw_url(mode, server_num);
    log(window, format!("Downloading JSON from {}", url))?;
    match reqwest::get(&url).await {
        Ok(resp) => {
            if resp.status() == StatusCode::NOT_FOUND {
                log(window, format!("JSON not found on GitHub for server {server_num} ({mode})"))?;
            } else if resp.status().is_success() {
                let bytes = resp.bytes().await.unwrap_or_default();
                let decoded = String::from_utf8(bytes.to_vec())
                    .ok()
                    .or_else(|| {
                        let (cow, _, had_errors) = WINDOWS_1251.decode(&bytes);
                        if had_errors { None } else { Some(cow.to_string()) }
                    });

                if let Some(s) = decoded {
                    let filename = if mode == "uk" { "SmartUK.json" } else { "SmartPDD.json" };
                    let dest = server_dir.join(filename);
                    backup_file(&dest)?;
                    write_cp1251(&dest, &s)?;
                    log(window, format!("Downloaded JSON -> {}", dest.display()))?;

                    let original_chapters = serde_json::from_str::<Vec<crate::types::Chapter>>(&s).ok();

                    log(window, "Calling AI to generate updated JSON...".to_string())?;
                    let ai_response = match crate::ai::call_ai(&cfg.ai, mode, &forum_text, &s).await {
                        Ok(r) => r,
                        Err(e) => {
                            log(window, format!("AI error: {e}"))?;
                            String::new()
                        }
                    };

                    let mut updated_json: Option<Vec<crate::types::Chapter>> = None;

                    if !ai_response.is_empty() {
                        match crate::merge::parse_ai_json(&ai_response) {
                            Ok(parsed) => {
                                let processed = crate::merge::postprocess(mode, parsed, original_chapters.as_ref());
                                let mut processed = crate::merge::add_updated_at(processed);
                                crate::merge::sanitize_strings(&mut processed);
                                if let Ok(out_json) = crate::merge::serialize_output(&processed) {
                                    backup_file(&dest)?;
                                    write_cp1251(&dest, &out_json)?;
                                    log(window, format!("Saved merged JSON -> {}", dest.display()))?;
                                    updated_json = Some(processed);
                                }
                            }
                            Err(e) => {
                                log(window, format!("AI returned invalid JSON: {e}"))?;
                            }
                        }
                    }

                    if let Some(updated) = updated_json {
                        if let Some(original) = original_chapters.as_ref() {
                            let (added, changed) = diff_summary(original, &updated, mode);
                            log(window, format!("Новых статей: +{}", added.len()))?;
                            if !added.is_empty() {
                                log(window, format!("-> {}", added.iter().take(20).cloned().collect::<Vec<_>>().join(", ")))?;
                            }
                            log(window, format!("Обновлено значений: {}", changed.len()))?;
                            if !changed.is_empty() {
                                log(window, format!("-> {}", changed.iter().take(20).cloned().collect::<Vec<_>>().join(", ")))?;
                            }

                            let changelog = format_changelog(mode, &added, &changed, server_num);
                            let cl_path = output_root.join(format!("changelog_{}_{}.txt", server_num, mode));
                            fs::write(&cl_path, format!("{}\n", changelog))?;
                            log(window, format!("Changelog -> {}", cl_path.display()))?;

                            log(window, "Verifying saved file...".to_string())?;
                            if let Ok(saved_raw) = fs::read(&dest) {
                                let saved_text = String::from_utf8(saved_raw.clone())
                                    .ok()
                                    .or_else(|| {
                                        let (cow, _, had_errors) = WINDOWS_1251.decode(&saved_raw);
                                        if had_errors { None } else { Some(cow.to_string()) }
                                    });
                                if let Some(text) = saved_text {
                                    if let Ok(saved_data) = serde_json::from_str::<Vec<crate::types::Chapter>>(&text) {
                                        let has_updated_at = saved_data.iter().any(|e| e.name == "##updated_at");
                                        if has_updated_at {
                                            log(window, "##updated_at присутствует".to_string())?;
                                        } else {
                                            log(window, "##updated_at не найден".to_string())?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    log(window, "Downloaded JSON but failed decode as UTF-8".to_string())?;
                }
            } else {
                log(window, format!("Failed to download JSON: {}", resp.status()))?;
            }
        }
        Err(e) => {
            log(window, format!("HTTP error while downloading JSON: {e}"))?;
        }
    }

    log(window, format!("Completed: server {server_num}, mode {mode}"))?;
    Ok(())
}

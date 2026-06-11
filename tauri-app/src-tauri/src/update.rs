use crate::config;
use crate::forum;
use anyhow::{anyhow, Result};
use encoding_rs::WINDOWS_1251;
use reqwest::StatusCode;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, Window};

fn log(window: &Window, line: String) -> Result<()> {
    window.emit("log", line).map_err(|e| anyhow!("{e}"))
}

fn github_raw_url(mode: &str, server_num: u32) -> String {
    let num_str = if server_num < 100 {
        format!("{:02}", server_num)
    } else {
        server_num.to_string()
    };
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

        let backup_dir = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("backups");
        fs::create_dir_all(&backup_dir)?;
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("SmartConfig");
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("json");
        let stamped = backup_dir.join(format!("{}_{}.{}", stem, crate::history::timestamp(), ext));
        fs::copy(path, stamped)?;
    }
    Ok(())
}

fn write_cp1251(path: &Path, data: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let (encoded, _, _) = WINDOWS_1251.encode(data);
    let mut file = fs::File::create(path)?;
    file.write_all(&encoded)?;
    Ok(())
}

fn pending_dir() -> PathBuf {
    config::app_config_dir().join("pending")
}

fn pending_path(server_num: u32, mode: &str) -> PathBuf {
    pending_dir().join(format!("{}_{}.json", server_num, mode.to_lowercase()))
}

fn remove_pending_quiet(server_num: u32, mode: &str) {
    if mode.eq_ignore_ascii_case("both") {
        remove_pending_quiet(server_num, "uk");
        remove_pending_quiet(server_num, "pdd");
        return;
    }
    let _ = fs::remove_file(pending_path(server_num, mode));
}

fn final_dest(cfg: &crate::types::Config, server_num: u32, mode: &str) -> PathBuf {
    let filename = if mode.eq_ignore_ascii_case("uk") { "SmartUK.json" } else { "SmartPDD.json" };
    config::output_dir(cfg).join(server_num.to_string()).join(filename)
}

fn normalize_reason(reason: &str) -> String {
    reason.trim().to_uppercase().replace("  ", " ")
}

fn diff_summary(
    old: &[crate::types::Chapter],
    new: &[crate::types::Chapter],
    mode: &str,
) -> (Vec<String>, Vec<String>, Vec<String>) {
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
    let mut removed = Vec::new();

    for reason in old_items.keys() {
        if !new_items.contains_key(reason) {
            removed.push(reason.clone());
        }
    }

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

    (added, changed, removed)
}

fn fmt_reasons(lst: &[String]) -> String {
    lst.iter()
        .map(|r| {
            let lower = r.to_lowercase();
            lower
                .replace("ук", "УК")
                .replace("ак", "АК")
                .replace("пдд", "ПДД")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_changelog(mode: &str, added: &[String], changed: &[String], server_num: u32) -> String {
    if mode == "uk" {
        let mut lines = vec![
            "УК:".to_string(),
            "Проведена сверка УК с актуальной инфой,  ".to_string(),
        ];
        if !added.is_empty() {
            lines.push(format!(
                "Добавлены пропущенные главы и статьи: {},  ",
                fmt_reasons(added)
            ));
        }
        if !changed.is_empty() {
            lines.push(format!("Обновлены статьи: {},  ", fmt_reasons(changed)));
        }
        lines.push(format!(
            "Убраны расхождения между кодом и актуальным УК {} сервера",
            server_num
        ));
        lines.join("\n")
    } else {
        let mut lines = vec![
            "ДК/АК:".to_string(),
            "Проведена сверка ДК/АК с актуальной инфой,  ".to_string(),
            "ДК приведен только к штрафным статьям, убраны пункты без штрафов,  ".to_string(),
            "АК добавлен и приведен к актуальной редакции,  ".to_string(),
        ];
        if !added.is_empty() {
            lines.push(format!("Добавлены статьи: {},  ", fmt_reasons(added)));
        }
        if !changed.is_empty() {
            lines.push(format!(
                "Исправлены суммы штрафов и тексты статей ({}),  ",
                fmt_reasons(&changed[..changed.len().min(10)])
            ));
        }
        lines.push(format!(
            "Убраны расхождения между кодом и актуальными ДК/АК {} сервера",
            server_num
        ));
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
    let app = window.app_handle().clone();

    if mode == "both" {
        log(window, format!("Start: server {server_num}, mode both"))?;

        if let Err(e) = run_update_mode(window, &app, &cfg, server_num, "uk", skip_login).await {
            remove_pending_quiet(server_num, "both");
            return Err(e);
        }
        let uk_diff = crate::history::load_last_diff().ok().flatten();

        if let Err(e) = run_update_mode(window, &app, &cfg, server_num, "pdd", skip_login).await {
            // UK+PDD is one user scenario: if the second part fails, do not leave partial pending JSON.
            remove_pending_quiet(server_num, "both");
            return Err(e);
        }
        let pdd_diff = crate::history::load_last_diff().ok().flatten();

        let mut combined = crate::history::DiffReport {
            created_at: 0,
            server: server_num.to_string(),
            mode: "BOTH".to_string(),
            added: Vec::new(),
            changed: Vec::new(),
            removed: Vec::new(),
        };
        if let Some(d) = uk_diff {
            combined.added.extend(d.added.into_iter().map(|x| format!("UK: {x}")));
            combined.changed.extend(d.changed.into_iter().map(|x| format!("UK: {x}")));
            combined.removed.extend(d.removed.into_iter().map(|x| format!("UK: {x}")));
        }
        if let Some(d) = pdd_diff {
            combined.added.extend(d.added.into_iter().map(|x| format!("PDD: {x}")));
            combined.changed.extend(d.changed.into_iter().map(|x| format!("PDD: {x}")));
            combined.removed.extend(d.removed.into_iter().map(|x| format!("PDD: {x}")));
        }
        let _ = crate::history::save_diff(combined);

        log(window, format!("Completed: server {server_num}, mode both"))?;
        return Ok(());
    }

    run_update_mode(window, &app, &cfg, server_num, &mode, skip_login).await
}

async fn run_update_mode(
    window: &Window,
    app: &AppHandle,
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

    // ── Check that forum URL is configured ──────────────────────────────────
    let has_url = !links.urls_for(mode).is_empty();
    if !has_url {
        log(window, format!("⚠ Нет ссылки на форум для сервера {server_num} / режим {mode} — пропускаю"))?;
        log(window, format!("  Открой вкладку «Серверы», введи ссылку и сохрани конфиг"))?;
        let _ = crate::history::append(crate::history::entry(server_num, mode, &cfg.ai.provider, "failed", false, false, "Нет ссылки на форум"));
        return Err(anyhow!("Нет ссылки на форум для сервера {server_num} / {mode}"));
    }

    // ── Fetch forum text ────────────────────────────────────────────────────
    log(window, "Fetching forum page...".to_string())?;
    let forum_text = match forum::fetch_with_fallback(
        app, &links, mode, login, password, skip_login,
    )
    .await
    {
        Ok(t) => {
            log(window, format!("  Спарсено {} символов", t.len()))?;
            t
        }
        Err(e) => {
            log(window, format!("Forum fetch failed: {e}"))?;
            let _ = crate::history::append(crate::history::entry(server_num, mode, &cfg.ai.provider, "failed", false, false, format!("Forum fetch failed: {e}")));
            return Err(anyhow!("Forum fetch failed: {e}"));
        }
    };

    // Save raw forum text for debugging
    let forum_path = server_dir.join(format!("forum_raw_{}.txt", mode));
    if let Err(e) = fs::write(&forum_path, forum_text.as_bytes()) {
        log(window, format!("⚠ Не удалось сохранить forum_raw: {e}"))?;
    } else {
        log(window, format!("Saved forum text -> {}", forum_path.display()))?;
    }

    // ── Guard: abort if forum text is too short ─────────────────────────────
    if forum_text.trim().len() < 200 {
        log(window, "⚠ Текст форума слишком короткий — возможно, не удалось авторизоваться".to_string())?;
        log(window, "  Проверь логин/пароль или включи «Пропустить логин» если форум открыт без авторизации".to_string())?;
        log(window, "  AI не вызывается — файл не изменён".to_string())?;
        let _ = crate::history::append(crate::history::entry(server_num, mode, &cfg.ai.provider, "failed", false, false, "Текст форума слишком короткий / страница не соответствует"));
        return Err(anyhow!("Текст форума слишком короткий / страница не соответствует"));
    }

    // ── Download base JSON from GitHub ──────────────────────────────────────
    let url = github_raw_url(mode, server_num);
    log(window, format!("Downloading JSON from {}", url))?;

    let resp = match reqwest::get(&url).await {
        Ok(r) => r,
        Err(e) => {
            log(window, format!("HTTP error while downloading JSON: {e}"))?;
            log(window, format!("Completed: server {server_num}, mode {mode}"))?;
            return Ok(());
        }
    };

    if resp.status() == StatusCode::NOT_FOUND {
        log(window, format!("JSON not found on GitHub for server {server_num} ({mode})"))?;
        log(window, format!("Completed: server {server_num}, mode {mode}"))?;
        return Ok(());
    }

    if !resp.status().is_success() {
        log(window, format!("Failed to download JSON: {}", resp.status()))?;
        log(window, format!("Completed: server {server_num}, mode {mode}"))?;
        return Ok(());
    }

    let bytes = resp.bytes().await.unwrap_or_default();
    let decoded = String::from_utf8(bytes.to_vec()).ok().or_else(|| {
        let (cow, _, had_errors) = WINDOWS_1251.decode(&bytes);
        if had_errors { None } else { Some(cow.to_string()) }
    });

    let json_str = match decoded {
        Some(s) => s,
        None => {
            log(window, "Downloaded JSON but failed to decode (not UTF-8 or cp1251)".to_string())?;
            log(window, format!("Completed: server {server_num}, mode {mode}"))?;
            return Ok(());
        }
    };

    log(window, format!("Downloaded base JSON for diff: {} bytes", json_str.len()))?;

    let original_chapters = serde_json::from_str::<Vec<crate::types::Chapter>>(&json_str).ok();

    // ── Call AI ─────────────────────────────────────────────────────────────
    log(window, "Calling AI to generate updated JSON...".to_string())?;
    let ai_response = match crate::ai::call_ai(&cfg.ai, mode, &forum_text, &json_str).await {
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
                let processed =
                    crate::merge::postprocess(mode, parsed, original_chapters.as_ref());

                // ── Sanity check: reject suspiciously empty results ──────────
                let item_count: usize = processed.iter().map(|c| c.item.len()).sum();
                let original_count: usize = original_chapters
                    .as_ref()
                    .map(|o| o.iter().map(|c| c.item.len()).sum())
                    .unwrap_or(0);

                if item_count == 0 {
                    log(window, "⚠ AI вернул пустой результат — сохранение отменено, файл не изменён".to_string())?;
                } else if original_count > 0 && item_count < original_count / 3 {
                    log(window, format!(
                        "⚠ AI вернул подозрительно мало статей ({} из {}), сохранение отменено",
                        item_count, original_count
                    ))?;
                } else {
                    let mut processed = crate::merge::add_updated_at(processed);
                    crate::merge::sanitize_strings(&mut processed);
                    if let Ok(out_json) = crate::merge::serialize_output(&processed) {
                        let p = pending_path(server_num, mode);
                        if let Some(parent) = p.parent() { fs::create_dir_all(parent)?; }
                        fs::write(&p, out_json.as_bytes())?;
                        log(window, format!("Pending JSON создан -> {}", p.display()))?;
                        log(window, "Открой вкладку Diff и нажми «Сохранить» или «Отменить»".to_string())?;
                        updated_json = Some(processed);
                    }
                }
            }
            Err(e) => {
                log(window, format!("AI returned invalid JSON: {e}"))?;
            }
        }
    }

    // ── Diff + changelog ────────────────────────────────────────────────────
    if let Some(updated) = updated_json.as_ref() {
        if let Some(original) = original_chapters.as_ref() {
            let (added, changed, removed) = diff_summary(original, updated, mode);
            log(window, format!("Новых статей: +{}", added.len()))?;
            if !added.is_empty() {
                log(window, format!("  -> {}", added.iter().take(20).cloned().collect::<Vec<_>>().join(", ")))?;
            }
            log(window, format!("Обновлено значений: {}", changed.len()))?;
            if !changed.is_empty() {
                log(window, format!("  -> {}", changed.iter().take(20).cloned().collect::<Vec<_>>().join(", ")))?;
            }
            log(window, format!("Удалено статей: -{}", removed.len()))?;
            if !removed.is_empty() {
                log(window, format!("  -> {}", removed.iter().take(20).cloned().collect::<Vec<_>>().join(", ")))?;
            }
            let _ = crate::history::save_diff(crate::history::DiffReport {
                created_at: 0,
                server: server_num.to_string(),
                mode: mode.to_uppercase(),
                added: added.clone(),
                changed: changed.clone(),
                removed: removed.clone(),
            });

            let changelog = format_changelog(mode, &added, &changed, server_num);
            let cl_path = output_root.join(format!("changelog_{}_{}.txt", server_num, mode));
            fs::write(&cl_path, format!("{}\n", changelog))?;
            log(window, format!("Changelog -> {}", cl_path.display()))?;
        }

        // ── Verify pending file ───────────────────────────────────────────────
        log(window, "Verifying pending JSON...".to_string())?;
        if let Ok(saved_raw) = fs::read(pending_path(server_num, mode)) {
            let text = String::from_utf8_lossy(&saved_raw);
            if let Ok(saved_data) = serde_json::from_str::<Vec<crate::types::Chapter>>(&text) {
                let has_ts = saved_data.iter().any(|e| e.name == "##updated_at");
                if has_ts {
                    log(window, "##updated_at присутствует".to_string())?;
                } else {
                    log(window, "##updated_at не найден".to_string())?;
                }
            }
        }
    }

    let saved = updated_json.is_some();
    if saved {
        let _ = crate::history::append(crate::history::entry(server_num, mode, &cfg.ai.provider, "pending", false, false, "JSON создан и ожидает подтверждения на Diff-экране"));
    } else {
        let _ = crate::history::append(crate::history::entry(server_num, mode, &cfg.ai.provider, "failed", false, false, "JSON не был создан"));
        return Err(anyhow!("JSON не был создан"));
    }

    log(window, format!("Completed: server {server_num}, mode {mode}"))?;
    Ok(())
}

pub fn apply_pending(server_num: u32, mode: &str) -> Result<()> {
    let cfg = config::load_config()?;
    if mode == "both" {
        apply_pending(server_num, "uk")?;
        apply_pending(server_num, "pdd")?;
        return Ok(());
    }
    let p = pending_path(server_num, mode);
    if !p.exists() {
        return Err(anyhow!("Pending JSON не найден для сервера {} / {}", server_num, mode));
    }
    let data = fs::read_to_string(&p)?;
    // Final JSON validation before writing cp1251 file.
    let parsed: Vec<crate::types::Chapter> = serde_json::from_str(&data)
        .map_err(|e| anyhow!("Pending JSON invalid: {e}"))?;
    if parsed.iter().map(|c| c.item.len()).sum::<usize>() == 0 {
        return Err(anyhow!("Pending JSON пустой — сохранение отменено"));
    }
    let dest = final_dest(&cfg, server_num, mode);
    backup_file(&dest)?;
    write_cp1251(&dest, &data)?;
    let _ = fs::remove_file(&p);
    let charged = cfg.ai.provider.to_lowercase() != "gemini";
    let _ = crate::history::append(crate::history::entry(
        server_num,
        mode,
        &cfg.ai.provider,
        "success",
        charged,
        true,
        "JSON сохранён после подтверждения diff",
    ));
    Ok(())
}

pub fn cancel_pending(server_num: u32, mode: &str) -> Result<()> {
    let cfg = config::load_config()?;
    if mode == "both" {
        cancel_pending(server_num, "uk")?;
        cancel_pending(server_num, "pdd")?;
        return Ok(());
    }
    let p = pending_path(server_num, mode);
    let _ = fs::remove_file(p);
    let _ = crate::history::append(crate::history::entry(
        server_num,
        mode,
        &cfg.ai.provider,
        "cancelled",
        false,
        false,
        "Пользователь отменил сохранение на diff-экране",
    ));
    Ok(())
}

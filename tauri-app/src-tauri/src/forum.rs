use anyhow::{anyhow, Result};
use reqwest::Client;
use scraper::{ElementRef, Html, Selector};
use std::process::Stdio;
use std::time::Duration;

use crate::types::ServerLinks;

/// Try to fetch forum HTML with a static HTTP client. If it looks empty or incomplete,
/// optionally fall back to a headless Chromium binary (if available) to dump the rendered DOM.
pub async fn fetch_with_fallback(
    links: &ServerLinks,
    mode: &str,
    login: &str,
    password: &str,
    skip_login: bool,
) -> Result<String> {
    let urls = match mode {
        "uk" => vec![&links.forum_uk_url, &links.forum_uk_url_2],
        "pdd" => vec![&links.forum_pdd_url, &links.forum_pdd_url_2],
        _ => return Err(anyhow!("Unknown mode: {}", mode)),
    };

    let mut parts: Vec<String> = Vec::new();

    for u in urls.iter().filter(|s| !s.trim().is_empty()) {
        // Try static fetch first
        if let Ok(txt) = fetch_forum_text(u, login, password, skip_login).await {
            if looks_like_valid_text(&txt) {
                parts.push(txt);
                continue;
            }
        }

        // Fallback to headless dump
        if let Ok(rendered) = run_headless_dump(u).await {
            let text = extract_forum_text(&rendered);
            if looks_like_valid_text(&text) {
                parts.push(text);
            }
        }
    }

    if parts.is_empty() {
        Err(anyhow!("All fetch attempts failed for mode {}", mode))
    } else {
        Ok(parts.join("\n\n"))
    }
}

async fn fetch_forum_text(
    url: &str,
    _login: &str,
    _password: &str,
    _skip_login: bool,
) -> Result<String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(75))
        .cookie_store(true)
        .build()?;

    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(anyhow!("Forum fetch failed: {} {}", resp.status(), url));
    }

    let html = resp.text().await.unwrap_or_default();
    Ok(extract_forum_text(&html))
}

pub fn extract_forum_text(html: &str) -> String {
    let doc = Html::parse_document(html);
    let selectors = [
        ".message-body .bbWrapper",
        ".messageText",
        ".p-body-main",
        "article",
    ];

    for selector in selectors.iter() {
        if let Ok(sel) = Selector::parse(selector) {
            let mut blocks = Vec::new();
            for element in doc.select(&sel) {
                let block = normalize_block(&element);
                if block.len() > 50 {
                    blocks.push(block);
                }
            }
            if !blocks.is_empty() {
                return blocks.join("\n\n");
            }
        }
    }

    if let Ok(body_sel) = Selector::parse("body") {
        if let Some(body) = doc.select(&body_sel).next() {
            let text = normalize_block(&body);
            if !text.is_empty() {
                return text;
            }
        }
    }

    normalize_whitespace(&doc.root_element().text().collect::<Vec<_>>().join(" "))
}

fn normalize_block(element: &ElementRef<'_>) -> String {
    normalize_whitespace(&element.text().collect::<Vec<_>>().join(" "))
}

fn normalize_whitespace(text: &str) -> String {
    let mut out = String::new();
    let mut prev_space = false;

    for ch in text.replace('\u{a0}', " ").chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }

    out.trim().to_string()
}

fn looks_like_valid_text(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.len() < 100 {
        return false;
    }
    let lines = trimmed.lines().filter(|line| !line.trim().is_empty()).count();
    lines >= 2 || trimmed.contains("Глава") || trimmed.contains("Статья")
}

async fn run_headless_dump(url: &str) -> Result<String> {
    // Try several known browser binaries that support --headless and --dump-dom
    let candidates = ["chromium", "chromium-browser", "google-chrome", "chrome", "brave-browser"];

    for cmd in &candidates {
        // Try newer headless flag first
        let args_sets = vec![vec!["--headless=new", "--disable-gpu", "--dump-dom", url], vec!["--headless", "--disable-gpu", "--dump-dom", url]];
        for args in args_sets {
            match tokio::process::Command::new(cmd)
                .args(&args)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    let output = child.wait_with_output().await;
                    if let Ok(out) = output {
                        if out.status.success() {
                            if let Ok(s) = String::from_utf8(out.stdout) {
                                return Ok(s);
                            }
                        }
                    }
                }
                Err(_) => continue,
            }
        }
    }

    Err(anyhow!("No headless browser available or dump failed"))
}

#[cfg(test)]
mod tests {
        use super::*;

        #[test]
        fn extract_forum_text_prefers_message_body() {
                let html = r#"
                        <html>
                            <body>
                                <div class="message-body"><div class="bbWrapper">
                                    Глава 1. Общая часть.
                                    <p>1.1 УК — Нападение на гражданское лицо.</p>
                                    <p>1.2 УК — Незаконное хранение оружия.</p>
                                </div></div>
                            </body>
                        </html>
                "#;

                let text = extract_forum_text(html);
                assert!(text.contains("Глава 1. Общая часть."));
                assert!(text.contains("1.1 УК"));
                assert!(text.contains("Нападение на гражданское лицо."));
        }

        #[test]
        fn looks_like_valid_text_rejects_short_pages() {
                assert!(!looks_like_valid_text("short"));
                assert!(looks_like_valid_text(&"Глава 1. ".repeat(40)));
        }
}

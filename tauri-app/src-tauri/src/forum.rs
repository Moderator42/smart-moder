use anyhow::{anyhow, Result};
use reqwest::header::{self, HeaderMap, HeaderValue};
use reqwest::{Client, ClientBuilder};
use scraper::{ElementRef, Html, Selector};
use std::time::Duration;

use crate::types::ServerLinks;

/// Fetch forum text with proper XenForo cookie-based auth, anti-bot headers,
/// and optional headless Chrome fallback.
pub async fn fetch_with_fallback(
    links: &ServerLinks,
    mode: &str,
    login: &str,
    password: &str,
    skip_login: bool,
) -> Result<String> {
    let urls: Vec<&str> = match mode {
        "uk"  => vec![links.forum_uk_url.as_str(),  links.forum_uk_url_2.as_str()],
        "pdd" => vec![links.forum_pdd_url.as_str(), links.forum_pdd_url_2.as_str()],
        _     => return Err(anyhow!("Unknown mode: {}", mode)),
    };

    let client = build_client()?;
    let mut parts: Vec<String> = Vec::new();

    for url in urls.iter().filter(|s| !s.trim().is_empty()) {
        // ── Step 1: try authenticated static fetch ──────────────────────────
        let text = fetch_authenticated(&client, url, login, password, skip_login).await;
        match text {
            Ok(t) if looks_like_valid_text(&t) => {
                parts.push(t);
                continue;
            }
            Ok(t) => {
                eprintln!("  Static fetch returned too-short text ({} chars), trying headless...", t.len());
            }
            Err(e) => {
                eprintln!("  Static fetch error: {e}, trying headless...");
            }
        }

        // ── Step 2: headless Chrome fallback ────────────────────────────────
        if let Ok(rendered) = run_headless_dump(url).await {
            let extracted = extract_forum_text(&rendered);
            if looks_like_valid_text(&extracted) {
                parts.push(extracted);
                continue;
            }
        }

        eprintln!("  All fetch attempts failed for {url}");
    }

    if parts.is_empty() {
        Err(anyhow!("All fetch attempts failed for mode {}", mode))
    } else {
        Ok(parts.join("\n\n"))
    }
}

/// Build reqwest client with cookie store + realistic browser headers.
fn build_client() -> Result<Client> {
    let mut default_headers = HeaderMap::new();
    default_headers.insert(
        header::USER_AGENT,
        HeaderValue::from_static(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
             AppleWebKit/537.36 (KHTML, like Gecko) \
             Chrome/124.0.0.0 Safari/537.36",
        ),
    );
    default_headers.insert(
        header::ACCEPT,
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
        ),
    );
    default_headers.insert(
        header::ACCEPT_LANGUAGE,
        HeaderValue::from_static("ru-RU,ru;q=0.9,en-US;q=0.8,en;q=0.7"),
    );
    default_headers.insert(
        header::ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, deflate, br"),
    );
    default_headers.insert(
        "Sec-Fetch-Dest",
        HeaderValue::from_static("document"),
    );
    default_headers.insert(
        "Sec-Fetch-Mode",
        HeaderValue::from_static("navigate"),
    );
    default_headers.insert(
        "Sec-Fetch-Site",
        HeaderValue::from_static("none"),
    );
    default_headers.insert(
        "Upgrade-Insecure-Requests",
        HeaderValue::from_static("1"),
    );

    ClientBuilder::new()
        .timeout(Duration::from_secs(90))
        .connect_timeout(Duration::from_secs(30))
        .cookie_store(true)
        .default_headers(default_headers)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| anyhow!("Failed to build HTTP client: {e}"))
}

/// Determine login URL based on forum domain.
fn login_url_for(thread_url: &str) -> &'static str {
    if thread_url.contains("rodina-rp") || thread_url.contains("rodina") {
        "https://forum.rodina-rp.com/login/"
    } else {
        "https://forum.arizona-rp.com/login/"
    }
}

/// Fetch a XenForo thread with cookie-based authentication.
///
/// Flow:
///   1. GET /login/ — grab `_xfToken` from the form (CSRF)
///   2. POST /login/login — submit credentials, follow redirect, store cookies
///   3. GET <thread_url> — now authenticated, parse content
async fn fetch_authenticated(
    client: &Client,
    url: &str,
    login: &str,
    password: &str,
    skip_login: bool,
) -> Result<String> {

    if !skip_login && !login.is_empty() && !password.is_empty() {
        let login_page_url = login_url_for(url);

        // ── 1. GET login page to obtain CSRF token ──────────────────────────
        let login_page = client
            .get(login_page_url)
            .send()
            .await
            .map_err(|e| anyhow!("GET login page failed: {e}"))?;

        if !login_page.status().is_success() {
            return Err(anyhow!("Login page returned {}", login_page.status()));
        }

        let login_html = login_page.text().await.unwrap_or_default();
        let xf_token = extract_xf_token(&login_html);

        // ── 2. POST credentials ─────────────────────────────────────────────
        let mut form = vec![
            ("login",    login),
            ("password", password),
            ("remember", "1"),
        ];
        let token_str;
        if let Some(ref tok) = xf_token {
            token_str = tok.clone();
            form.push(("_xfToken", token_str.as_str()));
        }

        let post_url = format!("{}login", login_page_url);
        let resp = client
            .post(&post_url)
            .header(header::REFERER, login_page_url)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .form(&form)
            .send()
            .await
            .map_err(|e| anyhow!("POST login failed: {e}"))?;

        if !resp.status().is_success() && resp.status().as_u16() != 303 {
            // 303 redirect after login is normal
            return Err(anyhow!("Login POST returned {}", resp.status()));
        }

        // Consume body so redirect chain is followed
        let _ = resp.text().await;

        // Small pause to mimic human behaviour
        tokio::time::sleep(Duration::from_millis(800)).await;
    }

    // ── 3. GET the actual forum thread ──────────────────────────────────────
    let resp = client
        .get(url)
        .header(header::REFERER, login_url_for(url))
        .send()
        .await
        .map_err(|e| anyhow!("GET thread failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(anyhow!("Thread GET returned {}", resp.status()));
    }

    let html = resp.text().await.unwrap_or_default();
    let text = extract_forum_text(&html);

    // Debug: if text looks short, log the start of the HTML for diagnosis
    if text.len() < 200 {
        let preview: String = html.chars().take(500).collect();
        eprintln!("  ⚠ Short text ({} chars). HTML preview:\n{}", text.len(), preview);
    }

    Ok(text)
}

/// Extract `_xfToken` value from XenForo login page HTML.
fn extract_xf_token(html: &str) -> Option<String> {
    // XenForo 2: <input type="hidden" name="_xfToken" value="TOKEN">
    let doc = Html::parse_document(html);
    if let Ok(sel) = Selector::parse("input[name=\"_xfToken\"]") {
        if let Some(el) = doc.select(&sel).next() {
            if let Some(val) = el.value().attr("value") {
                return Some(val.to_string());
            }
        }
    }

    // Fallback: regex-like scan
    if let Some(start) = html.find("name=\"_xfToken\"") {
        let slice = &html[start..];
        if let Some(v_start) = slice.find("value=\"") {
            let after = &slice[v_start + 7..];
            if let Some(end) = after.find('"') {
                return Some(after[..end].to_string());
            }
        }
    }

    None
}

pub fn extract_forum_text(html: &str) -> String {
    let doc = Html::parse_document(html);

    // Try XenForo-specific selectors first, then generic fallbacks
    let selectors = [
        ".message-body .bbWrapper",
        ".messageText",
        ".p-body-main",
        "article",
        ".bbWrapper",
        ".message-userContent",
    ];

    for selector in selectors.iter() {
        if let Ok(sel) = Selector::parse(selector) {
            let blocks: Vec<String> = doc
                .select(&sel)
                .map(|el| normalize_block(&el))
                .filter(|b| b.len() > 50)
                .collect();

            if !blocks.is_empty() {
                return blocks.join("\n\n");
            }
        }
    }

    // Last resort: whole body
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
    let mut out = String::with_capacity(text.len());
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
    if trimmed.len() < 200 {
        return false;
    }
    let nonempty_lines = trimmed.lines().filter(|l| !l.trim().is_empty()).count();
    nonempty_lines >= 3 || trimmed.contains("Глава") || trimmed.contains("Статья")
}

async fn run_headless_dump(url: &str) -> Result<String> {
    let candidates = [
        "chromium", "chromium-browser", "google-chrome", "chrome", "brave-browser",
    ];

    for cmd in &candidates {
        for args in &[
            vec!["--headless=new", "--disable-gpu", "--dump-dom", "--no-sandbox", url],
            vec!["--headless",     "--disable-gpu", "--dump-dom", "--no-sandbox", url],
        ] {
            if let Ok(child) = tokio::process::Command::new(cmd)
                .args(args)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                if let Ok(out) = child.wait_with_output().await {
                    if out.status.success() {
                        if let Ok(s) = String::from_utf8(out.stdout) {
                            return Ok(s);
                        }
                    }
                }
            }
        }
    }

    Err(anyhow!("No headless browser available"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_forum_text_prefers_message_body() {
        let html = r#"
            <html><body>
              <div class="message-body"><div class="bbWrapper">
                Глава 1. Общая часть.
                <p>1.1 УК — Нападение на гражданское лицо.</p>
                <p>1.2 УК — Незаконное хранение оружия.</p>
              </div></div>
            </body></html>
        "#;
        let text = extract_forum_text(html);
        assert!(text.contains("Глава 1. Общая часть."));
        assert!(text.contains("1.1 УК"));
    }

    #[test]
    fn extract_xf_token_finds_hidden_field() {
        let html = r#"<form><input type="hidden" name="_xfToken" value="abc123"></form>"#;
        assert_eq!(extract_xf_token(html), Some("abc123".to_string()));
    }

    #[test]
    fn looks_like_valid_text_rejects_short() {
        assert!(!looks_like_valid_text("short"));
        assert!(looks_like_valid_text(&"Глава 1. ".repeat(50)));
    }
}

//! Forum fetching via Tauri WebviewWindow (primary) with reqwest fallback.
//!
//! How it works:
//!   1. Create an invisible WebviewWindow pointed at `about:blank`.
//!   2. An initialization script runs on every page load inside that window.
//!      - On the login page: fills credentials, submits the form.
//!      - After redirect (any non-login, non-thread page): navigates to thread.
//!      - On the thread page: scrolls, extracts text, then navigates to a
//!        special sentinel URL:  `https://tauri-forum-result.invalid/?ok=<text>`
//!        or `https://tauri-forum-result.invalid/?err=<msg>`.
//!   3. Rust watches navigation events on the window. When the sentinel domain
//!      appears in a `navigation` event, it reads the query string and closes the window.
//!   4. If WebView fails (creation error, timeout, etc.), fall back to reqwest.

use anyhow::{anyhow, Result};
use reqwest::header::{self, HeaderMap, HeaderValue};
use reqwest::{Client, ClientBuilder};
use scraper::{ElementRef, Html, Selector};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Listener, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::types::ServerLinks;

// ── Sentinel domain used to pass results back from JS ─────────────────────
const SENTINEL: &str = "tauri-forum-result.invalid";

// ── Public entry point ──────────────────────────────────────────────────────

pub async fn fetch_with_fallback(
    app: &AppHandle,
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
    let urls: Vec<&str> = urls.into_iter().filter(|s| !s.trim().is_empty()).collect();
    if urls.is_empty() {
        return Err(anyhow!("No forum URLs configured for mode {}", mode));
    }

    let mut parts: Vec<String> = Vec::new();

    for url in urls {
        eprintln!("  [forum] Fetching: {url}");

        // ── Primary: WebView ────────────────────────────────────────────────
        let webview_result = fetch_via_webview(app, url, login, password, skip_login).await;

        match webview_result {
            Ok(ref text) if looks_like_valid_text(text) => {
                eprintln!("  [forum] WebView OK: {} chars", text.len());
                parts.push(webview_result.unwrap());
                continue;
            }
            Ok(ref text) => {
                eprintln!("  [forum] WebView returned short text ({} chars), trying reqwest...", text.len());
            }
            Err(ref e) => {
                eprintln!("  [forum] WebView failed: {e}, trying reqwest...");
            }
        }

        // ── Fallback: reqwest ───────────────────────────────────────────────
        let client = match build_client() {
            Ok(c) => c,
            Err(e) => { eprintln!("  [forum] reqwest client error: {e}"); continue; }
        };
        match fetch_authenticated(&client, url, login, password, skip_login).await {
            Ok(text) if looks_like_valid_text(&text) => {
                eprintln!("  [forum] reqwest OK: {} chars", text.len());
                parts.push(text);
            }
            Ok(text) if !text.is_empty() => {
                eprintln!("  [forum] reqwest short ({} chars), using anyway", text.len());
                parts.push(text);
            }
            Ok(_) => { eprintln!("  [forum] reqwest returned empty"); }
            Err(e) => { eprintln!("  [forum] reqwest failed: {e}"); }
        }
    }

    if parts.is_empty() {
        Err(anyhow!("All fetch attempts failed for mode {}", mode))
    } else {
        Ok(parts.join("\n\n"))
    }
}

// ── WebView fetch ───────────────────────────────────────────────────────────

async fn fetch_via_webview(
    app: &AppHandle,
    thread_url: &str,
    login: &str,
    password: &str,
    skip_login: bool,
) -> Result<String> {
    let label = format!("forum-fetch-{}", uuid_short());
    let done_event = format!("forum-nav-{}", label);

    // Shared result slot
    let slot: Arc<Mutex<Option<Result<String>>>> = Arc::new(Mutex::new(None));
    let slot_listener = slot.clone();

    // Listen for navigation events emitted by the window
    let done_event_clone = done_event.clone();
    let _listener = app.listen(done_event_clone, move |event| {
        let raw = event.payload();
        eprintln!("  [forum] event payload ({} bytes): {:?}", raw.len(), &raw[..raw.len().min(200)]);

        // Tauri 2 wraps the payload in an extra JSON string layer sometimes.
        // Try direct parse first, then unwrap one string layer.
        let v: serde_json::Value = serde_json::from_str(raw)
            .or_else(|_| {
                // Maybe payload is a JSON-quoted string: "\"...\""
                serde_json::from_str::<String>(raw)
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s))
            })
            .unwrap_or(serde_json::Value::Null);

        let result = if let Some(b64) = v.get("ok_b64").and_then(|s| s.as_str()) {
            // Base64-encoded text path
            let bytes = base64_decode(b64);
            match String::from_utf8(bytes) {
                Ok(text) => Ok(text),
                Err(e) => Err(anyhow!("base64 decode utf8: {e}")),
            }
        } else if let Some(text) = v.get("ok").and_then(|s| s.as_str()) {
            Ok(text.to_string())
        } else if let Some(msg) = v.get("err").and_then(|s| s.as_str()) {
            Err(anyhow!("{}", msg))
        } else {
            eprintln!("  [forum] unrecognized payload structure: {:?}", v);
            Err(anyhow!("bad event payload"))
        };
        *slot_listener.lock().unwrap() = Some(result);
    });

    let login_url = login_url_for(thread_url);
    let js = build_init_script(login_url, thread_url, login, password, skip_login, &done_event);

    // Build invisible window starting at login page (or thread directly if skip_login)
    let start_url = if skip_login {
        thread_url.to_string()
    } else {
        login_url.to_string()
    };

    let window = WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::External(start_url.parse().map_err(|e| anyhow!("bad url: {e}"))?),
    )
    .title("Forum Fetch")
    .visible(false)
    .skip_taskbar(true)
    .inner_size(1280.0, 900.0)
    .initialization_script(&js)
    .build()
    .map_err(|e| anyhow!("WebviewWindow create failed: {e}"))?;

    // Poll for result with 90s timeout
    let start = std::time::Instant::now();
    loop {
        tokio::time::sleep(Duration::from_millis(400)).await;
        {
            let guard = slot.lock().unwrap();
            if let Some(ref res) = *guard {
                let _ = window.close();
                return match res {
                    Ok(t)  => Ok(t.clone()),
                    Err(e) => Err(anyhow!("{e}")),
                };
            }
        }
        if start.elapsed() > Duration::from_secs(90) {
            let _ = window.close();
            return Err(anyhow!("WebView forum fetch timed out"));
        }
    }
}

// ── Init script injected into every page in the WebviewWindow ──────────────

fn build_init_script(
    login_url: &str,
    thread_url: &str,
    login: &str,
    password: &str,
    skip_login: bool,
    done_event: &str,
) -> String {
    let login_url_js  = js_escape(login_url);
    let thread_url_js = js_escape(thread_url);
    let login_js      = js_escape(login);
    let password_js   = js_escape(password);
    let done_event_js = js_escape(done_event);
    let skip_js       = if skip_login { "true" } else { "false" };

    // We send results by emitting a Tauri event via __TAURI_INTERNALS__
    // which is always available inside a Tauri WebviewWindow (including external URLs).
    format!(r#"
(function() {{
  'use strict';

  const LOGIN_URL    = "{login_url_js}";
  const THREAD_URL   = "{thread_url_js}";
  const LOGIN_VAL    = "{login_js}";
  const PASS_VAL     = "{password_js}";
  const DONE_EVENT   = "{done_event_js}";
  const SKIP_LOGIN   = {skip_js};

  function sleep(ms) {{ return new Promise(r => setTimeout(r, ms)); }}

  // Encode text as base64 (handles Unicode/Cyrillic safely)
  function toBase64(str) {{
    try {{
      return btoa(unescape(encodeURIComponent(str)));
    }} catch(e) {{
      // Fallback: send as-is if btoa fails
      return str;
    }}
  }}

  // Send result back to Rust via Tauri IPC
  async function sendResult(obj) {{
    // Use base64 for text to avoid encoding issues over IPC
    const payload = obj.ok !== undefined
      ? JSON.stringify({{ ok_b64: toBase64(obj.ok) }})
      : JSON.stringify(obj);
    try {{
      // Tauri 2 internal IPC — always available in any WebviewWindow
      await window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {{
        event: DONE_EVENT,
        payload: payload,
        windowLabel: null,
        target: {{ kind: 'App' }}
      }});
    }} catch(e1) {{
      // Fallback: try the older path
      try {{
        window.__TAURI_INTERNALS__.postMessage({{
          cmd: 'emit',
          event: DONE_EVENT,
          payload: payload
        }});
      }} catch(e2) {{
        console.error('[forum-fetch] sendResult failed:', e1, e2);
      }}
    }}
  }}

  async function scrollFull() {{
    let prev = -1;
    for (let i = 0; i < 25; i++) {{
      window.scrollBy(0, 2500);
      await sleep(300);
      const h = document.body.scrollHeight;
      if (h === prev) break;
      prev = h;
    }}
    window.scrollTo(0, 0);
    await sleep(500);
  }}

  function extractText() {{
    const selectors = [
      '.message-body .bbWrapper',
      '.messageText',
      '.p-body-main',
      'article',
      '.bbWrapper',
      '.message-userContent',
    ];
    for (const sel of selectors) {{
      const els = [...document.querySelectorAll(sel)];
      if (els.length > 0) {{
        const parts = els
          .map(el => (el.innerText || el.textContent || '').trim())
          .filter(t => t.length > 50);
        if (parts.length > 0) return parts.join('\n\n');
      }}
    }}
    return (document.body.innerText || document.body.textContent || '').trim();
  }}

  async function doLogin() {{
    for (let attempt = 0; attempt < 40; attempt++) {{
      const li = document.querySelector('input[name="login"]');
      const pi = document.querySelector('input[name="password"]');
      const sb = document.querySelector('button[type="submit"], input[type="submit"]');
      if (li && pi && sb) {{
        // Suppress webdriver fingerprint
        try {{ Object.defineProperty(navigator, 'webdriver', {{ get: () => undefined }}); }} catch(_) {{}}
        li.value = LOGIN_VAL;
        li.dispatchEvent(new Event('input', {{ bubbles: true }}));
        li.dispatchEvent(new Event('change', {{ bubbles: true }}));
        await sleep(200);
        pi.value = PASS_VAL;
        pi.dispatchEvent(new Event('input', {{ bubbles: true }}));
        pi.dispatchEvent(new Event('change', {{ bubbles: true }}));
        await sleep(200);
        sb.click();
        return;
      }}
      await sleep(300);
    }}
    // Login form not found — navigate straight to thread
    window.location.href = THREAD_URL;
  }}

  async function doFetchThread() {{
    await scrollFull();
    const text = extractText();
    await sendResult({{ ok: text }});
  }}

  async function main() {{
    try {{
      const href = window.location.href;
      const isLoginPage   = href.includes('/login') && !href.includes(THREAD_URL);
      const isThreadPage  = href.startsWith(THREAD_URL.split('?')[0].replace(/\/+$/, ''));

      if (SKIP_LOGIN || isThreadPage) {{
        // Already on thread (or skip_login) — just extract
        await doFetchThread();
        return;
      }}

      if (isLoginPage || href === 'about:blank' || href === '') {{
        await doLogin();
        // Navigation will happen; init script re-runs on the next page
        return;
      }}

      // Intermediate page after login redirect — go to thread
      window.location.href = THREAD_URL;

    }} catch(err) {{
      await sendResult({{ err: String(err) }});
    }}
  }}

  if (document.readyState === 'loading') {{
    document.addEventListener('DOMContentLoaded', main);
  }} else {{
    setTimeout(main, 0);
  }}
}})();
"#)
}

// ── reqwest fallback ────────────────────────────────────────────────────────

fn build_client() -> Result<Client> {
    let mut h = HeaderMap::new();
    h.insert(header::USER_AGENT,             HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36"));
    h.insert(header::ACCEPT,                 HeaderValue::from_static("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"));
    h.insert(header::ACCEPT_LANGUAGE,        HeaderValue::from_static("ru-RU,ru;q=0.9,en-US;q=0.8,en;q=0.7"));
    h.insert(header::ACCEPT_ENCODING,        HeaderValue::from_static("gzip, deflate, br"));
    h.insert("Sec-Fetch-Dest",               HeaderValue::from_static("document"));
    h.insert("Sec-Fetch-Mode",               HeaderValue::from_static("navigate"));
    h.insert("Sec-Fetch-Site",               HeaderValue::from_static("none"));
    h.insert("Upgrade-Insecure-Requests",    HeaderValue::from_static("1"));
    ClientBuilder::new()
        .timeout(Duration::from_secs(90))
        .connect_timeout(Duration::from_secs(30))
        .cookie_store(true)
        .default_headers(h)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| anyhow!("reqwest client: {e}"))
}

fn login_url_for(thread_url: &str) -> &'static str {
    if thread_url.contains("rodina-rp") || thread_url.contains("rodina") {
        "https://forum.rodina-rp.com/login/"
    } else {
        "https://forum.arizona-rp.com/login/"
    }
}

async fn fetch_authenticated(
    client: &Client,
    url: &str,
    login: &str,
    password: &str,
    skip_login: bool,
) -> Result<String> {
    if !skip_login && !login.is_empty() && !password.is_empty() {
        let login_page_url = login_url_for(url);
        let page_resp = client.get(login_page_url).send().await
            .map_err(|e| anyhow!("GET login: {e}"))?;
        if !page_resp.status().is_success() {
            return Err(anyhow!("login page status {}", page_resp.status()));
        }
        let page_html = page_resp.text().await.unwrap_or_default();
        let xf_token  = extract_xf_token(&page_html);

        let mut form = vec![
            ("login",       login),
            ("password",    password),
            ("remember",    "1"),
            ("_xfRedirect", "/"),
        ];
        let tok_str;
        if let Some(ref t) = xf_token {
            tok_str = t.clone();
            form.push(("_xfToken", tok_str.as_str()));
        }

        let post_url = format!("{}login", login_page_url);
        let resp = client.post(&post_url)
            .header(header::REFERER, login_page_url)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .form(&form)
            .send().await
            .map_err(|e| anyhow!("POST login: {e}"))?;
        if !resp.status().is_success() && resp.status().as_u16() != 303 {
            return Err(anyhow!("login POST {}", resp.status()));
        }
        let _ = resp.text().await;
        tokio::time::sleep(Duration::from_millis(800)).await;
    }

    let resp = client.get(url)
        .header(header::REFERER, login_url_for(url))
        .send().await
        .map_err(|e| anyhow!("GET thread: {e}"))?;
    if !resp.status().is_success() {
        return Err(anyhow!("thread GET {}", resp.status()));
    }
    let html = resp.text().await.unwrap_or_default();
    let text = extract_forum_text(&html);
    if text.len() < 200 {
        eprintln!("  [forum] reqwest short ({} chars), HTML preview: {}", text.len(),
                  html.chars().take(400).collect::<String>());
    }
    Ok(text)
}

// ── HTML text extraction ────────────────────────────────────────────────────

fn extract_xf_token(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    if let Ok(sel) = Selector::parse("input[name=\"_xfToken\"]") {
        if let Some(el) = doc.select(&sel).next() {
            if let Some(v) = el.value().attr("value") { return Some(v.to_string()); }
        }
    }
    if let Some(start) = html.find("name=\"_xfToken\"") {
        if let Some(vs) = html[start..].find("value=\"") {
            let after = &html[start + vs + 7..];
            if let Some(end) = after.find('"') { return Some(after[..end].to_string()); }
        }
    }
    None
}

pub fn extract_forum_text(html: &str) -> String {
    let doc = Html::parse_document(html);
    for selector in &[
        ".message-body .bbWrapper", ".messageText", ".p-body-main",
        "article", ".bbWrapper", ".message-userContent",
    ] {
        if let Ok(sel) = Selector::parse(selector) {
            let blocks: Vec<String> = doc.select(&sel)
                .map(|el| normalize_block(&el))
                .filter(|b| b.len() > 50)
                .collect();
            if !blocks.is_empty() { return blocks.join("\n\n"); }
        }
    }
    if let Ok(sel) = Selector::parse("body") {
        if let Some(b) = doc.select(&sel).next() {
            let t = normalize_block(&b);
            if !t.is_empty() { return t; }
        }
    }
    normalize_whitespace(&doc.root_element().text().collect::<Vec<_>>().join(" "))
}

fn normalize_block(el: &ElementRef<'_>) -> String {
    normalize_whitespace(&el.text().collect::<Vec<_>>().join(" "))
}

fn normalize_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut sp = false;
    for ch in text.replace('\u{a0}', " ").chars() {
        if ch.is_whitespace() { if !sp { out.push(' '); sp = true; } }
        else { out.push(ch); sp = false; }
    }
    out.trim().to_string()
}

fn looks_like_valid_text(s: &str) -> bool {
    let t = s.trim();
    if t.len() < 200 { return false; }
    t.lines().filter(|l| !l.trim().is_empty()).count() >= 3
        || t.contains("Глава") || t.contains("Статья")
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn js_escape(s: &str) -> String {
    s.chars().flat_map(|c| match c {
        '"'  => vec!['\\', '"'],
        '\'' => vec!['\\', '\''],
        '\\' => vec!['\\', '\\'],
        '\n' => vec!['\\', 'n'],
        '\r' => vec!['\\', 'r'],
        '\t' => vec!['\\', 't'],
        c    => vec![c],
    }).collect()
}

/// Minimal Base64 decoder (standard alphabet, no external deps).
fn base64_decode(s: &str) -> Vec<u8> {
    let table: [u8; 128] = {
        let mut t = [255u8; 128];
        for (i, &c) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/".iter().enumerate() {
            t[c as usize] = i as u8;
        }
        t
    };
    let bytes: Vec<u8> = s.bytes().filter(|&b| b != b'=' && (b as usize) < 128 && table[b as usize] != 255).collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let b: Vec<u8> = chunk.iter().map(|&b| table[b as usize]).collect();
        if b.len() >= 2 { out.push((b[0] << 2) | (b[1] >> 4)); }
        if b.len() >= 3 { out.push((b[1] << 4) | (b[2] >> 2)); }
        if b.len() >= 4 { out.push((b[2] << 6) | b[3]); }
    }
    out
}


    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("{:x}{:x}", t.as_secs(), t.subsec_nanos())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_forum_text_prefers_message_body() {
        let html = r#"<html><body>
            <div class="message-body"><div class="bbWrapper">
                Глава 1. Общая часть.
                <p>1.1 УК — Нападение на гражданское лицо.</p>
                <p>1.2 УК — Незаконное хранение оружия.</p>
            </div></div></body></html>"#;
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

    #[test]
    fn js_escape_special_chars() {
        assert_eq!(js_escape(r#"a "b" c"#), r#"a \"b\" c"#);
        assert_eq!(js_escape("line\nnew"),  r"line\nnew");
    }
}

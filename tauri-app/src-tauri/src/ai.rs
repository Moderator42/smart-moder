use crate::types::AiConfig;
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

const FILL_RULES_UK: &str = r#"""
=== ТВОЯ ЗАДАЧА ===
Ты получаешь текст с форума и текущий JSON (SmartUK.json).
Твоя задача: обновить значения lvl в существующих записях и добавить новые статьи/главы если они появились на форуме.

=== СТРУКТУРА ФАЙЛА ===
[
    {
        "name": "Глава X. Название главы.",
        "item": [
            {
                "lvl": "2",
                "reason": "X.Y УК",
                "text": "Краткое описание нарушения."
            }
        ]
    }
]

... (правила сокращены для компактности, полный промпт будет передан)
""#;

const FILL_RULES_PDD: &str = r#"""
=== ТВОЯ ЗАДАЧА ===
Ты получаешь текст с форума и текущий JSON (SmartPDD.json).
Твоя задача: обновить значения amount в существующих записях и добавить новые статьи/главы если они появились на форуме.

... (правила сокращены для компактности)
""#;

pub async fn call_ai(
    cfg: &AiConfig,
    mode: &str,
    forum_text: &str,
    original_json: &str,
) -> Result<String> {
    let provider = cfg.provider.as_str();
    // select key
    let (key, model) = if provider == "openai" {
        let k = cfg
            .openai_api_keys
            .iter()
            .find(|k| !k.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| cfg.openai_api_key.clone());
        if k.trim().is_empty() {
            return Err(anyhow!("OpenAI API key not set"));
        }
        (k, cfg.openai_model.clone())
    } else if provider == "gemini" {
        let k = cfg
            .gemini_api_keys
            .iter()
            .find(|k| !k.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| cfg.gemini_api_key.clone());
        if k.trim().is_empty() {
            return Err(anyhow!("Gemini API key not set"));
        }
        (k, cfg.gemini_model.clone())
    } else {
        return Err(anyhow!("Unknown AI provider: {}", cfg.provider));
    };

    // Build prompts
    let system = if mode == "uk" { FILL_RULES_UK } else { FILL_RULES_PDD };
    let user = format!(
        "=== ТЕКСТ С ФОРУМА ===\n{}\n\n=== ТЕКУЩИЙ JSON ===\n{}\n",
        forum_text,
        original_json
    );

    // Try several times with simple backoff.
    let mut last_error: Option<anyhow::Error> = None;
    for attempt in 0..3u32 {
        match if provider == "gemini" {
            call_gemini(&key, &model, system, &user).await
        } else {
            call_openai(&key, &model, system, &user).await
        } {
            Ok(content) => return Ok(content),
            Err(err) => {
                last_error = Some(err);
                if attempt < 2 {
                    tokio::time::sleep(Duration::from_secs((attempt + 1) as u64)).await;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("AI request failed")))
}

async fn call_openai(key: &str, model: &str, system: &str, user: &str) -> Result<String> {
    let client = Client::builder().timeout(Duration::from_secs(120)).build()?;
    let url = "https://api.openai.com/v1/chat/completions";
    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "temperature": 0.0,
        "max_tokens": 12000
    });

    let resp = client
        .post(url)
        .bearer_auth(key)
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow!("OpenAI request error: {e}"))?;

    let status = resp.status();
    let v: Value = resp.json().await.map_err(|e| anyhow!("Invalid OpenAI response: {e}"))?;
    if !status.is_success() {
        return Err(anyhow!("OpenAI error: {}", v));
    }

    if let Some(content) = v
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|ch| ch.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|s| s.as_str())
    {
        return Ok(content.to_string());
    }

    Err(anyhow!("OpenAI: no content in response"))
}

async fn call_gemini(key: &str, model: &str, system: &str, user: &str) -> Result<String> {
    let client = Client::builder().timeout(Duration::from_secs(120)).build()?;
    let model = model.trim();
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, key
    );
    let body = serde_json::json!({
        "systemInstruction": {"parts": [{"text": system}]},
        "contents": [{"parts": [{"text": user}]}],
        "generationConfig": {"temperature": 0.0, "maxOutputTokens": 12000}
    });

    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow!("Gemini request error: {e}"))?;

    let status = resp.status();
    let v: Value = resp.json().await.map_err(|e| anyhow!("Invalid Gemini response: {e}"))?;
    if !status.is_success() {
        return Err(anyhow!("Gemini error: {}", v));
    }

    if let Some(content) = v
        .get("candidates")
        .and_then(|c| c.get(0))
        .and_then(|cand| cand.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(|parts| parts.get(0))
        .and_then(|part| part.get("text"))
        .and_then(|s| s.as_str())
    {
        return Ok(content.to_string());
    }

    Err(anyhow!("Gemini: no content in response"))
}

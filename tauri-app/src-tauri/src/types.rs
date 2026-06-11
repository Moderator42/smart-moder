use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub arizona: LoginConfig,
    #[serde(default)]
    pub rodina: LoginConfig,
    #[serde(default)]
    pub servers: HashMap<String, ServerLinks>,
    #[serde(default)]
    pub output_dir: String,
    #[serde(default)]
    pub server_delay: u32,
    #[serde(default)]
    pub ai: AiConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            arizona: LoginConfig::default(),
            rodina: LoginConfig::default(),
            servers: HashMap::new(),
            output_dir: String::new(),
            server_delay: 0,
            ai: AiConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoginConfig {
    #[serde(default)]
    pub login: String,
    #[serde(default)]
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerLinks {
    /// New flexible format: any number of UK URLs.
    #[serde(default)]
    pub forum_uk_urls: Vec<String>,
    /// New flexible format: any number of PDD URLs.
    #[serde(default)]
    pub forum_pdd_urls: Vec<String>,

    /// Legacy fields kept for compatibility with old config.json files.
    #[serde(default)]
    pub forum_uk_url: String,
    #[serde(default)]
    pub forum_uk_url_2: String,
    #[serde(default)]
    pub forum_pdd_url: String,
    #[serde(default)]
    pub forum_pdd_url_2: String,
}

impl ServerLinks {
    pub fn normalize(&mut self) {
        if self.forum_uk_urls.is_empty() {
            self.forum_uk_urls = vec![self.forum_uk_url.clone(), self.forum_uk_url_2.clone()]
                .into_iter()
                .filter(|s| !s.trim().is_empty())
                .collect();
        }
        if self.forum_pdd_urls.is_empty() {
            self.forum_pdd_urls = vec![self.forum_pdd_url.clone(), self.forum_pdd_url_2.clone()]
                .into_iter()
                .filter(|s| !s.trim().is_empty())
                .collect();
        }
        self.forum_uk_url = self.forum_uk_urls.get(0).cloned().unwrap_or_default();
        self.forum_uk_url_2 = self.forum_uk_urls.get(1).cloned().unwrap_or_default();
        self.forum_pdd_url = self.forum_pdd_urls.get(0).cloned().unwrap_or_default();
        self.forum_pdd_url_2 = self.forum_pdd_urls.get(1).cloned().unwrap_or_default();
    }

    pub fn urls_for(&self, mode: &str) -> Vec<String> {
        let mut clone = self.clone();
        clone.normalize();
        match mode {
            "uk" | "UK" => clone.forum_uk_urls,
            "pdd" | "PDD" => clone.forum_pdd_urls,
            _ => Vec::new(),
        }
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiConfig {
    #[serde(default)]
    pub provider: String,
    #[serde(default = "default_backend_url")]
    pub backend_url: String,
    #[serde(default)]
    pub backend_token: String,
    #[serde(default)]
    pub openai_api_keys: Vec<String>,
    #[serde(default)]
    pub openai_api_key: String,
    #[serde(default)]
    pub openai_model: String,
    #[serde(default)]
    pub gemini_api_keys: Vec<String>,
    #[serde(default)]
    pub gemini_api_key: String,
    #[serde(default)]
    pub gemini_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub item: Vec<Item>,
    #[serde(default)]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub lvl: Option<String>,
    #[serde(default)]
    pub amount: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerInfo {
    pub id: u32,
    pub project: String,
}

fn default_backend_url() -> String {
    "https://api.smart.moder42.tech".to_string()
}

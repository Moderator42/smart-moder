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
    #[serde(default)]
    pub forum_uk_url: String,
    #[serde(default)]
    pub forum_uk_url_2: String,
    #[serde(default)]
    pub forum_pdd_url: String,
    #[serde(default)]
    pub forum_pdd_url_2: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiConfig {
    #[serde(default)]
    pub provider: String,
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

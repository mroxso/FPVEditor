//! Configurable OpenAI-compatible endpoint settings (PLAN.md section 4.1):
//! base URL + API key + model name, so this works against OpenAI, Azure
//! OpenAI, Ollama, LM Studio, vLLM, etc.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub extra_headers: HashMap<String, String>,
}

impl ProviderConfig {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: None,
            model: model.into(),
            extra_headers: HashMap::new(),
        }
    }

    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }
}

/// Common presets for local/self-hosted OpenAI-compatible servers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Preset {
    OpenAi,
    Ollama,
    LmStudio,
    Custom,
}

impl Preset {
    pub fn default_base_url(self) -> &'static str {
        match self {
            Preset::OpenAi => "https://api.openai.com/v1",
            Preset::Ollama => "http://localhost:11434/v1",
            Preset::LmStudio => "http://localhost:1234/v1",
            Preset::Custom => "",
        }
    }

    pub fn config_with_model(self, model: impl Into<String>) -> ProviderConfig {
        ProviderConfig::new(self.default_base_url(), model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ollama_preset_points_at_the_local_openai_compatible_port() {
        let cfg = Preset::Ollama.config_with_model("llama3");
        assert_eq!(cfg.base_url, "http://localhost:11434/v1");
        assert_eq!(cfg.model, "llama3");
        assert!(cfg.api_key.is_none());
    }

    #[test]
    fn with_api_key_sets_the_key() {
        let cfg = ProviderConfig::new("http://localhost:1234/v1", "local-model")
            .with_api_key("sk-test");
        assert_eq!(cfg.api_key.as_deref(), Some("sk-test"));
    }

    #[test]
    fn config_round_trips_through_json() {
        let cfg = ProviderConfig::new("http://x", "m").with_api_key("k");
        let json = serde_json::to_string(&cfg).unwrap();
        let back: ProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, back);
    }
}

use chat_prompts::MergeRagContextPolicy;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub rag: RagConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_info_push_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_health_push_url: Option<String>,
}
impl Config {
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let config = config::Config::builder()
            .add_source(config::File::with_name(path.as_ref().to_str().unwrap()))
            .build()?;
        Ok(config.try_deserialize::<Self>()?)
    }
}

// Add Default implementation for Config
impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 8080,
            },
            rag: RagConfig {
                enable: false,
                prompt: None,
                rag_policy: MergeRagContextPolicy::SystemMessage,
                context_window: 1,
                vector_db: VectorDbConfig {
                    url: "http://localhost:6333".to_string(),
                    collection_name: vec!["default".to_string()],
                    limit: 1,
                    score_threshold: 0.5,
                },
                kw_search: KwSearchConfig::default(),
            },
            server_info_push_url: None,
            server_health_push_url: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Serialize, Clone)]
pub struct RagConfig {
    pub enable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    pub rag_policy: MergeRagContextPolicy,
    pub context_window: u64,
    pub vector_db: VectorDbConfig,
    pub kw_search: KwSearchConfig,
}

impl<'de> Deserialize<'de> for RagConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RagConfigHelper {
            prompt: String,
            rag_policy: String,
            context_window: u64,
            vector_db: VectorDbConfig,
            kw_search: KwSearchConfig,
        }

        let helper = RagConfigHelper::deserialize(deserializer)?;

        let prompt = if helper.prompt.is_empty() {
            None
        } else {
            Some(helper.prompt)
        };

        let rag_policy = MergeRagContextPolicy::from_str(&helper.rag_policy, true)
            .map_err(|e| serde::de::Error::custom(e.to_string()))?;

        Ok(RagConfig {
            enable: false,
            prompt,
            rag_policy,
            context_window: helper.context_window,
            vector_db: helper.vector_db,
            kw_search: helper.kw_search,
        })
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VectorDbConfig {
    pub url: String,
    pub collection_name: Vec<String>,
    pub limit: u64,
    pub score_threshold: f32,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct KwSearchConfig {
    pub enable: bool,
    pub url: String,
    pub index_name: String,
}

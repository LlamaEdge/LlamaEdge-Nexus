use crate::server::ServerId;
use chat_prompts::PromptTemplateType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct ServerInfo {
    #[serde(rename = "servers", skip_serializing_if = "HashMap::is_empty")]
    pub(crate) servers: HashMap<ServerId, ApiServer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ApiServer {
    #[serde(rename = "type")]
    pub(crate) ty: String,
    pub(crate) version: String,
    #[serde(rename = "plugin_version", skip_serializing_if = "Option::is_none")]
    pub(crate) plugin_version: Option<String>,
    pub(crate) port: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) chat_model: Option<ModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) embedding_model: Option<ModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) image_model: Option<ModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tts_model: Option<ModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) translate_model: Option<ModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) transcribe_model: Option<ModelConfig>,
    pub(crate) extras: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) server_id: Option<ServerId>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ModelConfig {
    // model name
    name: String,
    // type: chat or embedding
    #[serde(rename = "type")]
    ty: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctx_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ubatch_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_template: Option<PromptTemplateType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_predict: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_gpu_layers: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_mmap: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main_gpu: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tensor_split: Option<String>,
}

impl Serialize for ModelConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        // Count how many fields we'll actually serialize
        let mut field_count = 2; // name and ty are always serialized

        // Count optional fields that are Some
        if self.ctx_size.is_some() {
            field_count += 1;
        }
        if self.batch_size.is_some() {
            field_count += 1;
        }
        if self.ubatch_size.is_some() {
            field_count += 1;
        }
        if self.prompt_template.is_some() {
            field_count += 1;
        }
        if self.n_predict.is_some() {
            field_count += 1;
        }
        if self.reverse_prompt.is_some() {
            field_count += 1;
        }
        if self.n_gpu_layers.is_some() {
            field_count += 1;
        }
        if self.use_mmap.is_some() {
            field_count += 1;
        }
        if self.temperature.is_some() {
            field_count += 1;
        }
        if self.top_p.is_some() {
            field_count += 1;
        }
        if self.repeat_penalty.is_some() {
            field_count += 1;
        }
        if self.presence_penalty.is_some() {
            field_count += 1;
        }
        if self.frequency_penalty.is_some() {
            field_count += 1;
        }
        if self.split_mode.is_some() {
            field_count += 1;
        }
        if self.main_gpu.is_some() {
            field_count += 1;
        }
        if self.tensor_split.is_some() {
            field_count += 1;
        }

        // Create a map with the calculated size
        let mut map = serializer.serialize_map(Some(field_count))?;

        // Always serialize name and ty (type)
        map.serialize_entry("name", &self.name)?;
        map.serialize_entry("type", &self.ty)?;

        // Only serialize optional fields if they are Some
        if let Some(value) = &self.ctx_size {
            map.serialize_entry("ctx_size", value)?;
        }
        if let Some(value) = &self.batch_size {
            map.serialize_entry("batch_size", value)?;
        }
        if let Some(value) = &self.ubatch_size {
            map.serialize_entry("ubatch_size", value)?;
        }
        if let Some(value) = &self.prompt_template {
            map.serialize_entry("prompt_template", &value.to_string())?;
        }
        if let Some(value) = &self.n_predict {
            map.serialize_entry("n_predict", value)?;
        }
        if let Some(value) = &self.reverse_prompt {
            map.serialize_entry("reverse_prompt", value)?;
        }
        if let Some(value) = &self.n_gpu_layers {
            map.serialize_entry("n_gpu_layers", value)?;
        }
        if let Some(value) = &self.use_mmap {
            map.serialize_entry("use_mmap", value)?;
        }
        if let Some(value) = &self.temperature {
            map.serialize_entry("temperature", value)?;
        }
        if let Some(value) = &self.top_p {
            map.serialize_entry("top_p", value)?;
        }
        if let Some(value) = &self.repeat_penalty {
            map.serialize_entry("repeat_penalty", value)?;
        }
        if let Some(value) = &self.presence_penalty {
            map.serialize_entry("presence_penalty", value)?;
        }
        if let Some(value) = &self.frequency_penalty {
            map.serialize_entry("frequency_penalty", value)?;
        }
        if let Some(value) = &self.split_mode {
            map.serialize_entry("split_mode", value)?;
        }
        if let Some(value) = &self.main_gpu {
            map.serialize_entry("main_gpu", value)?;
        }
        if let Some(value) = &self.tensor_split {
            map.serialize_entry("tensor_split", value)?;
        }

        map.end()
    }
}

#[test]
fn test_deserialize_api_server() {
    let s = r#"{"type":"sd","version":"0.2.4","plugin_version":"Unknown","port":"12345","image_model":{"name":"sd-v1.5","type":"image","ctx_size":0,"batch_size":0,"ubatch_size":0},"extras":{}}"#;
    let server: ApiServer = serde_json::from_str(s).unwrap();
    assert_eq!(server.ty, "sd");
    assert_eq!(server.version, "0.2.4");
    assert_eq!(server.plugin_version, Some("Unknown".to_string()));
    assert_eq!(server.port, "12345");
    let image_model = server.image_model.unwrap();
    assert_eq!(image_model.name, "sd-v1.5");
    assert_eq!(image_model.ty, "image");
    assert_eq!(server.extras, HashMap::new());
}

use crate::config::context_size::ContextSize;
use crate::model::Model;
use crate::services::vram_estimator::{KvQuant, estimate_memory};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Display;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use thiserror::Error;
use tracing::{error, info};

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct DraftModelConfig {
    pub file: String,
    pub cache_type_k: CacheQuantType,
    pub cache_type_v: CacheQuantType,
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "lowercase")]
pub enum CacheQuantType {
    F32,
    #[default]
    F16,
    Bf16,
    Q8_0,
    Q4_0,
    Q4_1,
    Iq4Nl,
    Q5_0,
    Q5_1,
}

impl FromStr for CacheQuantType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "f32" => Ok(Self::F32),
            "f16" => Ok(Self::F16),
            "bf16" => Ok(Self::Bf16),
            "q8_0" => Ok(Self::Q8_0),
            "q4_0" => Ok(Self::Q4_0),
            "q4_1" => Ok(Self::Q4_1),
            "iq4_nl" => Ok(Self::Iq4Nl),
            "q5_0" => Ok(Self::Q5_0),
            "q5_1" => Ok(Self::Q5_1),
            _ => Err(format!("Invalid cache type: {}", s)),
        }
    }
}

impl Display for CacheQuantType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            CacheQuantType::F32 => "f32".to_string(),
            CacheQuantType::F16 => "f16".to_string(),
            CacheQuantType::Bf16 => "bf16".to_string(),
            CacheQuantType::Q8_0 => "q8_0".to_string(),
            CacheQuantType::Q4_0 => "q4_0".to_string(),
            CacheQuantType::Q4_1 => "q4_1".to_string(),
            CacheQuantType::Iq4Nl => "iq4_nl".to_string(),
            CacheQuantType::Q5_0 => "q5_0".to_string(),
            CacheQuantType::Q5_1 => "q5_1".to_string(),
        };
        write!(f, "{}", str)
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct ModelParams {
    context: ContextSize,
    temperature: f32,
    top_k: i32,
    top_p: f32,
    min_p: f32,
    repetition_penalty: f32,
    cache_type_k: CacheQuantType,
    cache_type_v: CacheQuantType,
    flash_attention: bool,
    jinja: bool,
}

impl Default for ModelParams {
    fn default() -> Self {
        Self {
            context: ContextSize::default(),
            temperature: 0.8,
            top_k: 40,
            top_p: 0.9,
            min_p: 0.1,
            repetition_penalty: 1.0,
            cache_type_k: CacheQuantType::default(),
            cache_type_v: CacheQuantType::default(),
            flash_attention: false,
            jinja: false,
        }
    }
}

impl ModelParams {
    pub fn context_size(&self) -> i32 {
        self.context.size()
    }

    pub fn temperature(&self) -> f32 {
        self.temperature
    }

    pub fn top_k(&self) -> i32 {
        self.top_k
    }

    pub fn top_p(&self) -> f32 {
        self.top_p
    }

    pub fn min_p(&self) -> f32 {
        self.min_p
    }

    pub fn cache_type_k(&self) -> &CacheQuantType {
        &self.cache_type_k
    }

    pub fn cache_type_v(&self) -> &CacheQuantType {
        &self.cache_type_v
    }

    pub fn flash_attention(&self) -> bool {
        self.flash_attention
    }

    pub fn jinja(&self) -> bool {
        self.jinja
    }

    pub fn repetition_penalty(&self) -> f32 {
        self.repetition_penalty
    }
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct ModelConfig {
    file: String,
    params: ModelParams,
    draft: Option<DraftModelConfig>,
}

impl ModelConfig {
    pub fn container_model_path(&self) -> String {
        format!("/models/{}", self.file)
    }

    pub fn params(&self) -> &ModelParams {
        &self.params
    }

    pub fn draft(&self) -> Option<&DraftModelConfig> {
        self.draft.as_ref()
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DockerConfig {
    image: String,
    volume_mount: String,
    network_name: String,
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            image: "ghcr.io/ggml-org/llama.cpp:server-rocm".to_string(),
            volume_mount: "/path/to/models".to_string(),
            network_name: "llm-network".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    docker: DockerConfig,
    models: HashMap<String, ModelConfig>,
}

impl Default for Config {
    fn default() -> Self {
        let mut models = HashMap::new();
        models.insert(
            "llama-3.1-70b".to_string(),
            ModelConfig {
                params: ModelParams {
                    cache_type_k: CacheQuantType::Q8_0,
                    cache_type_v: CacheQuantType::Q8_0,
                    ..Default::default()
                },
                draft: Some(DraftModelConfig {
                    file: "llama-3.1-8b-instruct-Q4_K_M.gguf".to_string(),
                    cache_type_k: CacheQuantType::Q4_0,
                    cache_type_v: CacheQuantType::Q4_0,
                }),
                file: "llama-3.1-70b-instruct-Q4_K_M.gguf".to_string(),
            },
        );
        Self {
            docker: DockerConfig::default(),
            models,
        }
    }
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Error reading config: {0}")]
    Confy(#[from] confy::ConfyError),
}

impl Config {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Option<Config> {
        let cfg = Config::load_or_create(path);
        match cfg {
            Err(ConfigError::Confy(message, ..)) => {
                error!("Failed to load configuration: {}", message);
            }
            Err(ConfigError::Io(message, ..)) => {
                error!("Failed to load configuration: {}", message);
            }
            Ok(cfg) => return Some(cfg),
        }
        None
    }

    fn load_or_create<P: AsRef<Path>>(path: P) -> Result<Config, ConfigError> {
        let path = path.as_ref();

        if path.exists() {
            let cfg: Self = confy::load_path(path)?;
            Ok(cfg)
        } else {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)?;
            }
            let cfg = Config::default();
            confy::store_path(path, &cfg)?;
            Ok(cfg)
        }
    }

    pub fn get_model(&self, model_name: &str) -> Option<Model> {
        self.models
            .get(model_name)
            .map(|model_config| self.get_model_from_config(model_name, model_config))
    }

    pub fn get_docker_image(&self) -> String {
        self.docker.image.clone()
    }

    pub fn get_model_path(&self) -> String {
        self.docker.volume_mount.clone()
    }

    pub fn get_docker_network(&self) -> String {
        self.docker.network_name.clone()
    }

    pub fn get_all_models(&self) -> Vec<Model> {
        self.models
            .iter()
            .map(|(name, model_config)| self.get_model_from_config(name, model_config))
            .collect()
    }

    fn get_host_model_path(&self, file_name: &str) -> String {
        format!("{}/{}", self.docker.volume_mount, file_name)
    }

    fn get_model_from_config(&self, model_name: &str, model_config: &ModelConfig) -> Model {
        let container_name = format!("llm_{}", model_name);
        let ctx_size = model_config.params.context.size() as usize;
        let draft_estimated_memory_usage = model_config
            .draft()
            .and_then(|draft| {
                let host_draft_model_path = self.get_host_model_path(&draft.file);
                estimate_memory(host_draft_model_path, ctx_size, KvQuant::Q4)
                    .ok()
                    .flatten()
                    .map(|est| est.total_required_mb)
            })
            .unwrap_or(0);

        let host_model_path = self.get_host_model_path(&model_config.file);
        let estimated_memory_usage = estimate_memory(host_model_path, ctx_size, KvQuant::Int8)
            .ok()
            .flatten()
            .map(|est| est.total_required_mb)
            .unwrap_or(u64::MAX)
            + draft_estimated_memory_usage;
        info!("Estimated memory usage: {}", estimated_memory_usage);
        Model {
            config: model_config.clone(),
            model_name: model_name.to_string(),
            container_name,
            estimated_memory_usage,
        }
    }
}

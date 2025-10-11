use crate::config::config::ModelConfig;

pub struct Model {
    pub estimated_memory_usage: u64,
    pub model_name: String,
    pub container_name: String,
    pub config: ModelConfig,
}

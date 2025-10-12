use crate::config::config::Config;
use crate::model::Model;
use crate::repositories::docker_repository::DockerRepository;
use crate::repositories::vram_repository::VramRepository;
use crate::services::backend_server::BackendServer;
use bollard::errors::Error as DockerError;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{error, info};

pub struct BackendServerManager {
    docker_repository: DockerRepository,
    vram_repository: VramRepository,
    config: Config,
    last_used: HashMap<String, SystemTime>,
}

#[derive(Debug, Error)]
pub enum EstimateError {
    #[error("Model {0} not found")]
    ModelNotFound(String),
    #[error("Error from Docker: {0}")]
    Docker(#[from] DockerError),
    #[error("Unable to free enough memory for model: {0}")]
    FreeFailed(String),
}

pub type BackendServerManagerState = Arc<Mutex<BackendServerManager>>;

impl BackendServerManager {
    pub async fn new(docker_repository: DockerRepository, config: Config) -> Self {
        let manager = Self {
            docker_repository,
            vram_repository: VramRepository::new(),
            config: config.clone(),
            last_used: HashMap::new(),
        };
        for model in config.get_all_models() {
            manager.model_fits_total_memory(&model).await;
        }
        manager
    }

    pub fn get_all_models(&self) -> Vec<Model> {
        self.config.get_all_models()
    }

    /// Returns the server if available
    /// Should update the LRU
    pub async fn get_server(&mut self, model_name: &str) -> Result<BackendServer, EstimateError> {
        let model = self
            .config
            .get_model(model_name)
            .ok_or(EstimateError::ModelNotFound(model_name.to_string()))?;

        if !self.docker_repository.container_exists(&model).await {
            self.docker_repository
                .create_server_container(&model)
                .await?;
        }

        if !self.docker_repository.is_running(&model).await? {
            self.unload_models_to_fit_if_necessary(&model).await?;
            self.docker_repository
                .start_server_container(&model)
                .await?;
        }

        while !self.docker_repository.is_healthy(&model).await? {
            sleep(Duration::from_secs(1)).await;
        }

        // Update last used time
        self.last_used
            .insert(model.container_name.clone(), SystemTime::now());

        let backend_server = BackendServer {
            hostname: self.docker_repository.get_hostname(&model),
        };
        Ok(backend_server)
    }

    async fn model_fits_free_memory(&self, requested_model: &Model) -> bool {
        let required = requested_model.estimated_memory_usage;
        let free = self.vram_repository.get_free_memory().await;
        required <= free
    }

    async fn model_fits_total_memory(&self, requested_model: &Model) {
        let required = requested_model.estimated_memory_usage;
        let total = self.vram_repository.get_total_memory().await;
        let fits = required <= total;
        if !fits {
            error!(
                "Model {} may not fit in memory, requires {} MB of memory while only up to {} MB available",
                requested_model.model_name, required, total
            );
        }
    }

    async fn unload_models_to_fit_if_necessary(
        &self,
        requested_model: &Model,
    ) -> Result<(), EstimateError> {
        // Fast‑path: if it already fits we are done.
        if self.model_fits_free_memory(requested_model).await {
            return Ok(());
        }

        // Get all model configs
        let all_models = self.config.get_all_models();

        // Build a list of (model_config, last_used_time) for running containers
        let mut running_models = Vec::new();

        for model in all_models {
            // Skip the requested model
            if model.container_name == requested_model.container_name {
                continue;
            }

            // Only consider running containers
            if self.docker_repository.container_exists(&model).await
                && self.docker_repository.is_running(&model).await?
            {
                let last_used_time = self
                    .last_used
                    .get(&model.container_name)
                    .copied()
                    .unwrap_or(SystemTime::UNIX_EPOCH);

                running_models.push((model, last_used_time));
            }
        }

        // Sort by LRU (oldest first)
        running_models.sort_by_key(|(_, last_used)| *last_used);

        // Try to unload models until we have enough space
        for (model_config, _) in running_models {
            self.docker_repository
                .stop_server_container(&model_config)
                .await?;

            // Check if we now have enough space
            if self.model_fits_free_memory(requested_model).await {
                info!("Successfully freed enough memory for the requested model");
                return Ok(());
            }
        }

        // If we still don't have enough space, return an error
        error!(
            "Unable to free enough memory for model: {}",
            requested_model.model_name
        );
        Err(EstimateError::FreeFailed(
            requested_model.model_name.clone(),
        ))
    }
}

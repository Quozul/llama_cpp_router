use crate::config::config::Config;
use crate::model::Model;
use crate::repositories::docker_repository::DockerRepository;
use crate::services::backend_server::BackendServer;
use axum::Json;
use bollard::errors::Error as DockerError;
use serde_json::Value;
use std::collections::HashMap;
use std::ops::Div;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{error, info};

pub struct BackendServerManager {
    docker_repository: DockerRepository,
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
    pub fn new(docker_repository: DockerRepository, config: Config) -> Self {
        Self {
            docker_repository,
            config,
            last_used: HashMap::new(),
        }
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

    /// Returns true if the model fits in memory
    async fn model_fits(&self, requested_model: &Model) -> Result<bool, EstimateError> {
        let required = requested_model.estimated_memory_usage;
        let free = self.get_available_memory().await;
        let fits = required <= free;
        if !fits {
            info!(?required, ?free);
        }
        Ok(fits)
    }

    async fn unload_models_to_fit_if_necessary(
        &self,
        requested_model: &Model,
    ) -> Result<(), EstimateError> {
        // Fast‑path: if it already fits we are done.
        if self.model_fits(requested_model).await? {
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
            if self.model_fits(requested_model).await? {
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

    /// Runs `rocm-smi` to get the amount of VRAM available
    pub async fn get_available_memory(&self) -> u64 {
        // Execute the CLI tool.
        let output = Command::new("rocm-smi")
            .arg("--showmeminfo")
            .arg("vram")
            .arg("--json")
            .output()
            .await
            .expect("failed to execute rocm-smi");

        if !output.status.success() {
            // If the tool failed we treat it as no free memory (conservative).
            error!("rocm-smi returned a non‑zero exit code");
            return 0;
        }

        // Parse the JSON payload.
        let stdout = String::from_utf8_lossy(&output.stdout);
        let v: Value = match serde_json::from_str(&stdout) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to parse rocm‑smi JSON output: {}", e);
                return 0;
            }
        };

        // The JSON has a top‑level key like "card0". Grab the first object.
        let card = match v.as_object().and_then(|obj| obj.values().next()) {
            Some(c) => c,
            None => {
                error!("Unexpected rocm‑smi JSON structure");
                return 0;
            }
        };

        // Extract the two fields we need.
        let total_str = card
            .get("VRAM Total Memory (B)")
            .and_then(|v| v.as_str())
            .unwrap_or("0");
        let used_str = card
            .get("VRAM Total Used Memory (B)")
            .and_then(|v| v.as_str())
            .unwrap_or("0");

        let total = u64::from_str(total_str).unwrap_or(0);
        let used = u64::from_str(used_str).unwrap_or(0);

        // Free memory = total - used (but never negative).
        total.saturating_sub(used).div(1_000_000) // TODO: would be cool to have the same unit everywhere
    }
}

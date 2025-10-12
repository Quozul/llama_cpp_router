use crate::config::config::Config;
use crate::model::Model;
use bollard::models::{
    ContainerCreateBody, DeviceMapping, EndpointSettings, HealthStatusEnum, HostConfig, Mount,
    MountTypeEnum, NetworkingConfig, PortBinding, PortMap, RestartPolicy, RestartPolicyNameEnum,
};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, InspectContainerOptionsBuilder, StartContainerOptionsBuilder,
    StopContainerOptionsBuilder,
};
use bollard::{Docker, errors::Error as DockerError};
use std::collections::HashMap;
use thiserror::Error;
use tracing::info;

pub struct DockerRepository {
    docker: Docker,
    config: Config,
}

#[derive(Error, Debug)]
pub enum InitializationError {
    #[error("Error initializing docker daemon: {0}")]
    Docker(#[from] DockerError),
}

impl DockerRepository {
    const PORT: u16 = 8080;
    pub fn new(config: Config) -> Result<Self, InitializationError> {
        let docker = Docker::connect_with_local_defaults()?;
        Ok(Self { docker, config })
    }

    pub fn get_hostname(&self, model: &Model) -> String {
        format!("{}:{}", model.container_name, Self::PORT)
    }

    pub async fn create_server_container(&self, model: &Model) -> Result<(), DockerError> {
        info!("Creating container: {}", model.container_name);

        let model_params = model.config.params();
        let options = CreateContainerOptionsBuilder::new()
            .name(&model.container_name)
            .build();
        let port = Self::PORT.to_string();
        let ctx_size = model_params.context_size().to_string();
        let flash_attn = if model_params.flash_attention() {
            "on"
        } else {
            "off"
        };
        let temperature = model_params.temperature().to_string();
        let top_k = model_params.top_k().to_string();
        let top_p = model_params.top_p().to_string();
        let min_p = model_params.min_p().to_string();
        let repetition_penalty = model_params.repetition_penalty().to_string();
        let container_model_path = model.config.container_model_path();

        let fixed_args = [
            ("-m", container_model_path.as_str()),
            ("--host", "0.0.0.0"),
            ("--port", port.as_str()),
            ("--threads", "32"),
            ("--ctx-size", ctx_size.as_str()),
            ("--temp", temperature.as_str()),
            ("--top-k", top_k.as_str()),
            ("--top-p", top_p.as_str()),
            ("--min-p", min_p.as_str()),
            ("--repeat-penalty", repetition_penalty.as_str()),
            ("--cache-type-k", &model_params.cache_type_k().to_string()),
            ("--cache-type-v", &model_params.cache_type_v().to_string()),
            ("--flash-attn", flash_attn),
            ("--no-mmap", ""),
        ];

        let mut cmd = Vec::new();
        for (key, value) in fixed_args {
            cmd.push(key.to_string());
            if !value.is_empty() {
                cmd.push(value.to_string());
            }
        }

        if model_params.jinja() {
            cmd.push("--jinja".to_string());
        }

        if let Some(draft) = model.config.draft() {
            cmd.push("--model-draft".to_string());
            cmd.push(format!("/models/{}", draft.file));
            cmd.push("--cache-type-k-draft".to_string());
            cmd.push(draft.cache_type_k.to_string());
            cmd.push("--cache-type-v-draft".to_string());
            cmd.push(draft.cache_type_v.to_string());
        }

        let mut exposed_ports: HashMap<String, HashMap<(), ()>> = HashMap::new();
        exposed_ports.insert(format!("{port}/tcp"), HashMap::new());

        let mut port_map = PortMap::new();
        port_map.insert(
            format!("{port}/tcp"),
            Some(vec![PortBinding {
                host_port: Some("8080".to_string()),
                host_ip: Some("0.0.0.0".to_string()),
            }]),
        );

        let host_config = HostConfig {
            restart_policy: Some(RestartPolicy {
                name: Some(RestartPolicyNameEnum::NO),
                ..Default::default()
            }),
            security_opt: Some(vec!["seccomp:unconfined".to_string()]),
            group_add: Some(vec!["video".to_string()]),
            devices: Some(vec![
                DeviceMapping {
                    path_on_host: Some("/dev/dri".to_string()),
                    path_in_container: Some("/dev/dri".to_string()),
                    cgroup_permissions: Some("rwm".to_string()),
                },
                DeviceMapping {
                    path_on_host: Some("/dev/kfd".to_string()),
                    path_in_container: Some("/dev/kfd".to_string()),
                    cgroup_permissions: Some("rwm".to_string()),
                },
            ]),
            mounts: Some(vec![Mount {
                typ: Some(MountTypeEnum::BIND),
                source: Some(self.config.get_model_path()),
                target: Some("/models".to_string()),
                ..Default::default()
            }]),
            port_bindings: Some(port_map),
            ..Default::default()
        };

        let mut endpoints_config: HashMap<String, EndpointSettings> = HashMap::new();
        endpoints_config.insert(
            self.config.get_docker_network(),
            EndpointSettings::default(),
        );
        let networking_config = NetworkingConfig {
            endpoints_config: Some(endpoints_config),
        };

        let config = ContainerCreateBody {
            cmd: Some(cmd),
            image: Some(self.config.get_docker_image()),
            exposed_ports: Some(exposed_ports),
            host_config: Some(host_config),
            networking_config: Some(networking_config),
            ..Default::default()
        };
        self.docker.create_container(Some(options), config).await?;
        Ok(())
    }

    pub async fn start_server_container(&self, model: &Model) -> Result<(), DockerError> {
        info!("Starting container: {}", model.container_name);
        let options = StartContainerOptionsBuilder::new().build();
        self.docker
            .start_container(&model.container_name, Some(options))
            .await?;
        Ok(())
    }

    pub async fn stop_server_container(&self, model: &Model) -> Result<(), DockerError> {
        info!("Stopping container: {}", model.container_name);
        let options = StopContainerOptionsBuilder::new().build();
        self.docker
            .stop_container(&model.container_name, Some(options))
            .await?;
        Ok(())
    }

    pub async fn container_exists(&self, model: &Model) -> bool {
        self.docker
            .inspect_container(
                &model.container_name,
                Some(InspectContainerOptionsBuilder::new().build()),
            )
            .await
            .is_ok()
    }

    pub async fn is_running(&self, model: &Model) -> Result<bool, DockerError> {
        let health = self.get_health(model).await?;
        Ok(matches!(health, Health::Healthy | Health::Starting))
    }

    pub async fn is_healthy(&self, model: &Model) -> Result<bool, DockerError> {
        let health = self.get_health(model).await?;
        Ok(matches!(health, Health::Healthy))
    }

    async fn get_health(&self, model: &Model) -> Result<Health, DockerError> {
        let state = self
            .docker
            .inspect_container(
                &model.container_name,
                Some(InspectContainerOptionsBuilder::new().build()),
            )
            .await?
            .state;
        let health = state
            .and_then(|state| state.health)
            .and_then(|health| health.status)
            .map(|health_status| match health_status {
                HealthStatusEnum::EMPTY => Health::None,
                HealthStatusEnum::NONE => Health::None,
                HealthStatusEnum::STARTING => Health::Starting,
                HealthStatusEnum::HEALTHY => Health::Healthy,
                HealthStatusEnum::UNHEALTHY => Health::Unhealthy,
            })
            .unwrap_or_default();
        Ok(health)
    }
}

#[derive(Default, Debug)]
enum Health {
    Healthy,
    Starting,
    Unhealthy,
    #[default]
    None,
}

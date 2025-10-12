mod app_error;
mod cli;
mod config;
mod controllers;
mod event_source;
mod model;
mod repositories;
mod services;

use crate::cli::Cli;
use crate::config::config::Config;
use crate::controllers::chat_completions::post_chat_completions;
use crate::controllers::models::get_models;
use crate::services::backend_server_manager::{BackendServerManager, BackendServerManagerState};
use axum::routing::get;
use axum::{Router, routing::post};
use clap::Parser;
use repositories::docker_repository::DockerRepository;
use std::process::ExitCode;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};
use tracing::{Level, info};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    enable_logging(cli.verbose);
    let Some(config) = Config::from_path(cli.config_path) else {
        return Ok(ExitCode::FAILURE);
    };
    let docker_repository = DockerRepository::new(config.clone())?;
    let state: BackendServerManagerState = Arc::new(Mutex::new(
        BackendServerManager::new(docker_repository, config).await,
    ));

    let open_ai_router = Router::new()
        .route("/chat/completions", post(post_chat_completions))
        .route("/models", get(get_models))
        .with_state(state);
    let app = Router::new().nest("/v1", open_ai_router).layer(
        CorsLayer::new()
            .allow_origin(Any)
            .allow_headers(Any)
            .allow_methods(Any),
    );

    // run it
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(ExitCode::SUCCESS)
}

fn enable_logging(verbose: u8) {
    let log_level = match verbose {
        0 => Level::INFO,
        1 => Level::DEBUG,
        _ => Level::TRACE,
    };

    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive(log_level.into()))
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

use crate::event_source::{ClientEvent, EventSource};
use crate::services::backend_server_manager::BackendServerManagerState;
use axum::response::sse::{self, Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::{Json, extract::State, http::StatusCode};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tracing::error;

#[derive(Serialize)]
struct ErrorResponse {
    message: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ChatCompletionRequest {
    model: String,
    stream: Option<bool>,
    #[serde(flatten)]
    other: Map<String, Value>,
}

pub async fn post_chat_completions(
    State(backend_server_manager): State<BackendServerManagerState>,
    Json(payload): Json<ChatCompletionRequest>,
) -> Response {
    if payload.stream.unwrap_or(false) {
        streaming(backend_server_manager, payload)
            .await
            .into_response()
    } else {
        non_streaming(backend_server_manager, payload)
            .await
            .into_response()
    }
}

async fn streaming(
    backend_server_manager: BackendServerManagerState,
    payload: ChatCompletionRequest,
) -> impl IntoResponse {
    let (tx, rx) = mpsc::channel::<Result<Event, String>>(10);
    let event_stream = ReceiverStream::new(rx);

    // Clone the model name for tracking
    let model_name = payload.model.clone();
    let manager_for_cleanup = backend_server_manager.clone();

    tokio::spawn(async move {
        let backend = {
            let mut manager = backend_server_manager.lock().await;
            // Increment active requests before starting
            manager.increment_active_requests(&payload.model);

            match manager.get_server(&payload.model).await {
                Ok(b) => b,
                Err(e) => {
                    // Decrement on error before returning
                    manager.decrement_active_requests(&payload.model);
                    let _ = tx
                        .send(Ok(Event::default().data(
                            serde_json::to_string(&ErrorResponse {
                                message: e.to_string(),
                            })
                            .unwrap(),
                        )))
                        .await;
                    return;
                }
            }
        };

        let backend_url = format!("http://{}/v1/chat/completions", backend.hostname);
        let es = EventSource::new(&backend_url, &payload).await;
        match es {
            Ok(mut es) => {
                while let Some(event) = es.next().await {
                    match event {
                        Ok(ClientEvent::Open) => {
                            let _ = tx
                                .send(Ok(Event::default().comment("Connection open")))
                                .await;
                        }
                        Ok(ClientEvent::Message(message)) => {
                            let _ = tx.send(Ok(Event::default().data(&message.data))).await;
                        }
                        Err(err) => {
                            let _ = tx
                                .send(Ok(Event::default().data(
                                    serde_json::to_string(&ErrorResponse {
                                        message: err.to_string(),
                                    })
                                    .unwrap(),
                                )))
                                .await;
                        }
                    }
                }
            }
            Err(err) => {
                error!("{err}");
            }
        };

        // Decrement active requests when streaming completes
        let mut manager = manager_for_cleanup.lock().await;
        manager.decrement_active_requests(&model_name);
    });

    (
        StatusCode::OK,
        Sse::new(event_stream).keep_alive(
            sse::KeepAlive::new()
                .interval(Duration::from_secs(1))
                .text("keep-alive-text"),
        ),
    )
        .into_response()
}

async fn non_streaming(
    backend_server_manager: BackendServerManagerState,
    payload: ChatCompletionRequest,
) -> impl IntoResponse {
    let backend = {
        let mut manager = backend_server_manager.lock().await;
        // Increment active requests before starting
        manager.increment_active_requests(&payload.model);

        match manager.get_server(&payload.model).await {
            Ok(b) => b,
            Err(e) => {
                // Decrement on error before returning
                manager.decrement_active_requests(&payload.model);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        message: e.to_string(),
                    })
                    .into_response(),
                );
            }
        }
    };

    let client = Client::new();
    let backend_url = format!("http://{}/v1/chat/completions", backend.hostname);

    let result = match client.post(&backend_url).json(&payload).send().await {
        Ok(resp) => {
            let status = resp.status();
            match resp.json::<Value>().await {
                Ok(json) => (status, Json(json).into_response()),
                Err(err) => (
                    StatusCode::BAD_GATEWAY,
                    Json(ErrorResponse {
                        message: format!("failed to decode backend JSON: {}", err),
                    })
                    .into_response(),
                ),
            }
        }
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse {
                message: err.to_string(),
            })
            .into_response(),
        ),
    };

    // Decrement active requests after request completes
    {
        let mut manager = backend_server_manager.lock().await;
        manager.decrement_active_requests(&payload.model);
    }

    result
}

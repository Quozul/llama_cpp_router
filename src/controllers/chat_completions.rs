use crate::event_source::{ClientEvent, EventSource};
use crate::services::backend_server_manager::BackendServerManagerState;
use axum::response::Response;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{self, Event, Sse},
    },
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::{StreamExt, wrappers::ReceiverStream};

#[derive(Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    model: String,
    stream: Option<bool>,
    messages: Vec<Message>,
}

#[derive(Serialize, Deserialize)]
pub struct Message {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    message: String,
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

    tokio::spawn(async move {
        let backend = {
            let mut manager = backend_server_manager.lock().await;
            match manager.get_server(&payload.model).await {
                Ok(b) => b,
                Err(e) => {
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
        let mut es = EventSource::new(&backend_url, &payload).await.unwrap();
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
        match manager.get_server(&payload.model).await {
            Ok(b) => b,
            Err(e) => {
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

    match client.post(&backend_url).json(&payload).send().await {
        Ok(resp) => {
            let status = resp.status();
            match resp.json::<serde_json::Value>().await {
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
    }
}

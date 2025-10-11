use futures::Stream;
use futures::stream::StreamExt;
use reqwest::{Client, Error as ReqwestError};
use serde::Serialize;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

#[derive(Debug)]
pub enum ClientEvent {
    Open,
    Message(Message),
}

#[derive(Debug)]
pub struct Message {
    pub id: Option<String>,
    pub event: Option<String>,
    pub data: String,
    pub retry: Option<u64>,
}

#[derive(Debug)]
pub enum EventSourceError {
    Reqwest(ReqwestError),
    ParseError(String),
}

impl From<ReqwestError> for EventSourceError {
    fn from(error: ReqwestError) -> Self {
        EventSourceError::Reqwest(error)
    }
}

impl std::fmt::Display for EventSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventSourceError::Reqwest(e) => write!(f, "Reqwest error: {}", e),
            EventSourceError::ParseError(e) => write!(f, "Parse error: {}", e),
        }
    }
}

impl std::error::Error for EventSourceError {}

pub struct EventSource {
    stream: UnboundedReceiverStream<Result<ClientEvent, EventSourceError>>,
}

impl EventSource {
    pub async fn new<T: Serialize>(
        url: &str,
        payload: &T,
    ) -> Result<EventSource, EventSourceError> {
        let client = Client::new();
        let json_payload = serde_json::to_string(payload)
            .map_err(|e| EventSourceError::ParseError(e.to_string()))?;

        let response = client
            .post(url)
            .header("Accept", "text/event-stream")
            .header("Content-Type", "application/json")
            .body(json_payload)
            .send()
            .await
            .map_err(EventSourceError::Reqwest)?;

        if !response.status().is_success() {
            return Err(EventSourceError::Reqwest(
                response.error_for_status().unwrap_err(),
            ));
        }

        let (tx, rx) = mpsc::unbounded_channel();
        let mut response_stream = response.bytes_stream();

        tokio::spawn(async move {
            // Send connection closed event
            let _ = tx.send(Ok(ClientEvent::Open));

            while let Some(chunk) = response_stream.next().await {
                let result = async {
                    match chunk {
                        Ok(chunk) => {
                            if chunk.is_empty() {
                                return Ok(vec![]);
                            }

                            let chunk_str = std::str::from_utf8(&chunk)
                                .map_err(|e| EventSourceError::ParseError(e.to_string()))?;

                            Ok(parse_sse_chunk(chunk_str))
                        }
                        Err(e) => Err(e.into()),
                    }
                }
                .await;

                match result {
                    Ok(events) => {
                        for event in events {
                            if tx.send(Ok(event)).is_err() {
                                // Receiver dropped, exit
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        if tx.send(Err(e)).is_err() {
                            // Receiver dropped, exit
                            return;
                        }
                    }
                }
            }
        });

        Ok(EventSource {
            stream: UnboundedReceiverStream::new(rx),
        })
    }
}

impl Stream for EventSource {
    type Item = Result<ClientEvent, EventSourceError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.get_mut().stream).poll_next(cx)
    }
}

fn parse_sse_chunk(chunk: &str) -> Vec<ClientEvent> {
    let mut events = vec![];
    let mut current_message = Message {
        id: None,
        event: None,
        data: String::new(),
        retry: None,
    };

    for line in chunk.lines() {
        let line = line.trim();
        if line.is_empty() {
            // Empty line indicates end of event
            if !current_message.data.trim().is_empty() || current_message.id.is_some() {
                events.push(ClientEvent::Message(current_message));
            }
            current_message = Message {
                id: None,
                event: None,
                data: String::new(),
                retry: None,
            };
        } else if let Some(colon_pos) = line.find(':') {
            let key = &line[..colon_pos];
            let value = &line[colon_pos + 1..];

            match key {
                "data" => {
                    if value.starts_with(' ') {
                        current_message.data.push(' ');
                    }
                    current_message.data.push_str(value.trim_start());
                }
                "id" => current_message.id = Some(value.trim().to_string()),
                "event" => current_message.event = Some(value.trim().to_string()),
                "retry" => {
                    if let Ok(retry) = value.trim().parse::<u64>() {
                        current_message.retry = Some(retry);
                    }
                }
                _ => {}
            }
        } else {
            // Handle case where line doesn't contain ':'
            if line.starts_with("data") {
                current_message.data.push_str(&line[5..]);
            } else if line.starts_with("id") {
                current_message.id = Some(line[3..].trim().to_string());
            } else if line.starts_with("event") {
                current_message.event = Some(line[6..].trim().to_string());
            }
        }
    }

    // Handle last event if stream ends without empty line
    if !current_message.data.trim().is_empty() || current_message.id.is_some() {
        events.push(ClientEvent::Message(current_message));
    }

    events
}

use futures::Stream;
use futures::stream::StreamExt;
use reqwest::{Client, Error as ReqwestError};
use serde::Serialize;
use std::pin::Pin;
use std::task::{Context, Poll};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

#[derive(Debug, Eq, PartialEq)]
pub enum ClientEvent {
    Open,
    Message(Message),
}

#[derive(Debug, Eq, PartialEq)]
pub struct Message {
    pub id: Option<String>,
    pub event: Option<String>,
    pub data: String,
    pub retry: Option<u64>,
}

#[derive(Debug, Error)]
pub enum EventSourceError {
    #[error("Request error: {0}")]
    Request(String),
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] ReqwestError),
    #[error("ParseError error: {0}")]
    ParseError(String),
}

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
            return Err(EventSourceError::Request(response.text().await?));
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
            let value = &line[colon_pos + 2..];

            match key {
                "data" => {
                    current_message.data.push_str(value);
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

// -------------------------------------------------------------
//  Tests for `parse_sse_chunk` – Server‑Sent Events parser
// -------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::{ClientEvent, Message, parse_sse_chunk};

    /// Helper that builds a `Message` with only the fields that are
    /// relevant for a particular test.  All other fields default to `None`.
    fn msg(
        data: impl Into<String>,
        id: Option<&str>,
        event: Option<&str>,
        retry: Option<u64>,
    ) -> Message {
        Message {
            data: data.into(),
            id: id.map(str::to_string),
            event: event.map(str::to_string),
            retry,
        }
    }

    // -------------------------------------------------------------------------
    //  1️⃣  “empty / comment only” → no events at all
    // -------------------------------------------------------------------------
    #[test]
    fn empty_chunk_returns_nothing() {
        assert_eq!(parse_sse_chunk(""), Vec::<ClientEvent>::new());
    }

    #[test]
    fn comment_lines_are_ignored() {
        let src = ": this is a comment\n\
                   :another comment\r\n\
                   :yet another\r\n\r\n";
        assert_eq!(parse_sse_chunk(src), Vec::<ClientEvent>::new());
    }

    // -------------------------------------------------------------------------
    //  2️⃣  Simple “data” line terminated by a blank line → one Message event
    // -------------------------------------------------------------------------
    #[test]
    fn single_data_line_becomes_message() {
        let src = "data: hello world\r\n\r\n";
        let expected = vec![ClientEvent::Message(msg("hello world", None, None, None))];
        assert_eq!(parse_sse_chunk(src), expected);
    }

    // -------------------------------------------------------------------------
    //  3️⃣  Multiple “data” lines are concatenated with `\n`
    // -------------------------------------------------------------------------
    #[test]
    fn multi_data_lines_are_joined_with_newline() {
        let src = "data: line 1\r\ndata: line 2\r\ndata: line 3\r\n\r\n";
        let expected = vec![ClientEvent::Message(msg(
            "line 1\nline 2\nline 3",
            None,
            None,
            None,
        ))];
        assert_eq!(parse_sse_chunk(src), expected);
    }

    // -------------------------------------------------------------------------
    //  4️⃣  Optional fields `id`, `event`, and `retry`
    // -------------------------------------------------------------------------
    #[test]
    fn id_event_and_retry_are_parsed() {
        let src = "\
            id: 42\r\n\
            event: custom\r\n\
            retry: 2500\r\n\
            data: payload\r\n\r\n";

        let expected = vec![ClientEvent::Message(Message {
            data: "payload".into(),
            id: Some("42".into()),
            event: Some("custom".into()),
            retry: Some(2500),
        })];
        assert_eq!(parse_sse_chunk(src), expected);
    }

    // -------------------------------------------------------------------------
    //  5️⃣  Fields that appear multiple times – last one wins (except `data`)
    // -------------------------------------------------------------------------
    #[test]
    fn last_id_and_event_override_previous_ones() {
        let src = "\
            id: first\r\n\
            id: second\r\n\
            event: a\r\n\
            event: b\r\n\
            data: something\r\n\r\n";

        let expected = vec![ClientEvent::Message(Message {
            data: "something".into(),
            id: Some("second".into()),
            event: Some("b".into()),
            retry: None,
        })];
        assert_eq!(parse_sse_chunk(src), expected);
    }

    // -------------------------------------------------------------------------
    //  6️⃣  `retry` must be a *valid* u64 – malformed values are ignored
    // -------------------------------------------------------------------------
    #[test]
    fn malformed_retry_is_ignored() {
        let src = "\
            retry: not-a-number\r\n\
            data: ok\r\n\r\n";

        let expected = vec![ClientEvent::Message(msg("ok", None, None, None))]; // `retry` stays None
        assert_eq!(parse_sse_chunk(src), expected);
    }

    // -------------------------------------------------------------------------
    //  7️⃣  Whitespace after the colon is stripped, but leading spaces in the
    //      value are kept (per SSE spec)
    // -------------------------------------------------------------------------
    #[test]
    fn leading_spaces_after_colon_are_preserved() {
        let src = "data:   three spaces before\r\n\r\n";
        let expected = vec![ClientEvent::Message(msg(
            "  three spaces before",
            None,
            None,
            None,
        ))];
        assert_eq!(parse_sse_chunk(src), expected);
    }

    // -------------------------------------------------------------------------
    //  8️⃣  A blank line *terminates* the current event and resets the
    //      accumulator – subsequent lines belong to a new event
    // -------------------------------------------------------------------------
    #[test]
    fn two_events_are_separated_by_blank_line() {
        let src = "\
            data: first\r\n\r\n\
            data: second\r\n\r\n";

        let expected = vec![
            ClientEvent::Message(msg("first", None, None, None)),
            ClientEvent::Message(msg("second", None, None, None)),
        ];
        assert_eq!(parse_sse_chunk(src), expected);
    }

    // -------------------------------------------------------------------------
    //  9️⃣  An “open” event is emitted exactly once, the first time the parser
    //       sees any data (including just a blank line).  The open event does
    //       **not** carry any payload.
    // -------------------------------------------------------------------------
    #[test]
    fn open_event_is_the_first_event_returned() {
        // The exact moment when you decide to emit `Open` is up to the
        // implementation, but the most common contract is:
        //   * The very first call that receives *any* data (even a pure
        //     comment or a blank line) returns `ClientEvent::Open`.
        //   * Subsequent calls never emit `Open` again.
        //
        // The test expresses that contract – if you choose a different policy
        // you can simply adjust the expected vector.
        let first_chunk = "data: hello\r\n\r\n";
        let second_chunk = "data: world\r\n\r\n";

        let first = parse_sse_chunk(first_chunk);
        let second = parse_sse_chunk(second_chunk);

        assert_eq!(
            first,
            vec![
                ClientEvent::Open,
                ClientEvent::Message(msg("hello", None, None, None)),
            ],
            "first call should emit an Open + the first message"
        );

        assert_eq!(
            second,
            vec![ClientEvent::Message(msg("world", None, None, None))],
            "subsequent calls must NOT emit another Open"
        );
    }

    // -------------------------------------------------------------------------
    //  🔟  Chunk boundaries – a line that is split across two calls must be
    //       correctly re‑assembled.  This test forces the parser to keep state
    //       between invocations (the stub will need a mutable static or a
    //       `Parser` struct to pass the test).
    // -------------------------------------------------------------------------
    #[test]
    fn line_split_across_chunks_is_handled() {
        // Simulate a network that delivered the following two parts:
        //   1. "data: part1"
        //   2. "\ndata: part2\n\n"
        // The parser must treat them as a single event with data
        // "part1\npart2".
        let part1 = "data: part1";
        let part2 = "\ndata: part2\n\n";

        // The implementation may expose a mutable parser that you can keep
        // across calls, but the simple `fn parse_sse_chunk(&str) -> Vec<_>`
        // signature forces the parser to be **stateless**.  If you prefer a
        // stateful API you can wrap `parse_sse_chunk` in a tiny struct inside
        // the test and call it twice; the behavior should be the same.
        //
        // Our expectation (stateless) is that a **partial line** does NOT
        // produce a message until the terminating newline is seen.
        let first = parse_sse_chunk(part1);
        assert_eq!(first, Vec::<ClientEvent>::new(), "no event yet");

        let second = parse_sse_chunk(part2);
        let expected = vec![ClientEvent::Message(msg("part1\npart2", None, None, None))];
        assert_eq!(second, expected);
    }

    // -------------------------------------------------------------------------
    //  🧪  Misc edge‑cases – stray `:` lines, unknown fields, empty data.
    // -------------------------------------------------------------------------
    #[test]
    fn unknown_fields_are_ignored_and_empty_data_is_allowed() {
        let src = "\
            foo: bar\r\n\
            data:\r\n\
            \r\n";

        // `foo:` is ignored, `data:` with an empty payload results in an empty
        // string for `Message.data`.
        let expected = vec![ClientEvent::Message(msg("", None, None, None))];
        assert_eq!(parse_sse_chunk(src), expected);
    }
}

use eventsource_stream::Eventsource;
use futures::Stream;
use futures::StreamExt;
use std::pin::Pin;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event_type: String,
    pub data: String,
}

pub struct SseParser {
    buffer: String,
}

impl SseParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    pub fn parse_chunk(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        let text = String::from_utf8_lossy(chunk);
        self.buffer.push_str(&text);

        let mut events = Vec::new();
        while let Some(pos) = self.buffer.find("\n\n") {
            let block = self.buffer[..pos].to_string();
            self.buffer.drain(..pos + 2);

            let mut event_type = "message".to_string();
            let mut data_lines = Vec::new();

            for line in block.lines() {
                if let Some(rest) = line.strip_prefix("event:") {
                    event_type = rest.trim().to_string();
                } else if let Some(rest) = line.strip_prefix("data:") {
                    data_lines.push(rest.trim());
                }
            }

            if !data_lines.is_empty() {
                events.push(SseEvent {
                    event_type,
                    data: data_lines.join("\n"),
                });
            }
        }
        events
    }
}

impl Default for SseParser {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SseTransport;

impl SseTransport {
    pub fn stream_bytes(
        response: reqwest::Response,
    ) -> Pin<Box<dyn Stream<Item = Result<SseEvent, String>> + Send>> {
        let stream = response.bytes_stream().eventsource();
        let mapped = stream.map(|res| match res {
            Ok(event) => Ok(SseEvent {
                event_type: event.event,
                data: event.data,
            }),
            Err(e) => Err(format!("SSE stream error: {}", e)),
        });
        Box::pin(mapped)
    }
}

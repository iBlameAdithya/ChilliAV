use chilli_model::adapter::{ModelAdapter, StreamEvent};
use chilli_model::anthropic::AnthropicAdapter;
use chilli_model::transport::SseEvent;

#[test]
fn test_anthropic_sse_event_parsing() {
    let adapter = AnthropicAdapter::default();
    let sse = SseEvent {
        event_type: "content_block_delta".to_string(),
        data: r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hello"}}"#
            .to_string(),
    };

    let events = adapter.parse_sse_event(&sse).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], StreamEvent::TextChunk("Hello".to_string()));
}

#[test]
fn test_anthropic_sse_tool_use_no_duplicate_finished_on_message_stop() {
    let adapter = AnthropicAdapter::default();

    // 1. message_delta with stop_reason "tool_use"
    let sse_delta = SseEvent {
        event_type: "message_delta".to_string(),
        data: r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"}}"#.to_string(),
    };
    let events_delta = adapter.parse_sse_event(&sse_delta).unwrap();
    assert_eq!(events_delta.len(), 1);
    assert_eq!(
        events_delta[0],
        StreamEvent::Finished(chilli_model::adapter::StopReason::ToolUse)
    );

    // 2. message_stop must NOT emit duplicate Finished(EndTurn)
    let sse_stop = SseEvent {
        event_type: "message_stop".to_string(),
        data: r#"{"type":"message_stop"}"#.to_string(),
    };
    let events_stop = adapter.parse_sse_event(&sse_stop).unwrap();
    assert!(events_stop.is_empty());
}

#[test]
fn test_stream_accumulator_preserves_first_finished_reason() {
    let mut acc = chilli_model::adapter::StreamAccumulator::default();
    acc.process(StreamEvent::Finished(
        chilli_model::adapter::StopReason::ToolUse,
    ));
    acc.process(StreamEvent::Finished(
        chilli_model::adapter::StopReason::EndTurn,
    ));

    let resp = acc.into_response();
    assert_eq!(
        resp.stop_reason,
        Some(chilli_model::adapter::StopReason::ToolUse)
    );
}

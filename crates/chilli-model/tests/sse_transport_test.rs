use chilli_model::transport::SseParser;

#[test]
fn test_sse_parser_fragmented_utf8_and_json() {
    let mut parser = SseParser::new();
    let raw_payload = "event: message_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"🚀 Hello World\"}}\n\n";
    let bytes = raw_payload.as_bytes();

    // Split payload across arbitrary byte boundary mid-UTF8 sequence
    let (chunk1, chunk2) = bytes.split_at(55);

    let events1 = parser.parse_chunk(chunk1);
    assert_eq!(events1.len(), 0);

    let events2 = parser.parse_chunk(chunk2);
    assert_eq!(events2.len(), 1);
    assert_eq!(events2[0].event_type, "message_delta");
    assert!(events2[0].data.contains("🚀 Hello World"));
}

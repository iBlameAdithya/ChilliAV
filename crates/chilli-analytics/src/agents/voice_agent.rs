use tracing::info;

/// Voice Processing Agent
/// Handles speech-to-text transcript normalization, punctuation repair,
/// acoustic filler word stripping, and voice query ingestion.
pub struct VoiceProcessingAgent;

impl VoiceProcessingAgent {
    pub fn new() -> Self {
        Self
    }

    /// Process input query (whether coming from speech transcript or direct text)
    pub fn process_input(&self, raw_input: &str, is_voice: bool) -> (String, bool) {
        info!("VoiceProcessingAgent: Processing input (is_voice={})", is_voice);
        let trimmed = raw_input.trim();

        if !is_voice {
            return (trimmed.to_string(), false);
        }

        // Clean speech transcripts: remove filler words, normalize spoken terms and enterprise domain jargon
        let cleaned = trimmed
            .replace("um", "")
            .replace("uh", "")
            .replace("please show me", "Show")
            .replace("can you display", "Show")
            .replace("dist", "distribution")
            .replace("last year", "for the last year")
            .replace("this mth", "this month")
            .replace("SK U hundred", "SKU-100")
            .replace("SKU hundred", "SKU-100");

        // Format casing
        let mut chars = cleaned.chars();
        let normalized = match chars.next() {
            None => String::new(),
            Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
        };

        info!("VoiceProcessingAgent: Transcribed & normalized voice query -> '{}'", normalized);
        (normalized, true)
    }
}

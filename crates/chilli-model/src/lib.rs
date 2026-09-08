//! chilli-model: Provider-neutral model capability router & streaming SSE adapters.

pub mod adapter;
pub mod anthropic;
pub mod capabilities;
pub mod endpoint;
pub mod failure;
pub mod manager;
pub mod openai;
pub mod openai_compatible;
pub mod resilience;
pub mod router;
pub mod transport;

use adapter::ModelAdapter;
use anthropic::AnthropicAdapter;
use openai::OpenAIAdapter;
use std::sync::Arc;

pub fn find_target_api_key() -> Option<String> {
    let mut search_paths = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        search_paths.push(cwd.join("target/.api_key"));
        search_paths.push(cwd.join("../target/.api_key"));
        search_paths.push(cwd.join("../../target/.api_key"));
    }
    search_paths.push(std::path::PathBuf::from("target/.api_key"));
    search_paths.push(std::path::PathBuf::from("../target/.api_key"));
    search_paths.push(std::path::PathBuf::from("../../target/.api_key"));

    for path in search_paths {
        if let Ok(content) = std::fs::read_to_string(&path) {
            let trimmed = content.trim_start_matches('\u{feff}').trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

pub fn create_adapter_for_model(model_name: Option<&str>) -> Arc<dyn ModelAdapter> {
    let name = model_name.unwrap_or("").trim();
    let lower = name.to_lowercase();

    if std::env::var("AGCL_BASE_URL").is_ok()
        || std::env::var("AGCL_MODEL").is_ok()
        || lower == "agcl"
        || lower == "ag-cl"
        || lower.starts_with("agcl/")
        || lower.starts_with("ag-cl/")
    {
        return Arc::new(AnthropicAdapter::from_env_with_model(model_name));
    }

    if lower.starts_with("claude") && std::env::var("ANTHROPIC_API_KEY").is_ok() {
        return Arc::new(AnthropicAdapter::from_env_with_model(model_name));
    }

    if lower.starts_with("gpt")
        || lower.starts_with("o1")
        || lower.starts_with("o3")
        || lower.starts_with("llama")
        || lower.starts_with("mixtral")
        || lower.starts_with("gemma")
        || lower.starts_with("deepseek")
        || lower.starts_with("gemini")
        || lower.contains("openrouter")
        || lower.contains('/')
    {
        return Arc::new(OpenAIAdapter::from_env_with_model(model_name));
    }

    // Default fallback based on environment keys
    if std::env::var("ANTHROPIC_API_KEY").is_ok()
        && std::env::var("OPENAI_API_KEY").is_err()
        && std::env::var("GROQ_API_KEY").is_err()
        && std::env::var("OPENROUTER_API_KEY").is_err()
    {
        Arc::new(AnthropicAdapter::from_env_with_model(model_name))
    } else {
        Arc::new(OpenAIAdapter::from_env_with_model(model_name))
    }
}

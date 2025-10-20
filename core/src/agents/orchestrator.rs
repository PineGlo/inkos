use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::config::AiRuntimeSelection;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiChatInput {
    pub messages: Vec<AiChatMessage>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiUsageMetrics {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiChatResponse {
    pub provider_id: String,
    pub model: String,
    pub content: String,
    pub usage: Option<AiUsageMetrics>,
    pub raw: Value,
}

pub struct AiOrchestrator {
    client: Client,
}

impl AiOrchestrator {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(45))
            .user_agent("InkOS-Core/0.1 (+https://github.com/inkos)")
            .build()
            .context("failed to construct HTTP client")?;
        Ok(Self { client })
    }

    pub async fn chat(
        &self,
        selection: &AiRuntimeSelection,
        input: AiChatInput,
    ) -> Result<AiChatResponse> {
        match selection.provider.id.as_str() {
            "openai" => self.chat_openai(selection, &input).await,
            "anthropic" => self.chat_anthropic(selection, &input).await,
            "google" => self.chat_gemini(selection, &input).await,
            "ollama" => self.chat_ollama(selection, &input).await,
            "lmstudio" => self.chat_lmstudio(selection, &input).await,
            other => {
                if selection.provider.kind == "local"
                    && selection
                        .provider
                        .capability_tags
                        .iter()
                        .any(|t| t.contains("openai"))
                {
                    self.chat_openai_like(selection, &input, false).await
                } else {
                    Err(anyhow!("Unsupported AI provider: {other}"))
                }
            }
        }
    }

    async fn chat_openai(
        &self,
        selection: &AiRuntimeSelection,
        input: &AiChatInput,
    ) -> Result<AiChatResponse> {
        if selection.secret.is_none() {
            return Err(anyhow!("OpenAI API key is not configured"));
        }
        self.chat_openai_like(selection, input, true)
            .await
            .with_context(|| "OpenAI request failed".to_string())
    }

    async fn chat_openai_like(
        &self,
        selection: &AiRuntimeSelection,
        input: &AiChatInput,
        include_auth: bool,
    ) -> Result<AiChatResponse> {
        let base_url = selection
            .provider
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.openai.com".to_string());
        let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));
        let mut request = self.client.post(url);
        if include_auth {
            let secret = selection
                .secret
                .as_ref()
                .ok_or_else(|| anyhow!("API key missing for provider {}", selection.provider.id))?;
            request = request.bearer_auth(secret);
        }

        let payload = serde_json::json!({
            "model": selection.model.clone(),
            "messages": normalise_messages(&input.messages),
            "temperature": input.temperature.unwrap_or(0.2),
        });

        let response = request.json(&payload).send().await?.error_for_status()?;
        let body: Value = response.json().await?;

        let content = body
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("content"))
            .and_then(|val| val.as_str())
            .unwrap_or_default()
            .to_string();

        Ok(AiChatResponse {
            provider_id: selection.provider.id.clone(),
            model: selection.model.clone(),
            usage: extract_openai_usage(&body),
            content,
            raw: body,
        })
    }

    async fn chat_lmstudio(
        &self,
        selection: &AiRuntimeSelection,
        input: &AiChatInput,
    ) -> Result<AiChatResponse> {
        self.chat_openai_like(selection, input, false).await
    }

    async fn chat_anthropic(
        &self,
        selection: &AiRuntimeSelection,
        input: &AiChatInput,
    ) -> Result<AiChatResponse> {
        let secret = selection
            .secret
            .as_ref()
            .ok_or_else(|| anyhow!("Anthropic API key is not configured"))?;
        let base_url = selection
            .provider
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.anthropic.com".to_string());
        let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));
        let mut system_prompt = String::new();
        let mut messages = Vec::new();
        for msg in &input.messages {
            match msg.role.as_str() {
                "system" => {
                    if !system_prompt.is_empty() {
                        system_prompt.push_str("\n\n");
                    }
                    system_prompt.push_str(&msg.content);
                }
                "assistant" | "user" => {
                    messages.push(serde_json::json!({
                        "role": msg.role,
                        "content": [{"type": "text", "text": msg.content}],
                    }));
                }
                _ => {}
            }
        }

        if messages.is_empty() {
            messages.push(serde_json::json!({
                "role": "user",
                "content": [{"type": "text", "text": "Hello from InkOS"}],
            }));
        }

        let payload = serde_json::json!({
            "model": selection.model.clone(),
            "max_tokens": 1024,
            "system": if system_prompt.is_empty() { Value::Null } else { Value::String(system_prompt.clone()) },
            "messages": messages,
            "temperature": input.temperature.unwrap_or(0.2),
        });

        let response = self
            .client
            .post(url)
            .header("x-api-key", secret)
            .header("anthropic-version", "2023-06-01")
            .json(&payload)
            .send()
            .await?
            .error_for_status()?;
        let body: Value = response.json().await?;
        let content = body
            .get("content")
            .and_then(|c| c.get(0))
            .and_then(|part| part.get("text"))
            .and_then(|text| text.as_str())
            .unwrap_or_default()
            .to_string();
        Ok(AiChatResponse {
            provider_id: selection.provider.id.clone(),
            model: selection.model.clone(),
            usage: extract_anthropic_usage(&body),
            content,
            raw: body,
        })
    }

    async fn chat_gemini(
        &self,
        selection: &AiRuntimeSelection,
        input: &AiChatInput,
    ) -> Result<AiChatResponse> {
        let secret = selection
            .secret
            .as_ref()
            .ok_or_else(|| anyhow!("Gemini API key is not configured"))?;
        let base_url = selection
            .provider
            .base_url
            .clone()
            .unwrap_or_else(|| "https://generativelanguage.googleapis.com/v1beta".to_string());
        let endpoint = format!(
            "{}/{}:generateContent?key={}",
            base_url.trim_end_matches('/'),
            selection.model,
            secret
        );

        let conversation = build_conversation_prompt(&input.messages);
        let payload = serde_json::json!({
            "contents": [
                {
                    "role": "user",
                    "parts": [{"text": conversation}]
                }
            ],
            "generationConfig": {
                "temperature": input.temperature.unwrap_or(0.2)
            }
        });

        let response = self
            .client
            .post(endpoint)
            .json(&payload)
            .send()
            .await?
            .error_for_status()?;
        let body: Value = response.json().await?;
        let content = body
            .get("candidates")
            .and_then(|c| c.get(0))
            .and_then(|cand| cand.get("content"))
            .and_then(|content| content.get("parts"))
            .and_then(|parts| parts.get(0))
            .and_then(|part| part.get("text"))
            .and_then(|text| text.as_str())
            .unwrap_or_default()
            .to_string();
        Ok(AiChatResponse {
            provider_id: selection.provider.id.clone(),
            model: selection.model.clone(),
            usage: None,
            content,
            raw: body,
        })
    }

    async fn chat_ollama(
        &self,
        selection: &AiRuntimeSelection,
        input: &AiChatInput,
    ) -> Result<AiChatResponse> {
        let base_url = selection
            .provider
            .base_url
            .clone()
            .unwrap_or_else(|| "http://127.0.0.1:11434".to_string());
        let url = format!("{}/api/chat", base_url.trim_end_matches('/'));
        let payload = serde_json::json!({
            "model": selection.model.clone(),
            "messages": normalise_messages(&input.messages),
            "stream": false,
            "options": {
                "temperature": input.temperature.unwrap_or(0.2)
            }
        });
        let response = self
            .client
            .post(url)
            .json(&payload)
            .send()
            .await?
            .error_for_status()?;
        let body: Value = response.json().await?;
        let content = body
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or_default()
            .to_string();
        Ok(AiChatResponse {
            provider_id: selection.provider.id.clone(),
            model: selection.model.clone(),
            usage: None,
            content,
            raw: body,
        })
    }
}

fn normalise_messages(messages: &[AiChatMessage]) -> Vec<Value> {
    messages
        .iter()
        .map(|m| {
            let role = match m.role.to_lowercase().as_str() {
                "system" => "system",
                "assistant" => "assistant",
                _ => "user",
            };
            serde_json::json!({
                "role": role,
                "content": m.content,
            })
        })
        .collect()
}

fn extract_openai_usage(body: &Value) -> Option<AiUsageMetrics> {
    body.get("usage").map(|usage| AiUsageMetrics {
        prompt_tokens: usage
            .get("prompt_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        completion_tokens: usage
            .get("completion_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        total_tokens: usage
            .get("total_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
    })
}

fn extract_anthropic_usage(body: &Value) -> Option<AiUsageMetrics> {
    body.get("usage").map(|usage| AiUsageMetrics {
        prompt_tokens: usage
            .get("input_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        completion_tokens: usage
            .get("output_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        total_tokens: None,
    })
}

fn build_conversation_prompt(messages: &[AiChatMessage]) -> String {
    let mut sections = Vec::new();
    for msg in messages {
        sections.push(format!(
            "{}: {}",
            msg.role.to_uppercase(),
            msg.content.trim()
        ));
    }
    sections.join("\n\n")
}

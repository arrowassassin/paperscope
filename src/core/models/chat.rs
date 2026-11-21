//! A minimal client for OpenAI-compatible chat servers.
//!
//! The same wire format is spoken by Ollama, LM Studio, vLLM, llama.cpp's
//! server and OpenRouter, so one client covers the local polish engine and the
//! opt-in cloud fallback. Nothing here is required for search or for the
//! extractive answer path: the client is only reachable through
//! `paper answer --polish`.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::time::Duration;

/// Upper bound on a chat reply. A polished answer is a paragraph or two.
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

/// A chat endpoint that speaks the OpenAI `/v1/chat/completions` format.
#[derive(Debug, Clone)]
pub struct ChatClient {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct Message {
    content: Content,
}

/// Servers return either a plain string or a list of content parts.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Parts(Vec<Part>),
}

#[derive(Debug, Deserialize)]
struct Part {
    #[serde(default)]
    text: String,
}

impl Content {
    fn into_text(self) -> String {
        match self {
            Content::Text(text) => text,
            Content::Parts(parts) => parts
                .into_iter()
                .map(|p| p.text)
                .collect::<Vec<_>>()
                .join(""),
        }
    }
}

impl ChatClient {
    /// Build a client for `base_url`.
    ///
    /// `api_key` is optional: local servers usually need none, cloud servers
    /// do. The URL may or may not already end in `/v1`.
    pub fn new(
        base_url: &str,
        api_key: Option<String>,
        model: &str,
        timeout: Duration,
    ) -> Result<Self> {
        if base_url.trim().is_empty() {
            bail!("no base URL configured for the chat engine");
        }
        if model.trim().is_empty() {
            bail!("no model configured for the chat engine");
        }
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .context("building the HTTP client")?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model: model.to_string(),
        })
    }

    fn endpoint(&self) -> String {
        if self.base_url.ends_with("/v1") {
            format!("{}/chat/completions", self.base_url)
        } else {
            format!("{}/v1/chat/completions", self.base_url)
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// Send one system + one user message and return the assistant's text.
    pub async fn chat(&self, system: &str, user: &str) -> Result<String> {
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "temperature": 0,
            "stream": false,
        });

        let mut request = self.client.post(self.endpoint()).json(&body);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        let response = request
            .send()
            .await
            .with_context(|| format!("calling {}", self.endpoint()))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!("chat engine returned {status}: {}", truncate(&body, 400));
        }
        // A chat reply is a few kilobytes at most. Refuse anything larger
        // rather than letting a broken or hostile server exhaust memory.
        if let Some(len) = response.content_length() {
            if len > MAX_RESPONSE_BYTES as u64 {
                bail!("chat engine response is too large ({len} bytes)");
            }
        }
        let body = response.text().await.context("reading the chat response")?;
        if body.len() > MAX_RESPONSE_BYTES {
            bail!("chat engine response is too large ({} bytes)", body.len());
        }
        let parsed: ChatResponse =
            serde_json::from_str(&body).context("parsing the chat engine response")?;
        let text = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content.into_text())
            .unwrap_or_default();
        if text.trim().is_empty() {
            bail!("chat engine returned an empty answer");
        }
        Ok(text)
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max).collect();
    format!("{head}…")
}

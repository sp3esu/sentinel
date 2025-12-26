//! Token counter implementation
//!
//! Uses tiktoken-rs for accurate token counting compatible with OpenAI models.
//! Provides both a basic TokenCounter and a thread-safe SharedTokenCounter.

use std::sync::{Arc, RwLock};

use tiktoken_rs::{get_bpe_from_model, CoreBPE};

use crate::error::AppResult;

/// Token counter for various models
pub struct TokenCounter {
    /// Cached encoders for different models
    encoders: std::collections::HashMap<String, CoreBPE>,
}

impl TokenCounter {
    /// Create a new token counter
    pub fn new() -> Self {
        Self {
            encoders: std::collections::HashMap::new(),
        }
    }

    /// Get or create an encoder for a model
    fn get_encoder(&mut self, model: &str) -> &CoreBPE {
        if !self.encoders.contains_key(model) {
            let encoder = match get_bpe_from_model(model) {
                Ok(e) => e,
                Err(e) => {
                    // Fall back to gpt-4 encoder for unknown models
                    tracing::warn!(
                        "Unknown model '{}', falling back to gpt-4 encoder: {}",
                        model,
                        e
                    );
                    get_bpe_from_model("gpt-4").expect("gpt-4 encoder should exist")
                }
            };

            self.encoders.insert(model.to_string(), encoder);
        }

        self.encoders.get(model).unwrap()
    }

    /// Count tokens in a text string
    pub fn count_tokens(&mut self, model: &str, text: &str) -> usize {
        let encoder = self.get_encoder(model);
        encoder.encode_with_special_tokens(text).len()
    }

    /// Count tokens in a chat message
    pub fn count_message_tokens(
        &mut self,
        model: &str,
        role: &str,
        content: &str,
        name: Option<&str>,
    ) -> usize {
        let encoder = self.get_encoder(model);

        // Token overhead varies by model
        // For gpt-4, gpt-3.5-turbo, and newer models
        let tokens_per_message = 3; // <|start|>{role/name}\n{content}<|end|>\n
        let tokens_per_name = 1;

        let mut count = tokens_per_message;
        count += encoder.encode_with_special_tokens(role).len();
        count += encoder.encode_with_special_tokens(content).len();

        if let Some(n) = name {
            count += encoder.encode_with_special_tokens(n).len();
            count += tokens_per_name;
        }

        count
    }

    /// Count tokens for a complete chat completion request
    pub fn count_chat_request_tokens(
        &mut self,
        model: &str,
        messages: &[(&str, &str, Option<&str>)], // (role, content, name)
    ) -> usize {
        let mut total = 0;

        for (role, content, name) in messages {
            total += self.count_message_tokens(model, role, content, *name);
        }

        // Add reply priming tokens (every reply is primed with <|start|>assistant<|message|>)
        total += 3;

        total
    }
}

impl Default for TokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe token counter wrapper
///
/// Uses a RwLock to allow concurrent reads while protecting writes.
/// This is suitable for use in async handlers where multiple requests
/// may need to count tokens concurrently.
#[derive(Clone)]
pub struct SharedTokenCounter {
    inner: Arc<RwLock<TokenCounter>>,
}

impl SharedTokenCounter {
    /// Create a new shared token counter
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(TokenCounter::new())),
        }
    }

    /// Count tokens in a text string
    pub fn count_tokens(&self, model: &str, text: &str) -> AppResult<usize> {
        let mut counter = self
            .inner
            .write()
            .map_err(|e| anyhow::anyhow!("Failed to acquire token counter lock: {}", e))?;
        Ok(counter.count_tokens(model, text))
    }

    /// Count tokens in a chat message
    pub fn count_message_tokens(
        &self,
        model: &str,
        role: &str,
        content: &str,
        name: Option<&str>,
    ) -> AppResult<usize> {
        let mut counter = self
            .inner
            .write()
            .map_err(|e| anyhow::anyhow!("Failed to acquire token counter lock: {}", e))?;
        Ok(counter.count_message_tokens(model, role, content, name))
    }

    /// Count tokens for a complete chat completion request
    pub fn count_chat_request_tokens(
        &self,
        model: &str,
        messages: &[(&str, &str, Option<&str>)],
    ) -> AppResult<usize> {
        let mut counter = self
            .inner
            .write()
            .map_err(|e| anyhow::anyhow!("Failed to acquire token counter lock: {}", e))?;
        Ok(counter.count_chat_request_tokens(model, messages))
    }

    /// Count tokens for chat messages (convenience method)
    ///
    /// Takes a slice of ChatMessage-like tuples and counts total tokens.
    pub fn count_chat_messages(
        &self,
        model: &str,
        messages: &[(String, String, Option<String>)],
    ) -> AppResult<usize> {
        let refs: Vec<(&str, &str, Option<&str>)> = messages
            .iter()
            .map(|(role, content, name)| {
                (role.as_str(), content.as_str(), name.as_deref())
            })
            .collect();
        self.count_chat_request_tokens(model, &refs)
    }
}

impl Default for SharedTokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_tokens() {
        let mut counter = TokenCounter::new();
        let count = counter.count_tokens("gpt-4", "Hello, world!");
        assert!(count > 0);
    }

    #[test]
    fn test_count_message_tokens() {
        let mut counter = TokenCounter::new();
        let count = counter.count_message_tokens("gpt-4", "user", "Hello!", None);
        assert!(count > 0);
    }

    #[test]
    fn test_unknown_model_fallback() {
        let mut counter = TokenCounter::new();
        // Should not panic, should fall back to gpt-4 encoder
        let count = counter.count_tokens("unknown-model-xyz", "Hello");
        assert!(count > 0);
    }

    #[test]
    fn test_shared_counter() {
        let counter = SharedTokenCounter::new();
        let count = counter.count_tokens("gpt-4", "Hello, world!").unwrap();
        assert!(count > 0);
    }

    #[test]
    fn test_shared_counter_message_tokens() {
        let counter = SharedTokenCounter::new();
        let count = counter
            .count_message_tokens("gpt-4", "user", "Hello!", None)
            .unwrap();
        assert!(count > 0);
    }

    #[test]
    fn test_chat_request_tokens() {
        let mut counter = TokenCounter::new();
        let messages = vec![
            ("system", "You are a helpful assistant.", None),
            ("user", "Hello!", None),
        ];
        let count = counter.count_chat_request_tokens("gpt-4", &messages);
        assert!(count > 0);
    }
}

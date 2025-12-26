//! Token counter implementation
//!
//! Uses tiktoken-rs for accurate token counting compatible with OpenAI models.

use tiktoken_rs::{get_bpe_from_model, CoreBPE};

use crate::error::{AppError, AppResult};

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
    fn get_encoder(&mut self, model: &str) -> AppResult<&CoreBPE> {
        if !self.encoders.contains_key(model) {
            let encoder = get_bpe_from_model(model).map_err(|e| {
                // Fall back to cl100k_base for unknown models
                tracing::warn!("Unknown model {}, falling back to cl100k_base: {}", model, e);
                get_bpe_from_model("gpt-4").expect("gpt-4 encoder should exist")
            });

            let encoder = match encoder {
                Ok(e) => e,
                Err(e) => e, // Use the fallback encoder
            };

            self.encoders.insert(model.to_string(), encoder);
        }

        Ok(self.encoders.get(model).unwrap())
    }

    /// Count tokens in a text string
    pub fn count_tokens(&mut self, model: &str, text: &str) -> AppResult<usize> {
        let encoder = self.get_encoder(model)?;
        let tokens = encoder.encode_with_special_tokens(text);
        Ok(tokens.len())
    }

    /// Count tokens in a chat message
    pub fn count_message_tokens(
        &mut self,
        model: &str,
        role: &str,
        content: &str,
        name: Option<&str>,
    ) -> AppResult<usize> {
        let encoder = self.get_encoder(model)?;

        // Token overhead varies by model
        let tokens_per_message = 3; // For most recent models
        let tokens_per_name = 1;

        let mut count = tokens_per_message;
        count += encoder.encode_with_special_tokens(role).len();
        count += encoder.encode_with_special_tokens(content).len();

        if let Some(n) = name {
            count += encoder.encode_with_special_tokens(n).len();
            count += tokens_per_name;
        }

        Ok(count)
    }

    /// Count tokens for a complete chat completion request
    pub fn count_chat_request_tokens(
        &mut self,
        model: &str,
        messages: &[(&str, &str, Option<&str>)], // (role, content, name)
    ) -> AppResult<usize> {
        let mut total = 0;

        for (role, content, name) in messages {
            total += self.count_message_tokens(model, role, content, *name)?;
        }

        // Add reply priming tokens
        total += 3;

        Ok(total)
    }
}

impl Default for TokenCounter {
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
        let count = counter.count_tokens("gpt-4", "Hello, world!").unwrap();
        assert!(count > 0);
    }

    #[test]
    fn test_count_message_tokens() {
        let mut counter = TokenCounter::new();
        let count = counter
            .count_message_tokens("gpt-4", "user", "Hello!", None)
            .unwrap();
        assert!(count > 0);
    }

    #[test]
    fn test_unknown_model_fallback() {
        let mut counter = TokenCounter::new();
        // Should not panic, should fall back to gpt-4 encoder
        let count = counter.count_tokens("unknown-model-xyz", "Hello").unwrap();
        assert!(count > 0);
    }
}

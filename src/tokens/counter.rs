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
    use std::thread;

    // ===========================================
    // TokenCounter Basic Tests
    // ===========================================

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

    // ===========================================
    // Model-Specific Token Counting Tests
    // ===========================================

    #[test]
    fn test_count_tokens_gpt4() {
        let mut counter = TokenCounter::new();
        let text = "The quick brown fox jumps over the lazy dog.";
        let count = counter.count_tokens("gpt-4", text);
        // GPT-4 should tokenize this into approximately 10 tokens
        assert!(count >= 8 && count <= 12, "Expected 8-12 tokens, got {}", count);
    }

    #[test]
    fn test_count_tokens_gpt35_turbo() {
        let mut counter = TokenCounter::new();
        let text = "Hello, how are you today?";
        let count = counter.count_tokens("gpt-3.5-turbo", text);
        assert!(count > 0);
        // GPT-3.5-turbo uses same tokenizer as GPT-4
        assert!(count >= 5 && count <= 10, "Expected 5-10 tokens, got {}", count);
    }

    #[test]
    fn test_count_tokens_gpt4_turbo() {
        let mut counter = TokenCounter::new();
        let text = "Testing GPT-4 Turbo tokenization.";
        let count = counter.count_tokens("gpt-4-turbo", text);
        assert!(count > 0);
    }

    #[test]
    fn test_count_tokens_claude_fallback() {
        let mut counter = TokenCounter::new();
        // Claude models should fall back to gpt-4 encoder
        let text = "Testing Claude model tokenization.";
        let count = counter.count_tokens("claude-3-opus-20240229", text);
        assert!(count > 0, "Claude fallback should return tokens");

        // Verify it matches gpt-4 (since it falls back)
        let gpt4_count = counter.count_tokens("gpt-4", text);
        assert_eq!(count, gpt4_count, "Claude should use GPT-4 encoder as fallback");
    }

    #[test]
    fn test_count_tokens_empty_string() {
        let mut counter = TokenCounter::new();
        let count = counter.count_tokens("gpt-4", "");
        assert_eq!(count, 0, "Empty string should have 0 tokens");
    }

    #[test]
    fn test_count_tokens_whitespace_only() {
        let mut counter = TokenCounter::new();
        let count = counter.count_tokens("gpt-4", "   \t\n  ");
        // Whitespace tokens should be minimal
        assert!(count <= 3, "Whitespace should have few tokens, got {}", count);
    }

    #[test]
    fn test_count_tokens_unicode() {
        let mut counter = TokenCounter::new();
        let text = "Hello! Bonjour! Hola!";
        let count = counter.count_tokens("gpt-4", text);
        assert!(count > 0, "Unicode text should have tokens");
    }

    #[test]
    fn test_count_tokens_special_characters() {
        let mut counter = TokenCounter::new();
        let text = "Hello @user! Check out #rust for $100 off!";
        let count = counter.count_tokens("gpt-4", text);
        assert!(count > 0, "Special characters should be tokenized");
    }

    #[test]
    fn test_count_tokens_code() {
        let mut counter = TokenCounter::new();
        let code = r#"fn main() { println!("Hello, world!"); }"#;
        let count = counter.count_tokens("gpt-4", code);
        assert!(count > 0, "Code should be tokenized");
    }

    #[test]
    fn test_count_tokens_json() {
        let mut counter = TokenCounter::new();
        let json = r#"{"name": "John", "age": 30, "city": "New York"}"#;
        let count = counter.count_tokens("gpt-4", json);
        assert!(count > 0, "JSON should be tokenized");
    }

    // ===========================================
    // Message Token Counting Tests
    // ===========================================

    #[test]
    fn test_count_message_tokens_with_name() {
        let mut counter = TokenCounter::new();
        let count_without_name = counter.count_message_tokens("gpt-4", "user", "Hello!", None);
        let count_with_name = counter.count_message_tokens("gpt-4", "user", "Hello!", Some("John"));

        // With name should have more tokens due to the name token overhead
        assert!(count_with_name > count_without_name,
            "Message with name should have more tokens: {} vs {}",
            count_with_name, count_without_name);
    }

    #[test]
    fn test_count_message_tokens_different_roles() {
        let mut counter = TokenCounter::new();
        let content = "Test message content";

        let user_count = counter.count_message_tokens("gpt-4", "user", content, None);
        let assistant_count = counter.count_message_tokens("gpt-4", "assistant", content, None);
        let system_count = counter.count_message_tokens("gpt-4", "system", content, None);

        // All should have tokens
        assert!(user_count > 0);
        assert!(assistant_count > 0);
        assert!(system_count > 0);

        // Token counts should be similar (only role name differs)
        // assistant is longer than user/system so may have slightly more tokens
        assert!(assistant_count >= user_count);
    }

    #[test]
    fn test_count_message_tokens_long_content() {
        let mut counter = TokenCounter::new();
        let short_content = "Hi";
        let long_content = "This is a much longer message that should have significantly more tokens than the short message above. It contains multiple sentences and provides more context.";

        let short_count = counter.count_message_tokens("gpt-4", "user", short_content, None);
        let long_count = counter.count_message_tokens("gpt-4", "user", long_content, None);

        assert!(long_count > short_count,
            "Longer content should have more tokens: {} vs {}", long_count, short_count);
    }

    // ===========================================
    // Chat Request Token Counting Tests
    // ===========================================

    #[test]
    fn test_chat_request_tokens_single_message() {
        let mut counter = TokenCounter::new();
        let messages = vec![("user", "Hello!", None)];
        let count = counter.count_chat_request_tokens("gpt-4", &messages);
        // Should include message tokens + reply priming (3 tokens)
        assert!(count >= 4, "Single message should have at least 4 tokens");
    }

    #[test]
    fn test_chat_request_tokens_conversation() {
        let mut counter = TokenCounter::new();
        let messages = vec![
            ("system", "You are a helpful assistant.", None),
            ("user", "What is 2+2?", None),
            ("assistant", "2+2 equals 4.", None),
            ("user", "Thanks!", None),
        ];
        let count = counter.count_chat_request_tokens("gpt-4", &messages);
        assert!(count > 10, "Conversation should have significant tokens");
    }

    #[test]
    fn test_chat_request_tokens_empty_messages() {
        let mut counter = TokenCounter::new();
        let messages: Vec<(&str, &str, Option<&str>)> = vec![];
        let count = counter.count_chat_request_tokens("gpt-4", &messages);
        // Should only have reply priming tokens (3)
        assert_eq!(count, 3, "Empty messages should only have reply priming tokens");
    }

    #[test]
    fn test_chat_request_tokens_with_names() {
        let mut counter = TokenCounter::new();
        let messages_without_names = vec![
            ("user", "Hello!", None),
        ];
        let messages_with_names = vec![
            ("user", "Hello!", Some("Alice")),
        ];

        let count_without = counter.count_chat_request_tokens("gpt-4", &messages_without_names);
        let count_with = counter.count_chat_request_tokens("gpt-4", &messages_with_names);

        assert!(count_with > count_without,
            "Messages with names should have more tokens");
    }

    // ===========================================
    // Encoder Caching Tests
    // ===========================================

    #[test]
    fn test_encoder_caching() {
        let mut counter = TokenCounter::new();

        // First call should create encoder
        let count1 = counter.count_tokens("gpt-4", "Hello");

        // Second call should use cached encoder
        let count2 = counter.count_tokens("gpt-4", "Hello");

        // Counts should be identical
        assert_eq!(count1, count2);

        // Verify encoder is cached
        assert!(counter.encoders.contains_key("gpt-4"));
    }

    #[test]
    fn test_multiple_encoder_caching() {
        let mut counter = TokenCounter::new();

        // Create encoders for different models
        counter.count_tokens("gpt-4", "Hello");
        counter.count_tokens("gpt-3.5-turbo", "Hello");

        // Both should be cached
        assert!(counter.encoders.contains_key("gpt-4"));
        assert!(counter.encoders.contains_key("gpt-3.5-turbo"));
        assert_eq!(counter.encoders.len(), 2);
    }

    #[test]
    fn test_unknown_model_encoder_caching() {
        let mut counter = TokenCounter::new();

        // Unknown model should fall back to gpt-4 but cache under its own name
        counter.count_tokens("my-custom-model", "Hello");

        // Should be cached under the unknown model name
        assert!(counter.encoders.contains_key("my-custom-model"));
    }

    // ===========================================
    // SharedTokenCounter Tests
    // ===========================================

    #[test]
    fn test_shared_counter_default() {
        let counter = SharedTokenCounter::default();
        let count = counter.count_tokens("gpt-4", "Hello").unwrap();
        assert!(count > 0);
    }

    #[test]
    fn test_shared_counter_clone() {
        let counter1 = SharedTokenCounter::new();
        let counter2 = counter1.clone();

        // Both should work independently
        let count1 = counter1.count_tokens("gpt-4", "Hello").unwrap();
        let count2 = counter2.count_tokens("gpt-4", "Hello").unwrap();

        assert_eq!(count1, count2);
    }

    #[test]
    fn test_shared_counter_count_chat_messages() {
        let counter = SharedTokenCounter::new();
        let messages = vec![
            ("system".to_string(), "You are helpful.".to_string(), None),
            ("user".to_string(), "Hi!".to_string(), Some("Alice".to_string())),
        ];

        let count = counter.count_chat_messages("gpt-4", &messages).unwrap();
        assert!(count > 0, "Chat messages should have tokens");
    }

    #[test]
    fn test_shared_counter_thread_safety() {
        let counter = SharedTokenCounter::new();
        let mut handles = vec![];

        for i in 0..10 {
            let counter_clone = counter.clone();
            let handle = thread::spawn(move || {
                let text = format!("Thread {} message", i);
                counter_clone.count_tokens("gpt-4", &text).unwrap()
            });
            handles.push(handle);
        }

        // All threads should complete successfully
        for handle in handles {
            let count = handle.join().expect("Thread should complete");
            assert!(count > 0, "Each thread should return valid count");
        }
    }

    #[test]
    fn test_shared_counter_concurrent_different_models() {
        let counter = SharedTokenCounter::new();
        let mut handles = vec![];

        let models = vec!["gpt-4", "gpt-3.5-turbo", "gpt-4-turbo"];

        for model in models {
            let counter_clone = counter.clone();
            let model_owned = model.to_string();
            let handle = thread::spawn(move || {
                counter_clone.count_tokens(&model_owned, "Hello world").unwrap()
            });
            handles.push(handle);
        }

        for handle in handles {
            let count = handle.join().expect("Thread should complete");
            assert!(count > 0);
        }
    }

    // ===========================================
    // Default Trait Tests
    // ===========================================

    #[test]
    fn test_token_counter_default() {
        let counter = TokenCounter::default();
        assert!(counter.encoders.is_empty());
    }

    // ===========================================
    // Edge Cases and Error Handling
    // ===========================================

    #[test]
    fn test_very_long_text() {
        let mut counter = TokenCounter::new();
        let long_text = "word ".repeat(1000);
        let count = counter.count_tokens("gpt-4", &long_text);
        // Should handle long text without issue
        assert!(count > 500, "Long text should have many tokens");
    }

    #[test]
    fn test_newlines_and_tabs() {
        let mut counter = TokenCounter::new();
        let text = "Line 1\nLine 2\n\tIndented line\n";
        let count = counter.count_tokens("gpt-4", text);
        assert!(count > 0);
    }

    #[test]
    fn test_numbers() {
        let mut counter = TokenCounter::new();
        let text = "The answer is 42 and pi is 3.14159";
        let count = counter.count_tokens("gpt-4", text);
        assert!(count > 0);
    }

    #[test]
    fn test_mixed_content() {
        let mut counter = TokenCounter::new();
        let text = r#"
        Here's some code:
        ```python
        def hello():
            print("Hello, World!")
        ```
        And some JSON: {"key": "value", "number": 123}
        "#;
        let count = counter.count_tokens("gpt-4", text);
        assert!(count > 10, "Mixed content should have many tokens");
    }
}

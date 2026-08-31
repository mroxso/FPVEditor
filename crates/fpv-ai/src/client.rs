//! An `async-openai` client wired to a configurable [`ProviderConfig`], so
//! the same code path talks to OpenAI, Azure OpenAI, Ollama, LM Studio,
//! vLLM, or anything else that speaks the OpenAI chat-completions dialect.

use async_openai::config::OpenAIConfig;
use async_openai::types::chat::{
    ChatCompletionRequestMessage, ChatCompletionRequestUserMessageArgs,
    ChatCompletionTools, CreateChatCompletionRequestArgs, CreateChatCompletionResponse,
};
use async_openai::Client;

use crate::config::ProviderConfig;

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("request to AI provider failed: {0}")]
    Request(String),
    #[error("provider returned no choices")]
    NoChoices,
}

pub struct AiClient {
    inner: Client<OpenAIConfig>,
    model: String,
}

impl AiClient {
    pub fn new(config: &ProviderConfig) -> Self {
        let mut openai_config = OpenAIConfig::new().with_api_base(config.base_url.clone());
        if let Some(key) = &config.api_key {
            openai_config = openai_config.with_api_key(key.clone());
        }
        Self {
            inner: Client::with_config(openai_config),
            model: config.model.clone(),
        }
    }

    /// A minimal request used by the settings panel's "test connection" button.
    pub async fn test_connection(&self) -> Result<(), AiError> {
        let message: ChatCompletionRequestMessage = ChatCompletionRequestUserMessageArgs::default()
            .content("ping")
            .build()
            .map_err(|e| AiError::Request(e.to_string()))?
            .into();
        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.model)
            .messages(vec![message])
            .max_tokens(1u32)
            .build()
            .map_err(|e| AiError::Request(e.to_string()))?;
        self.inner
            .chat()
            .create(request)
            .await
            .map_err(|e| AiError::Request(e.to_string()))?;
        Ok(())
    }

    /// Send a chat-completion request, optionally with tool definitions for
    /// function calling (PLAN.md section 4.2).
    pub async fn chat(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Option<Vec<ChatCompletionTools>>,
    ) -> Result<CreateChatCompletionResponse, AiError> {
        let mut builder = CreateChatCompletionRequestArgs::default();
        builder.model(&self.model).messages(messages);
        if let Some(tools) = tools {
            builder.tools(tools);
        }
        let request = builder.build().map_err(|e| AiError::Request(e.to_string()))?;
        self.inner
            .chat()
            .create(request)
            .await
            .map_err(|e| AiError::Request(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderConfig;
    use async_openai::types::chat::ChatCompletionRequestUserMessageArgs;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn canned_completion_body(content: &str) -> serde_json::Value {
        serde_json::json!({
            "id": "chatcmpl-test",
            "object": "chat.completion",
            "created": 1_700_000_000,
            "model": "test-model",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": content
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 5,
                "total_tokens": 10
            }
        })
    }

    #[tokio::test]
    async fn chat_completion_against_a_local_mock_server_parses_the_reply() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(canned_completion_body("hello from the mock")))
            .mount(&server)
            .await;

        let config = ProviderConfig::new(server.uri(), "test-model");
        let client = AiClient::new(&config);

        let message: ChatCompletionRequestMessage = ChatCompletionRequestUserMessageArgs::default()
            .content("hi")
            .build()
            .unwrap()
            .into();
        let response = client.chat(vec![message], None).await.unwrap();

        let content = response.choices[0].message.content.as_deref().unwrap();
        assert_eq!(content, "hello from the mock");
    }

    #[tokio::test]
    async fn test_connection_succeeds_against_a_healthy_mock_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(canned_completion_body("pong")))
            .mount(&server)
            .await;

        let config = ProviderConfig::new(server.uri(), "test-model");
        let client = AiClient::new(&config);
        client.test_connection().await.unwrap();
    }

    #[tokio::test]
    async fn test_connection_surfaces_an_error_on_a_5xx_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let config = ProviderConfig::new(server.uri(), "test-model");
        let client = AiClient::new(&config);
        assert!(client.test_connection().await.is_err());
    }
}

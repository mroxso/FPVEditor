//! The internal chat/agent loop (PLAN.md section 4.3): sends the user's
//! message plus the tool catalog to the configured provider, executes any
//! tool calls against the project's [`CommandBus`], feeds the results back,
//! and repeats until the model returns a plain text reply.

use async_openai::types::chat::{
    ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
    ChatCompletionRequestToolMessageArgs, ChatCompletionRequestUserMessageArgs,
};
use fpv_core::CommandBus;

use crate::client::{AiClient, AiError};
use crate::tools::{self, ToolError};

const MAX_TOOL_ITERATIONS: usize = 8;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error(transparent)]
    Ai(#[from] AiError),
    #[error(transparent)]
    Tool(#[from] ToolError),
    #[error("agent exceeded {0} tool-call iterations without a final answer")]
    TooManyIterations(usize),
    #[error("message build error: {0}")]
    MessageBuild(String),
}

/// Run one turn of the agent: given a user prompt, converse with the model
/// (looping over tool calls it makes against the timeline) until it
/// produces a final text reply.
pub async fn run_turn(
    client: &AiClient,
    bus: &mut CommandBus,
    user_prompt: &str,
) -> Result<String, AgentError> {
    let mut messages: Vec<ChatCompletionRequestMessage> = vec![ChatCompletionRequestUserMessageArgs::default()
        .content(user_prompt)
        .build()
        .map_err(|e| AgentError::MessageBuild(e.to_string()))?
        .into()];

    let tools = tools::to_chat_tools();

    for _ in 0..MAX_TOOL_ITERATIONS {
        let response = client.chat(messages.clone(), Some(tools.clone())).await?;
        let choice = response.choices.first().ok_or(AiError::NoChoices)?;
        let message = &choice.message;

        if let Some(tool_calls) = &message.tool_calls {
            if tool_calls.is_empty() {
                return Ok(message.content.clone().unwrap_or_default());
            }

            messages.push(
                ChatCompletionRequestAssistantMessageArgs::default()
                    .tool_calls(tool_calls.clone())
                    .build()
                    .map_err(|e| AgentError::MessageBuild(e.to_string()))?
                    .into(),
            );

            for call in tool_calls {
                let async_openai::types::chat::ChatCompletionMessageToolCalls::Function(call) = call
                else {
                    // Custom (non-function) tool calls aren't part of our catalog.
                    continue;
                };
                let args: serde_json::Value =
                    serde_json::from_str(&call.function.arguments).unwrap_or(serde_json::json!({}));
                let result = match tools::dispatch(bus, &call.function.name, args) {
                    Ok(v) => v,
                    Err(e) => serde_json::json!({ "error": e.to_string() }),
                };
                messages.push(
                    ChatCompletionRequestToolMessageArgs::default()
                        .tool_call_id(call.id.clone())
                        .content(result.to_string())
                        .build()
                        .map_err(|e| AgentError::MessageBuild(e.to_string()))?
                        .into(),
                );
            }
            continue;
        }

        return Ok(message.content.clone().unwrap_or_default());
    }

    Err(AgentError::TooManyIterations(MAX_TOOL_ITERATIONS))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderConfig;
    use fpv_core::{Command, Project, TrackKind};
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn tool_call_response(tool_name: &str, args: serde_json::Value) -> serde_json::Value {
        json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "created": 1_700_000_000,
            "model": "test-model",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": { "name": tool_name, "arguments": args.to_string() }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
        })
    }

    fn final_text_response(text: &str) -> serde_json::Value {
        json!({
            "id": "chatcmpl-2",
            "object": "chat.completion",
            "created": 1_700_000_001,
            "model": "test-model",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": text },
                "finish_reason": "stop"
            }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
        })
    }

    #[tokio::test]
    async fn agent_executes_a_tool_call_then_returns_the_models_final_text() {
        let server = MockServer::start().await;

        let mut bus = CommandBus::new(Project::new("test"));
        bus.execute(Command::AddTrack {
            kind: TrackKind::Video,
            name: "V1".into(),
        })
        .unwrap();
        let track_id = bus.project().tracks[0].id;

        let tool_args = json!({
            "track_id": track_id,
            "clip": { "source_path": "run.mp4", "in_point": 0, "out_point": 2_000_000, "position": 0 }
        });

        // First call: model asks to add a clip. Second call: model answers.
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(tool_call_response("add_clip", tool_args)))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(final_text_response("Added the clip.")))
            .mount(&server)
            .await;

        let config = ProviderConfig::new(server.uri(), "test-model");
        let client = AiClient::new(&config);

        let reply = run_turn(&client, &mut bus, "cut out my crash and add the good run").await.unwrap();

        assert_eq!(reply, "Added the clip.");
        assert_eq!(bus.project().clips.len(), 1, "the tool call should have actually mutated the timeline");
    }

    #[tokio::test]
    async fn agent_returns_immediately_when_the_model_has_no_tool_calls() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(final_text_response("Nothing to do here.")))
            .mount(&server)
            .await;

        let config = ProviderConfig::new(server.uri(), "test-model");
        let client = AiClient::new(&config);
        let mut bus = CommandBus::new(Project::new("test"));

        let reply = run_turn(&client, &mut bus, "hello").await.unwrap();
        assert_eq!(reply, "Nothing to do here.");
    }
}

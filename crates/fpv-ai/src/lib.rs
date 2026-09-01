//! `fpv-ai`: an OpenAI-compatible client (configurable base URL/key/model,
//! per PLAN.md section 4.1) plus the internal chat agent loop and the tool
//! catalog shared with the MCP server, per section 4.2-4.3.

pub mod agent;
pub mod client;
pub mod config;
pub mod tools;

pub use agent::{run_turn, AgentError};
pub use client::{AiClient, AiError};
pub use config::{Preset, ProviderConfig};
pub use tools::{catalog, dispatch, to_chat_tools, ToolError, ToolSpec};

//! `fpv-mcp`: an MCP server exposing the editor's command API as tools for
//! external agents (Claude Code and friends), per PLAN.md section 4.4. The
//! tool catalog and dispatch logic are reused directly from `fpv-ai`
//! (section 4.2's "defined once, exposed twice") — this crate is just the
//! MCP protocol adapter around them.

use std::sync::{Arc, Mutex};

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::Value;

use fpv_core::{CommandBus, Project};

pub struct FpvMcpServer {
    bus: Arc<Mutex<CommandBus>>,
}

impl FpvMcpServer {
    pub fn new(bus: CommandBus) -> Self {
        Self {
            bus: Arc::new(Mutex::new(bus)),
        }
    }

    /// Shared handle to the underlying project state, e.g. so the host
    /// process can also render/export outside of tool calls.
    pub fn bus(&self) -> Arc<Mutex<CommandBus>> {
        self.bus.clone()
    }
}

fn spec_to_tool(spec: fpv_ai::ToolSpec) -> Tool {
    let schema = match spec.parameters {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    Tool::new(spec.name.to_string(), spec.description.to_string(), schema)
}

impl ServerHandler for FpvMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "FPV Editor: drive timeline editing, stabilization, color grading, and export \
             through these tools instead of a GUI.",
        )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let tools = fpv_ai::catalog().into_iter().map(spec_to_tool).collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let args = request
            .arguments
            .map(Value::Object)
            .unwrap_or_else(|| Value::Object(Default::default()));

        let mut bus = self
            .bus
            .lock()
            .map_err(|_| McpError::internal_error("command bus lock poisoned", None))?;

        let response = match fpv_ai::dispatch(&mut bus, &request.name, args) {
            Ok(value) => CallToolResult::success(vec![ContentBlock::text(value.to_string())]),
            Err(err) => CallToolResult::error(vec![ContentBlock::text(err.to_string())]),
        };
        Ok(CallToolResponse::Complete(response))
    }
}

/// Serve the MCP server over stdio (what `fpv-cli mcp-serve` runs, per
/// PLAN.md section 4.4) until the peer disconnects, returning the project's
/// final state so the caller can persist whatever the external agent did.
pub async fn serve_stdio(bus: CommandBus) -> anyhow::Result<Project> {
    use rmcp::ServiceExt;
    let server = FpvMcpServer::new(bus);
    let bus_handle = server.bus();
    let running = server.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    let project = bus_handle.lock().unwrap().project().clone();
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fpv_core::{Command, Project, TrackKind};
    use rmcp::ServiceExt;

    fn project_with_track() -> CommandBus {
        let mut bus = CommandBus::new(Project::new("mcp-test"));
        bus.execute(Command::AddTrack {
            kind: TrackKind::Video,
            name: "V1".into(),
        })
        .unwrap();
        bus
    }

    #[test]
    fn spec_to_tool_carries_name_description_and_schema() {
        let spec = fpv_ai::catalog().into_iter().find(|t| t.name == "trim_clip").unwrap();
        let tool = spec_to_tool(spec);
        assert_eq!(tool.name, "trim_clip");
        assert!(tool.description.unwrap().contains("in/out"));
        assert!(tool.input_schema.contains_key("properties"));
    }

    #[tokio::test]
    async fn a_real_mcp_client_can_list_tools_and_mutate_the_timeline_over_a_pipe() {
        let bus = project_with_track();
        let track_id = bus.project().tracks[0].id;
        let server = FpvMcpServer::new(bus);
        let bus_handle = server.bus();

        let (server_io, client_io) = tokio::io::duplex(64 * 1024);
        let server_task = tokio::spawn(async move {
            let running = server.serve(server_io).await.expect("server should start");
            running.waiting().await.ok();
        });

        let client = ().serve(client_io).await.expect("client should connect");

        let tools = client.list_tools(None).await.expect("list_tools should succeed");
        assert!(tools.tools.iter().any(|t| t.name == "add_clip"));
        assert!(tools.tools.iter().any(|t| t.name == "apply_stabilization"));

        let args = serde_json::json!({
            "track_id": track_id,
            "clip": { "source_path": "run.mp4", "in_point": 0, "out_point": 2_000_000, "position": 0 }
        });
        let mut params = CallToolRequestParams::new("add_clip");
        params.arguments = args.as_object().cloned();
        let result = client.call_tool(params).await.expect("call_tool should succeed");
        assert_ne!(result.is_error, Some(true), "add_clip should not report an error");

        assert_eq!(bus_handle.lock().unwrap().project().clips.len(), 1);

        client.cancel().await.ok();
        server_task.await.ok();
    }

    #[tokio::test]
    async fn an_unknown_tool_call_reports_a_tool_error_not_a_protocol_error() {
        let server = FpvMcpServer::new(project_with_track());
        let (server_io, client_io) = tokio::io::duplex(64 * 1024);
        let server_task = tokio::spawn(async move {
            let running = server.serve(server_io).await.expect("server should start");
            running.waiting().await.ok();
        });
        let client = ().serve(client_io).await.expect("client should connect");

        let result = client
            .call_tool(CallToolRequestParams::new("delete_universe"))
            .await
            .expect("the request itself should succeed at the protocol level");
        assert_eq!(result.is_error, Some(true));

        client.cancel().await.ok();
        server_task.await.ok();
    }
}

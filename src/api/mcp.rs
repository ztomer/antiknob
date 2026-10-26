//! Model Context Protocol server, on rmcp (the official Rust SDK).
//!
//! Until 2026-10-26 this file was a hand-rolled JSON-RPC loop: its own
//! framing, its own version negotiation, its own `tools/call` envelope. All
//! of that is upstream's to keep now (owner ruling OW.60) -- what is left is
//! the part only antiknob can say: which tools exist (generated from
//! `registry::COMMANDS`), what each one does to the world (annotations), and
//! how a call reaches `execute_command`.
//!
//! The decisions live in [`AntiknobMcp::tools`] and [`AntiknobMcp::call`],
//! plain synchronous methods tested without a runtime or a pipe; the
//! `ServerHandler` impl below them is a thin adapter, and
//! [`run_mcp_server`] is the only place tokio appears.

use super::dispatch::{execute_command, ApiContext};
use super::registry::Effect;
use super::types::{all_tools, Command, ToolDef};
use anyhow::{Context, Result};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    JsonObject, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex, PoisonError};

/// What a started server offers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct McpOptions {
    /// Offer the tools whose [`Effect`] is `Destructive`. Off by default: a
    /// host that connects to this server with no further say gets the tools
    /// that read and the ones that change easily restored state, and must be
    /// started with `--allow-destructive` to be handed the ones that
    /// overwrite the keymap, the config file or raw firmware state.
    pub allow_destructive: bool,
}

/// The MCP face of the command registry over one [`ApiContext`].
#[derive(Clone)]
pub struct AntiknobMcp {
    ctx: Arc<Mutex<ApiContext>>,
    options: McpOptions,
}

impl AntiknobMcp {
    pub fn new(ctx: ApiContext, options: McpOptions) -> Self {
        Self {
            ctx: Arc::new(Mutex::new(ctx)),
            options,
        }
    }

    fn offered(&self, def: &ToolDef) -> bool {
        self.options.allow_destructive || def.effect != Effect::Destructive
    }

    /// The tools this server lists: the registry's MCP surface, minus the
    /// destructive ones unless opted in, each with annotations and an
    /// output schema.
    pub fn tools(&self) -> Vec<Tool> {
        all_tools()
            .into_iter()
            .filter(|def| self.offered(def))
            .map(|def| tool_of(&def))
            .collect()
    }

    /// Run one tool call. A failure of the COMMAND is an `isError` result
    /// the model can read and act on, never a protocol error.
    pub fn call(&self, name: &str, arguments: Option<JsonObject>) -> CallToolResult {
        if let Some(def) = all_tools().into_iter().find(|d| d.name == name) {
            if !self.offered(&def) {
                return failure(format!(
                    "Tool '{name}' overwrites stored state and is not enabled on this server; \
                     start it with --allow-destructive to use it"
                ));
            }
        }
        let command = serde_json::from_value::<Command>(json!({
            "method": name,
            "params": Value::Object(arguments.unwrap_or_default()),
        }));
        let command = match command {
            Ok(command) => command,
            Err(e) => return failure(format!("Invalid arguments for tool '{name}': {e}")),
        };
        // A poisoned lock means an earlier call panicked, which that call
        // already reported; the context itself is still the one to use.
        let mut ctx = self.ctx.lock().unwrap_or_else(PoisonError::into_inner);
        let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            execute_command(&mut ctx, command)
        }));
        match ran {
            Ok(Ok(value)) => success(value),
            Ok(Err(e)) => failure(format!("Error: {e}")),
            Err(payload) => failure(format!("Internal error: {}", panic_message(&*payload))),
        }
    }
}

fn tool_of(def: &ToolDef) -> Tool {
    let object = |v: &Value| -> JsonObject { v.as_object().cloned().unwrap_or_default() };
    let annotations = ToolAnnotations::new()
        .read_only(def.effect == Effect::Read)
        .destructive(def.effect == Effect::Destructive)
        .idempotent(def.effect != Effect::Destructive)
        // The knob and this Mac are the whole world a tool reaches.
        .open_world(false);
    let mut tool = Tool::new(def.name, def.description, object(&def.input_schema));
    tool.output_schema = Some(Arc::new(object(&def.output_schema)));
    tool.annotations = Some(annotations);
    tool
}

/// A successful reply: the pretty text older clients read, and the same
/// object as `structuredContent` for the ones that validate it against the
/// tool's `outputSchema`. The schema promises an object, so anything else
/// is wrapped rather than sent as a shape the schema forbids.
fn success(value: Value) -> CallToolResult {
    let structured = match value {
        Value::Object(_) => value,
        other => json!({ "result": other }),
    };
    let text = serde_json::to_string_pretty(&structured).unwrap_or_default();
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(structured);
    result
}

fn failure(message: String) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message)])
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(ToString::to_string)
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "Internal panic during tool execution".to_string())
}

impl ServerHandler for AntiknobMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("antiknob", env!("CARGO_PKG_VERSION")))
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: self.tools(),
            ..ListToolsResult::default()
        })
    }

    // A device call blocks this thread on purpose: one client on a local
    // pipe, and the knob takes one HID conversation at a time anyway.
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        Ok(self.call(&request.name, request.arguments).into())
    }
}

/// Run the MCP server on stdio until the client closes it.
///
/// The one place async lives: a current-thread runtime built for the length
/// of the session, so the rest of antiknob stays synchronous.
pub fn run_mcp_server(ctx: ApiContext, options: McpOptions) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("cannot start the MCP runtime")?;
    runtime.block_on(async {
        let service = AntiknobMcp::new(ctx, options)
            .serve(rmcp::transport::stdio())
            .await
            .context("MCP handshake failed")?;
        service.waiting().await.context("MCP session failed")?;
        Ok(())
    })
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;

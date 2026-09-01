use std::{fmt, future::Future, pin::Pin};

use serde_json::Value;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq)]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct McpToolResult {
    pub content: Value,
    pub structured_content: Value,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolClientError {
    Unavailable,
    TimedOut,
    Protocol,
    Server,
    ProcessExited,
}

impl fmt::Display for ToolClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "ArcGIS tool client is unavailable",
            Self::TimedOut => "ArcGIS tool request timed out",
            Self::Protocol => "ArcGIS MCP protocol failed",
            Self::Server => "ArcGIS MCP server returned an error",
            Self::ProcessExited => "ArcGIS MCP server exited",
        })
    }
}

impl std::error::Error for ToolClientError {}

pub trait ArcGisToolClient: Send + Sync {
    fn list_tools<'a>(&'a self) -> BoxFuture<'a, Result<Vec<McpTool>, ToolClientError>>;
    fn call_tool<'a>(
        &'a self,
        name: &'a str,
        arguments: Value,
    ) -> BoxFuture<'a, Result<McpToolResult, ToolClientError>>;
}

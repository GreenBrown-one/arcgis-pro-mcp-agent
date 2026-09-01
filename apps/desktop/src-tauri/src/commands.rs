use std::{
    borrow::Cow,
    ffi::OsString,
    future::Future,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    addin_install::{
        AddInInstallError, AddInInstallerOpenResult, arcgis_pro_process_state,
        open_packaged_addin_with, uninstall_guidance,
    },
    app_state::{
        AccountSnapshot, ActiveViewSnapshot, AppServerClient, AppServerFuture, BridgeSnapshot,
        BridgeStatus, ContextExtentSnapshot, DesktopSnapshot, DesktopState, HealthLease,
        LayerSnapshot, ManagedRuntime, McpDiscoveryLease, RuntimeOwnership, RuntimeProcess,
        ServerReply, retain_stale_snapshot,
    },
    arcgis_install::{
        ArcGisInstallSnapshot, choose_arcgis_executable as validate_chosen_arcgis_executable,
        discover_arcgis as discover_arcgis_install, launch_arcgis as launch_arcgis_process,
        validate_installation,
    },
    arcgis_tool_client::{ArcGisToolClient, BoxFuture, McpTool, McpToolResult, ToolClientError},
    codex::{
        CodexDiscoveryError, CodexEvent, CodexInstallation, CodexRuntime, CodexStartFailure,
        CodexStartOptions, CodexVersionConfidence, ProcessCodexVersionProbe, codex_candidates,
        discover_codex_with,
    },
    credential_store::{
        clear_deepseek as clear_deepseek_credential,
        configure_deepseek as configure_deepseek_credential, deepseek_credential_is_configured,
    },
    mcp_status::{
        CURRENT_STATUS_METHOD, LEGACY_STATUS_METHOD, arcgis_inventory_is_valid,
        mcp_status_list_params, parse_arcgis_status_notification,
    },
    paths::codex_home,
    providers::{ProviderEvent, ProviderKind},
};

#[cfg(debug_assertions)]
use crate::paths::resolve_mcp_command;

#[cfg(not(debug_assertions))]
use crate::paths::resolve_release_mcp_command;

impl AppServerClient for CodexRuntime {
    fn request<'a>(&'a self, method: &'a str, params: Value) -> AppServerFuture<'a, Value> {
        Box::pin(async move {
            CodexRuntime::request(self, method, params)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn respond<'a>(&'a self, id: Value, reply: ServerReply) -> AppServerFuture<'a, ()> {
        Box::pin(async move {
            let response = match reply {
                ServerReply::Result(result) => json!({"id": id, "result": result}),
                ServerReply::Error { code, message } => {
                    json!({"id": id, "error": {"code": code, "message": message}})
                }
            };
            self.send_server_response(response)
                .await
                .map_err(|error| error.to_string())
        })
    }
}

impl RuntimeProcess for CodexRuntime {
    fn next_event<'a>(&'a self) -> BoxFuture<'a, Option<CodexEvent>> {
        Box::pin(CodexRuntime::next_event(self))
    }

    fn persistent_event_waiter_count(&self) -> usize {
        CodexRuntime::persistent_event_waiter_count(self)
    }

    fn shutdown<'a>(&'a self) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            CodexRuntime::shutdown(self)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn ensure_terminated<'a>(&'a self) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            CodexRuntime::ensure_terminated(self)
                .await
                .map_err(|error| error.to_string())
        })
    }
}

struct CodexArcGisToolClient {
    client: Arc<dyn AppServerClient>,
    thread_id: String,
}

impl ArcGisToolClient for CodexArcGisToolClient {
    fn list_tools<'a>(&'a self) -> BoxFuture<'a, Result<Vec<McpTool>, ToolClientError>> {
        Box::pin(async move {
            let response = self
                .client
                .request(
                    "mcpServerStatus/list",
                    mcp_status_list_params(&self.thread_id),
                )
                .await
                .map_err(|_| ToolClientError::Unavailable)?;
            let tools = response
                .get("data")
                .and_then(Value::as_array)
                .and_then(|servers| {
                    servers
                        .iter()
                        .find(|server| server.get("name").and_then(Value::as_str) == Some("arcgis"))
                })
                .and_then(|server| server.get("tools").and_then(Value::as_object))
                .ok_or(ToolClientError::Protocol)?;
            Ok(tools
                .iter()
                .map(|(name, definition)| McpTool {
                    name: name.clone(),
                    description: definition
                        .get("description")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    input_schema: definition
                        .get("inputSchema")
                        .cloned()
                        .unwrap_or_else(|| json!({})),
                })
                .collect())
        })
    }

    fn call_tool<'a>(
        &'a self,
        name: &'a str,
        arguments: Value,
    ) -> BoxFuture<'a, Result<McpToolResult, ToolClientError>> {
        Box::pin(async move {
            let response = self
                .client
                .request(
                    "mcpServer/tool/call",
                    json!({
                        "threadId": self.thread_id,
                        "server": "arcgis",
                        "tool": name,
                        "arguments": arguments,
                    }),
                )
                .await
                .map_err(|_| ToolClientError::Unavailable)?;
            Ok(McpToolResult {
                content: response.get("content").cloned().unwrap_or(Value::Null),
                structured_content: response
                    .get("structuredContent")
                    .cloned()
                    .unwrap_or(Value::Null),
                is_error: response
                    .get("isError")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
    }
}

const MAX_TURN_BYTES: usize = 20_000;
const MAX_NAME_CHARS: usize = 256;
const MAX_URI_CHARS: usize = 2_000;
const MAX_PROJECT_ITEMS: usize = 100;
const MAX_LAYERS: usize = 200;
const MAX_LAYER_DEPTH: u64 = 32;
const MAX_MCP_TEXT_BYTES: usize = 512_000;
const MAX_STRUCTURED_JSON_DEPTH: usize = 64;
const MAX_STRUCTURED_JSON_NODES: usize = 20_000;
const MAX_TOOL_NAME_CHARS: usize = 128;
const MAX_TOOL_SUMMARY_CHARS: usize = 200;
const MAX_TOOL_DURATION_MS: u64 = 86_400_000;
const MCP_CALL_TIMEOUT: Duration = Duration::from_secs(5);
const MCP_STATUS_TIMEOUT: Duration = Duration::from_secs(10);
const EVENT_CONSUMER_START_TIMEOUT: Duration = Duration::from_secs(10);
const ACCOUNT_READ_TIMEOUT: Duration = Duration::from_secs(15);
const DEVELOPER_INSTRUCTIONS: &str = "You operate ArcGIS Pro only through tools from MCP server arcgis. Never use shell, command execution, file changes, arbitrary scripts, or unregistered geoprocessing. Treat MCP elicitation as mandatory user approval and accurately report structured tool results.";
const OFFICIAL_AUTH_HOSTS: [&str; 3] = ["auth.openai.com", "chatgpt.com", "openai.com"];

pub fn validate_auth_url(value: &str) -> Result<(), String> {
    let parsed = tauri::Url::parse(value).map_err(|_| "invalid login URL".to_owned())?;
    let authority = value
        .strip_prefix("https://")
        .and_then(|remainder| remainder.split(['/', '?', '#']).next())
        .ok_or_else(|| "login URL must use HTTPS".to_owned())?;
    let normalized_authority = authority.to_ascii_lowercase();
    let host = parsed
        .host_str()
        .ok_or_else(|| "login URL must include a host".to_owned())?;
    if parsed.scheme() != "https"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
        || authority != normalized_authority
        || !OFFICIAL_AUTH_HOSTS.contains(&normalized_authority.as_str())
        || !OFFICIAL_AUTH_HOSTS.contains(&host)
    {
        return Err("login URL is not an approved ChatGPT host".to_owned());
    }
    Ok(())
}

pub fn login_start_params() -> Value {
    json!({
        "type": "chatgpt",
        "codexStreamlinedLogin": true,
        "useHostedLoginSuccessPage": true,
        "appBrand": "codex"
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginStartResult {
    pub login_id: String,
    pub auth_url: String,
}

impl LoginStartResult {
    pub fn from_response(response: Value) -> Result<Self, String> {
        if response.get("type").and_then(Value::as_str) != Some("chatgpt") {
            return Err("App Server returned a non-ChatGPT login flow".to_owned());
        }
        let login_id = response
            .get("loginId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "App Server omitted the login ID".to_owned())?;
        let auth_url = response
            .get("authUrl")
            .and_then(Value::as_str)
            .ok_or_else(|| "App Server omitted the login URL".to_owned())?;
        validate_auth_url(auth_url)?;
        Ok(Self {
            login_id: login_id.to_owned(),
            auth_url: auth_url.to_owned(),
        })
    }
}

pub fn parse_account_snapshot(response: &Value) -> AccountSnapshot {
    let Some(account) = response.get("account").filter(|value| !value.is_null()) else {
        return AccountSnapshot::SignedOut;
    };
    match account.get("type").and_then(Value::as_str) {
        Some("chatgpt") => AccountSnapshot::SignedIn {
            email: account
                .get("email")
                .and_then(Value::as_str)
                .map(str::to_owned),
            plan_type: account
                .get("planType")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned(),
        },
        Some("apiKey") | Some(_) => AccountSnapshot::UnsupportedAuth,
        None => AccountSnapshot::SignedOut,
    }
}

fn parse_startup_account_snapshot(response: &Value) -> Result<AccountSnapshot, ()> {
    let account = response.get("account").ok_or(())?;
    if account.is_null() {
        return Ok(AccountSnapshot::SignedOut);
    }
    if account.get("type").and_then(Value::as_str) != Some("chatgpt") {
        return Err(());
    }
    if account
        .get("email")
        .is_some_and(|email| !email.is_null() && !email.is_string())
        || account
            .get("planType")
            .is_some_and(|plan_type| !plan_type.is_string())
    {
        return Err(());
    }
    Ok(parse_account_snapshot(response))
}

fn private_workspace_path(local_app_data: &Path) -> PathBuf {
    local_app_data.join("ArcGISProAgent").join("workspace")
}

pub fn thread_start_params(local_app_data: &Path) -> Value {
    let cwd = private_workspace_path(local_app_data)
        .to_string_lossy()
        .into_owned();
    json!({
        "cwd": cwd,
        "sandbox": "read-only",
        "approvalPolicy": "never",
        "approvalsReviewer": "user",
        "serviceName": "ArcGIS Pro Agent",
        "developerInstructions": DEVELOPER_INSTRUCTIONS
    })
}

pub fn turn_start_params(thread_id: &str, message: &str) -> Result<Value, String> {
    if message.trim().is_empty() {
        return Err("消息不能为空".to_owned());
    }
    if message.len() > MAX_TURN_BYTES {
        return Err("消息不能超过 20,000 个 UTF-8 字节".to_owned());
    }
    Ok(json!({
        "threadId": thread_id,
        "input": [{"type": "text", "text": message, "text_elements": []}]
    }))
}

pub fn turn_interrupt_params(thread_id: &str, turn_id: &str) -> Value {
    json!({"threadId": thread_id, "turnId": turn_id})
}

pub fn health_call_params(thread_id: &str) -> Value {
    json!({
        "threadId": thread_id,
        "server": "arcgis",
        "tool": "arcgis_connection_status",
        "arguments": {}
    })
}

pub fn context_call_params(thread_id: &str) -> Value {
    json!({
        "threadId": thread_id,
        "server": "arcgis",
        "tool": "arcgis_describe_context",
        "arguments": {}
    })
}

pub fn list_layers_call_params(thread_id: &str) -> Value {
    json!({
        "threadId": thread_id,
        "server": "arcgis",
        "tool": "arcgis_list_layers",
        "arguments": {"includeNested": true}
    })
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerRequestDisposition {
    ForwardElicitation { id: Value, request: Value },
    RejectUnsupported { id: Value },
}

pub fn classify_server_request(id: Value, method: &str, params: Value) -> ServerRequestDisposition {
    let server = params
        .get("serverName")
        .or_else(|| params.get("server"))
        .and_then(Value::as_str);
    if method == "mcpServer/elicitation/request" && server == Some("arcgis") {
        ServerRequestDisposition::ForwardElicitation {
            id,
            request: params,
        }
    } else {
        ServerRequestDisposition::RejectUnsupported { id }
    }
}

pub fn unsupported_server_response(id: Value) -> Value {
    json!({
        "id": id,
        "error": {"code": -32601, "message": "Unsupported server request"}
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationSafetyEvent {
    pub request_id: Value,
    pub server_name: String,
    pub thread_id: String,
    pub message: String,
    pub mode: ElicitationMode,
    pub outcome: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ElicitationMode {
    Form,
    Url,
    Unknown,
}

pub async fn handle_server_request_with<C: AppServerClient + ?Sized>(
    client: &C,
    id: Value,
    method: &str,
    params: Value,
) -> Result<Option<ElicitationSafetyEvent>, String> {
    let is_arcgis_elicitation = method == "mcpServer/elicitation/request"
        && params.get("serverName").and_then(Value::as_str) == Some("arcgis");
    if !is_arcgis_elicitation {
        client
            .respond(
                id,
                ServerReply::Error {
                    code: -32601,
                    message: "Unsupported server request".to_owned(),
                },
            )
            .await?;
        return Ok(None);
    }

    client
        .respond(
            id.clone(),
            ServerReply::Result(json!({"action": "decline"})),
        )
        .await?;
    let mode = match params.get("mode").and_then(Value::as_str) {
        Some("form") | Some("openai-form") => ElicitationMode::Form,
        Some("url") => ElicitationMode::Url,
        _ => ElicitationMode::Unknown,
    };
    Ok(Some(ElicitationSafetyEvent {
        request_id: id,
        server_name: "arcgis".to_owned(),
        thread_id: params
            .get("threadId")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned(),
        message: params
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("ArcGIS requested confirmation")
            .to_owned(),
        mode,
        outcome: "declined".to_owned(),
    }))
}

pub fn parse_health_response(response: &Value, updated_at: &str) -> Result<BridgeSnapshot, String> {
    let health = mcp_payload(response, "health")?;
    let health = health.as_ref();
    let connected = health
        .get("connected")
        .and_then(Value::as_bool)
        .ok_or_else(|| "ArcGIS health response omitted connected".to_owned())?;
    Ok(BridgeSnapshot {
        status: if connected {
            BridgeStatus::Connected
        } else {
            BridgeStatus::Disconnected
        },
        context_is_live: false,
        protocol_version: optional_bounded_string(&health, "protocolVersion", MAX_NAME_CHARS)?,
        add_in_version: optional_bounded_string(&health, "addInVersion", MAX_NAME_CHARS)?,
        arc_gis_pro_version: optional_bounded_string(&health, "arcGisProVersion", MAX_NAME_CHARS)?,
        project_name: optional_bounded_string(&health, "projectName", MAX_NAME_CHARS)?,
        project_has_unsaved_changes: None,
        active_map_name: optional_bounded_string(&health, "activeMapName", MAX_NAME_CHARS)?,
        active_view: None,
        layers: Vec::new(),
        last_updated: Some(updated_at.to_owned()),
        error: None,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_has_unsaved_changes: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_view: Option<ActiveViewSnapshot>,
}

pub fn parse_context_response(response: &Value) -> Result<ContextSnapshot, String> {
    let context = mcp_payload(response, "context")?;
    let context = context.as_ref();
    let project = match context.get("project") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_object()
                .ok_or_else(|| "ArcGIS context project was malformed".to_owned())?,
        ),
    };
    let (project_name, project_has_unsaved_changes) = match project {
        None => (None, None),
        Some(project) => {
            let name =
                required_bounded_string(project.get("name"), "project name", MAX_NAME_CHARS)?;
            let dirty = project
                .get("hasUnsavedChanges")
                .and_then(Value::as_bool)
                .ok_or_else(|| "ArcGIS context omitted project dirty state".to_owned())?;
            let items = project
                .get("items")
                .and_then(Value::as_array)
                .ok_or_else(|| "ArcGIS context omitted project items".to_owned())?;
            if items.len() > MAX_PROJECT_ITEMS {
                return Err("ArcGIS context project item limit exceeded".to_owned());
            }
            for item in items {
                required_bounded_string(item.get("uri"), "project item URI", MAX_URI_CHARS)?;
                required_bounded_string(item.get("name"), "project item name", MAX_NAME_CHARS)?;
                parse_project_kind(item.get("kind"), "project item kind")?;
                item.get("isActive")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| "ArcGIS project item omitted active state".to_owned())?;
            }
            (Some(name), Some(dirty))
        }
    };

    let active_view = match context.get("activeView") {
        None | Some(Value::Null) => None,
        Some(value) => {
            if project.is_none() {
                return Err("ArcGIS context had a view without a project".to_owned());
            }
            Some(parse_active_view(value)?)
        }
    };
    Ok(ContextSnapshot {
        project_name,
        project_has_unsaved_changes,
        active_view,
    })
}

pub fn parse_layers_response(response: &Value) -> Result<Vec<LayerSnapshot>, String> {
    let result = mcp_payload(response, "layers")?;
    let result = result.as_ref();
    let layers = result
        .get("layers")
        .and_then(Value::as_array)
        .ok_or_else(|| "ArcGIS layer response omitted layers".to_owned())?;
    if layers.len() > MAX_LAYERS {
        return Err("ArcGIS layer limit exceeded".to_owned());
    }
    layers
        .iter()
        .map(|layer| {
            let depth = layer
                .get("depth")
                .and_then(Value::as_u64)
                .filter(|depth| *depth <= MAX_LAYER_DEPTH)
                .ok_or_else(|| "ArcGIS layer depth was malformed".to_owned())?;
            Ok(LayerSnapshot {
                uri: required_bounded_string(layer.get("uri"), "layer URI", MAX_URI_CHARS)?,
                name: required_bounded_string(layer.get("name"), "layer name", MAX_NAME_CHARS)?,
                long_name: required_bounded_string(
                    layer.get("longName"),
                    "layer long name",
                    MAX_URI_CHARS,
                )?,
                layer_type: required_bounded_string(
                    layer.get("layerType"),
                    "layer type",
                    MAX_NAME_CHARS,
                )?,
                parent_uri: optional_bounded_value(
                    layer.get("parentUri"),
                    "layer parent URI",
                    MAX_URI_CHARS,
                )?,
                depth: depth as u16,
                visible: layer
                    .get("visible")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| "ArcGIS layer omitted visibility".to_owned())?,
                is_feature_layer: layer
                    .get("isFeatureLayer")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| "ArcGIS layer omitted feature-layer state".to_owned())?,
            })
        })
        .collect()
}

fn parse_active_view(value: &Value) -> Result<ActiveViewSnapshot, String> {
    Ok(ActiveViewSnapshot {
        uri: required_bounded_string(value.get("uri"), "active view URI", MAX_URI_CHARS)?,
        name: required_bounded_string(value.get("name"), "active view name", MAX_NAME_CHARS)?,
        kind: parse_project_kind(value.get("kind"), "active view kind")?,
        extent: match value.get("extent") {
            None | Some(Value::Null) => None,
            Some(extent) => Some(parse_extent(extent)?),
        },
    })
}

fn parse_extent(value: &Value) -> Result<ContextExtentSnapshot, String> {
    fn coordinate(value: &Value, field: &str) -> Result<f64, String> {
        value
            .get(field)
            .and_then(Value::as_f64)
            .filter(|coordinate| coordinate.is_finite())
            .ok_or_else(|| "ArcGIS extent coordinate was malformed".to_owned())
    }

    let wkid = match value.get("wkid") {
        None | Some(Value::Null) => None,
        Some(wkid) => Some(
            wkid.as_i64()
                .and_then(|wkid| i32::try_from(wkid).ok())
                .ok_or_else(|| "ArcGIS extent WKID was malformed".to_owned())?,
        ),
    };
    Ok(ContextExtentSnapshot {
        x_min: coordinate(value, "xMin")?,
        y_min: coordinate(value, "yMin")?,
        x_max: coordinate(value, "xMax")?,
        y_max: coordinate(value, "yMax")?,
        wkid,
    })
}

fn parse_project_kind(value: Option<&Value>, field: &str) -> Result<String, String> {
    match value {
        Some(Value::String(kind)) => match kind.to_ascii_lowercase().as_str() {
            "map" => Ok("map".to_owned()),
            "scene" => Ok("scene".to_owned()),
            "layout" => Ok("layout".to_owned()),
            _ => Err(format!("ArcGIS {field} was malformed")),
        },
        Some(Value::Number(kind)) => match kind.as_u64() {
            Some(0) => Ok("map".to_owned()),
            Some(1) => Ok("scene".to_owned()),
            Some(2) => Ok("layout".to_owned()),
            _ => Err(format!("ArcGIS {field} was malformed")),
        },
        _ => Err(format!("ArcGIS {field} was malformed")),
    }
}

struct LimitedJsonWriter {
    written: usize,
    limit: usize,
}

impl LimitedJsonWriter {
    fn new(limit: usize) -> Self {
        Self { written: 0, limit }
    }
}

impl Write for LimitedJsonWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let remaining = self.limit.saturating_sub(self.written);
        if buffer.len() > remaining {
            return Err(io::Error::other("structured JSON limit exceeded"));
        }
        self.written += buffer.len();
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn structured_json_is_within_limit(value: &Value) -> bool {
    fn shape_is_within_limit(value: &Value, depth: usize, nodes: &mut usize) -> bool {
        if depth > MAX_STRUCTURED_JSON_DEPTH {
            return false;
        }
        *nodes = nodes.saturating_add(1);
        if *nodes > MAX_STRUCTURED_JSON_NODES {
            return false;
        }
        match value {
            Value::Array(values) => values
                .iter()
                .all(|value| shape_is_within_limit(value, depth + 1, nodes)),
            Value::Object(values) => values
                .values()
                .all(|value| shape_is_within_limit(value, depth + 1, nodes)),
            _ => true,
        }
    }

    let mut nodes = 0;
    if !shape_is_within_limit(value, 0, &mut nodes) {
        return false;
    }
    let mut writer = LimitedJsonWriter::new(MAX_MCP_TEXT_BYTES);
    serde_json::to_writer(&mut writer, value).is_ok()
}

fn mcp_payload<'a>(response: &'a Value, label: &str) -> Result<Cow<'a, Value>, String> {
    if response.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err(format!("ArcGIS {label} tool returned an error"));
    }
    if let Some(structured) = response
        .get("structuredContent")
        .filter(|value| !value.is_null())
    {
        // The Codex App Server client has already allocated upstream JSON as a Value before this
        // desktop safety boundary. This pass avoids another full allocation and stops serializing
        // immediately when the local byte budget is exceeded.
        if !structured_json_is_within_limit(structured) {
            return Err(format!(
                "ArcGIS {label} structured response exceeded the safe limit"
            ));
        }
        return Ok(Cow::Borrowed(structured));
    }
    let text = response
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("type").and_then(Value::as_str) == Some("text"))
        })
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .ok_or_else(|| format!("ArcGIS {label} response omitted structured content"))?;
    if text.len() > MAX_MCP_TEXT_BYTES {
        return Err(format!(
            "ArcGIS {label} text response exceeded the safe limit"
        ));
    }
    serde_json::from_str(text)
        .map(Cow::Owned)
        .map_err(|_| format!("ArcGIS {label} text response was malformed"))
}

fn required_bounded_string(
    value: Option<&Value>,
    field: &str,
    max_chars: usize,
) -> Result<String, String> {
    let value = value
        .and_then(Value::as_str)
        .ok_or_else(|| format!("ArcGIS {field} was malformed"))?;
    if value.is_empty() || value.chars().count() > max_chars {
        return Err(format!("ArcGIS {field} exceeded the safe limit"));
    }
    Ok(value.to_owned())
}

fn optional_bounded_string(
    value: &Value,
    field: &str,
    max_chars: usize,
) -> Result<Option<String>, String> {
    optional_bounded_value(value.get(field), field, max_chars)
}

fn optional_bounded_value(
    value: Option<&Value>,
    field: &str,
    max_chars: usize,
) -> Result<Option<String>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => required_bounded_string(Some(value), field, max_chars).map(Some),
    }
}

pub fn tool_completion_event(params: &Value) -> Option<Value> {
    let item = params.get("item")?;
    if item.get("type").and_then(Value::as_str) != Some("mcpToolCall")
        || item.get("server").and_then(Value::as_str) != Some("arcgis")
    {
        return None;
    }
    let tool = safe_tool_name(item.get("tool").and_then(Value::as_str));
    let risk = tool_risk(&tool);
    let structured = item
        .pointer("/result/structuredContent")
        .filter(|value| !value.is_null() && structured_json_is_within_limit(value))
        .or_else(|| {
            item.get("structuredContent")
                .filter(|value| !value.is_null() && structured_json_is_within_limit(value))
        });
    let outcome = normalized_tool_outcome(item, structured);
    let duration = item
        .get("durationMs")
        .and_then(Value::as_u64)
        .filter(|duration| *duration <= MAX_TOOL_DURATION_MS);
    let summary = structured.and_then(safe_tool_summary);
    let error_code = structured.and_then(public_error_code);

    let mut safe_item = serde_json::Map::new();
    safe_item.insert("type".to_owned(), json!("mcpToolCall"));
    safe_item.insert("server".to_owned(), json!("arcgis"));
    safe_item.insert("tool".to_owned(), json!(tool));
    safe_item.insert("risk".to_owned(), json!(risk));
    safe_item.insert("outcome".to_owned(), json!(outcome));
    if let Some(duration) = duration {
        safe_item.insert("durationMs".to_owned(), json!(duration));
    }
    if let Some(summary) = summary {
        safe_item.insert("summary".to_owned(), json!(summary));
    }
    if let Some(error_code) = error_code {
        safe_item.insert("errorCode".to_owned(), json!(error_code));
    }
    Some(json!({"type": "item/completed", "item": Value::Object(safe_item)}))
}

fn safe_tool_name(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "unknown".to_owned();
    };
    if value.is_empty()
        || value.chars().count() > MAX_TOOL_NAME_CHARS
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return "unknown".to_owned();
    }
    value.to_owned()
}

fn tool_risk(tool: &str) -> &'static str {
    match tool {
        "arcgis_connection_status"
        | "arcgis_capabilities"
        | "arcgis_count_features"
        | "arcgis_describe_context"
        | "arcgis_describe_layer"
        | "arcgis_get_selection"
        | "arcgis_list_fields"
        | "arcgis_list_layers"
        | "arcgis_query_features"
        | "arcgis_query_spatial" => "R0",
        "arcgis_activate_view"
        | "arcgis_clear_selection"
        | "arcgis_flash_features"
        | "arcgis_select_by_attribute"
        | "arcgis_select_by_location"
        | "arcgis_zoom_to_extent"
        | "arcgis_zoom_to_layer" => "R1",
        _ => "unknown",
    }
}

fn normalized_tool_outcome(item: &Value, structured: Option<&Value>) -> &'static str {
    let is_error = item.get("isError").and_then(Value::as_bool) == Some(true)
        || item.pointer("/result/isError").and_then(Value::as_bool) == Some(true)
        || structured.is_some_and(|structured| {
            structured
                .get("error")
                .is_some_and(|error| !error.is_null())
                || public_error_code(structured).is_some()
        });
    if is_error {
        return "failed";
    }
    match item.get("status").and_then(Value::as_str) {
        Some("completed" | "succeeded" | "success") => "succeeded",
        Some("failed" | "error") => "failed",
        _ => "unknown",
    }
}

fn safe_tool_summary(structured: &Value) -> Option<String> {
    const SAFE_FIELDS: [&str; 8] = [
        "count",
        "selectedCount",
        "layersCleared",
        "featuresCleared",
        "activated",
        "completed",
        "flashedCount",
        "hasMore",
    ];
    let mut parts = Vec::new();
    for field in SAFE_FIELDS {
        let Some(value) = structured.get(field) else {
            continue;
        };
        let rendered = match value {
            Value::Bool(value) => value.to_string(),
            Value::Number(value) if value.as_u64().is_some() => value.to_string(),
            _ => continue,
        };
        let part = format!("{field}={rendered}");
        let next_len = parts.iter().map(String::len).sum::<usize>()
            + parts.len().saturating_mul(2)
            + part.len();
        if next_len > MAX_TOOL_SUMMARY_CHARS {
            break;
        }
        parts.push(part);
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

fn public_error_code(structured: &Value) -> Option<String> {
    let value = structured
        .pointer("/error/code")
        .or_else(|| structured.get("errorCode"))?
        .as_str()?;
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return None;
    }
    Some(value.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationStartResult {
    pub thread_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnStartResult {
    pub turn_id: String,
}

pub async fn chatgpt_login_start_with(state: &DesktopState) -> Result<LoginStartResult, String> {
    let client = state
        .app_server_client()
        .await
        .ok_or_else(|| "Codex App Server 尚未就绪".to_owned())?;
    let response = client
        .request("account/login/start", login_start_params())
        .await
        .map_err(|_| "无法开始 ChatGPT 登录".to_owned())?;
    let login = LoginStartResult::from_response(response)?;
    state
        .apply_account(AccountSnapshot::LoginPending {
            login_id: login.login_id.clone(),
        })
        .await;
    Ok(login)
}

pub async fn chatgpt_login_cancel_with(state: &DesktopState) -> DesktopSnapshot {
    state.apply_account(AccountSnapshot::SignedOut).await
}

pub async fn chatgpt_logout_with(state: &DesktopState) -> Result<DesktopSnapshot, String> {
    let client = state
        .app_server_client()
        .await
        .ok_or_else(|| "Codex App Server 尚未就绪".to_owned())?;
    client
        .request("account/logout", json!({}))
        .await
        .map_err(|_| "退出 ChatGPT 登录失败".to_owned())?;
    Ok(state.apply_account(AccountSnapshot::SignedOut).await)
}

pub async fn refresh_account_with(state: &DesktopState) -> Result<DesktopSnapshot, String> {
    let client = state
        .app_server_client()
        .await
        .ok_or_else(|| "Codex App Server 尚未就绪".to_owned())?;
    let response = client
        .request("account/read", json!({"refreshToken": false}))
        .await
        .map_err(|_| "无法读取 ChatGPT 账号".to_owned())?;
    Ok(state.apply_account(parse_account_snapshot(&response)).await)
}

pub async fn turn_start_with(
    state: &DesktopState,
    message: String,
) -> Result<TurnStartResult, String> {
    let (lease, client) = state.begin_turn().await?;
    let params = match turn_start_params(lease.thread_id(), &message) {
        Ok(params) => params,
        Err(error) => {
            state.abort_turn(&lease).await;
            return Err(error);
        }
    };
    let response = match client.request("turn/start", params).await {
        Ok(response) => response,
        Err(_) => {
            state.abort_turn(&lease).await;
            return Err("Unable to send message".to_owned());
        }
    };
    let turn_id = match response
        .pointer("/turn/id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        Some(turn_id) => turn_id.to_owned(),
        None => {
            state.abort_turn(&lease).await;
            return Err("App Server omitted the turn ID".to_owned());
        }
    };
    state.commit_turn(&lease, turn_id.clone()).await?;
    Ok(TurnStartResult { turn_id })
}

pub async fn handle_turn_completed(state: &DesktopState, params: Value) -> bool {
    handle_turn_completed_event(state, params).await.is_some()
}

pub async fn handle_turn_completed_event(
    state: &DesktopState,
    params: Value,
) -> Option<ProviderEvent> {
    let thread_id = params.get("threadId").and_then(Value::as_str);
    let turn_id = params
        .get("turnId")
        .and_then(Value::as_str)
        .or_else(|| params.pointer("/turn/id").and_then(Value::as_str));
    match (thread_id, turn_id) {
        (Some(thread_id), Some(turn_id))
            if state.complete_turn_if_matching(thread_id, turn_id).await =>
        {
            Some(ProviderEvent::TurnCompleted {
                turn_id: turn_id.to_owned(),
            })
        }
        _ => None,
    }
}

pub async fn health_refresh_with(state: &DesktopState, updated_at: &str) -> Result<bool, String> {
    let Some((lease, snapshot)) = prepare_health_refresh(state, updated_at).await? else {
        return Ok(false);
    };
    Ok(state.commit_health(&lease, snapshot).await)
}

async fn prepare_health_refresh(
    state: &DesktopState,
    updated_at: &str,
) -> Result<Option<(HealthLease, BridgeSnapshot)>, String> {
    let Some((lease, client)) = state.health_lease().await else {
        return Ok(None);
    };
    let current = state.snapshot().await.arcgis;
    let tool_client = CodexArcGisToolClient {
        client,
        thread_id: lease.thread_id().to_owned(),
    };
    let health_response =
        request_mcp_with_timeout(&tool_client, "arcgis_connection_status", json!({})).await;
    let mut health = match health_response {
        Ok(response) => match parse_health_response(&response, updated_at) {
            Ok(health) => health,
            Err(error) => {
                return Ok(Some((lease, retain_stale_snapshot(&current, &error))));
            }
        },
        Err(()) => {
            return Ok(Some((
                lease,
                retain_stale_snapshot(&current, "connection request failed"),
            )));
        }
    };
    if !state.health_lease_is_current(&lease).await {
        return Ok(None);
    }
    if health.status != BridgeStatus::Connected || health.project_name.is_none() {
        clear_project_context(&mut health);
        health.context_is_live = true;
        return Ok(Some((lease, health)));
    }

    let context_response =
        request_mcp_with_timeout(&tool_client, "arcgis_describe_context", json!({})).await;
    let context = match context_response
        .and_then(|response| parse_context_response(&response).map_err(|_| ()))
    {
        Ok(context) => context,
        Err(()) => {
            return Ok(Some((lease, retain_context_with_health(&current, &health))));
        }
    };
    if !state.health_lease_is_current(&lease).await {
        return Ok(None);
    }

    let has_active_map = context
        .active_view
        .as_ref()
        .is_some_and(|view| matches!(view.kind.as_str(), "map" | "scene"));
    let layers = if has_active_map {
        let layer_response = request_mcp_with_timeout(
            &tool_client,
            "arcgis_list_layers",
            json!({"includeNested": true}),
        )
        .await;
        match layer_response.and_then(|response| parse_layers_response(&response).map_err(|_| ())) {
            Ok(layers) => layers,
            Err(()) => {
                return Ok(Some((lease, retain_context_with_health(&current, &health))));
            }
        }
    } else {
        Vec::new()
    };
    if !state.health_lease_is_current(&lease).await {
        return Ok(None);
    }

    health.project_name = context.project_name;
    health.project_has_unsaved_changes = context.project_has_unsaved_changes;
    health.active_map_name = context
        .active_view
        .as_ref()
        .filter(|view| matches!(view.kind.as_str(), "map" | "scene"))
        .map(|view| view.name.clone());
    health.active_view = context.active_view;
    health.layers = layers;
    health.context_is_live = true;
    health.error = None;
    Ok(Some((lease, health)))
}

async fn request_mcp_with_timeout(
    client: &dyn ArcGisToolClient,
    name: &str,
    arguments: Value,
) -> Result<Value, ()> {
    tokio::time::timeout(MCP_CALL_TIMEOUT, client.call_tool(name, arguments))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())
        .map(|result| {
            json!({
                "content": result.content,
                "structuredContent": result.structured_content,
                "isError": result.is_error,
            })
        })
}

fn clear_project_context(snapshot: &mut BridgeSnapshot) {
    snapshot.project_name = None;
    snapshot.project_has_unsaved_changes = None;
    snapshot.active_map_name = None;
    snapshot.active_view = None;
    snapshot.layers.clear();
    snapshot.error = None;
}

fn retain_context_with_health(
    retained: &BridgeSnapshot,
    health: &BridgeSnapshot,
) -> BridgeSnapshot {
    BridgeSnapshot {
        status: health.status.clone(),
        context_is_live: false,
        protocol_version: health.protocol_version.clone(),
        add_in_version: health.add_in_version.clone(),
        arc_gis_pro_version: health.arc_gis_pro_version.clone(),
        project_name: retained.project_name.clone(),
        project_has_unsaved_changes: retained.project_has_unsaved_changes,
        active_map_name: retained.active_map_name.clone(),
        active_view: retained.active_view.clone(),
        layers: retained.layers.clone(),
        last_updated: retained.last_updated.clone(),
        error: Some("ArcGIS 上下文刷新失败".to_owned()),
    }
}

pub async fn conversation_start_with(
    state: &DesktopState,
) -> Result<ConversationStartResult, String> {
    let (lease, client) = state.begin_conversation().await?;
    if std::fs::create_dir_all(private_workspace_path(state.local_app_data())).is_err() {
        state.abort_conversation(&lease).await;
        return Err("Unable to prepare ArcGIS conversation".to_owned());
    }
    let response = match client
        .request("thread/start", thread_start_params(state.local_app_data()))
        .await
    {
        Ok(response) => response,
        Err(_) => {
            state.abort_conversation(&lease).await;
            return Err("Unable to create ArcGIS conversation".to_owned());
        }
    };
    let thread_id = match response
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        Some(thread_id) => thread_id.to_owned(),
        None => {
            state.abort_conversation(&lease).await;
            return Err("App Server omitted the conversation ID".to_owned());
        }
    };
    let discovery = state.commit_conversation(&lease, thread_id.clone()).await?;
    let _ = refresh_mcp_status_with(state, client.as_ref(), &discovery).await;
    Ok(ConversationStartResult { thread_id })
}

pub async fn refresh_mcp_status_with(
    state: &DesktopState,
    client: &dyn AppServerClient,
    lease: &McpDiscoveryLease,
) -> bool {
    refresh_mcp_status_with_timeout(state, client, lease, MCP_STATUS_TIMEOUT).await
}

pub async fn refresh_mcp_status_with_timeout(
    state: &DesktopState,
    client: &dyn AppServerClient,
    lease: &McpDiscoveryLease,
    timeout: Duration,
) -> bool {
    let response = tokio::time::timeout(
        timeout,
        client.request(
            "mcpServerStatus/list",
            mcp_status_list_params(lease.thread_id()),
        ),
    )
    .await;
    let valid = matches!(response, Ok(Ok(ref value)) if arcgis_inventory_is_valid(value));
    state.apply_arcgis_inventory(lease, valid).await
}

pub async fn handle_mcp_status_notification_with(
    state: &DesktopState,
    runtime_epoch: u64,
    method: &str,
    params: Value,
) -> bool {
    let Some(update) = parse_arcgis_status_notification(method, &params) else {
        return false;
    };
    state.apply_arcgis_status(runtime_epoch, update).await
}

pub async fn run_after_event_consumer_start_with<Started, After>(
    started: Started,
    after_started: After,
) -> bool
where
    Started: Future<Output = bool>,
    After: Future<Output = ()>,
{
    if !started.await {
        return false;
    }
    after_started.await;
    true
}

pub async fn turn_interrupt_with(state: &DesktopState) -> Result<(), String> {
    let (lease, client) = state.begin_interrupt().await?;
    let result = client
        .request(
            "turn/interrupt",
            turn_interrupt_params(lease.thread_id(), lease.turn_id()),
        )
        .await;
    match result {
        Ok(_) => {
            state.finish_interrupt(&lease, true).await;
            Ok(())
        }
        Err(_) => {
            state.finish_interrupt(&lease, false).await;
            Err("Unable to interrupt the current turn".to_owned())
        }
    }
}

#[tauri::command]
pub async fn desktop_snapshot(state: State<'_, DesktopState>) -> Result<DesktopSnapshot, String> {
    Ok(state.snapshot().await)
}

pub async fn discover_arcgis_with(state: &DesktopState) -> ArcGisInstallSnapshot {
    let settings = match state.settings_store().load() {
        Ok(settings) => settings,
        Err(_) => {
            let install = ArcGisInstallSnapshot::Error {
                code: "settings_unavailable".to_owned(),
            };
            state
                .update_snapshot(|snapshot| snapshot.arcgis_install = install.clone())
                .await;
            return install;
        }
    };
    let install = match discover_arcgis_install(settings.arcgis_pro_root.as_deref()) {
        Ok(installation) => {
            if save_arcgis_pro_root_with(state, installation.root.clone())
                .await
                .is_err()
            {
                ArcGisInstallSnapshot::Error {
                    code: "settings_unavailable".to_owned(),
                }
            } else {
                ArcGisInstallSnapshot::Ready { installation }
            }
        }
        Err(_) => ArcGisInstallSnapshot::NotFound,
    };
    state
        .update_snapshot(|snapshot| snapshot.arcgis_install = install.clone())
        .await;
    install
}

#[tauri::command]
pub async fn discover_arcgis(
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<ArcGisInstallSnapshot, String> {
    let install = discover_arcgis_with(&state).await;
    emit_snapshot(&app, state.snapshot().await);
    Ok(install)
}

#[tauri::command]
pub async fn choose_arcgis_executable(
    app: AppHandle,
    executable: PathBuf,
    state: State<'_, DesktopState>,
) -> Result<ArcGisInstallSnapshot, String> {
    let installation =
        validate_chosen_arcgis_executable(&executable).map_err(|error| error.to_string())?;
    save_arcgis_pro_root_with(&state, installation.root.clone()).await?;
    let install = ArcGisInstallSnapshot::Ready { installation };
    state
        .update_snapshot(|snapshot| snapshot.arcgis_install = install.clone())
        .await;
    emit_snapshot(&app, state.snapshot().await);
    Ok(install)
}

pub async fn save_arcgis_pro_root_with(state: &DesktopState, root: PathBuf) -> Result<(), String> {
    let _mutation = state.lock_settings_mutation().await;
    state
        .settings_store()
        .save_arcgis_pro_root(root)
        .map_err(|_| "Unable to save ArcGIS Pro settings".to_owned())
}

#[tauri::command]
pub fn open_addin_installer(app: AppHandle) -> Result<AddInInstallerOpenResult, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|_| "Unable to locate application resources".to_owned())?;
    open_packaged_addin_with(&resource_dir, arcgis_pro_process_state(), |package| {
        tauri_plugin_opener::open_path(package, None::<&str>)
            .map_err(|_| AddInInstallError::Unavailable)
    })
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn launch_arcgis(state: State<'_, DesktopState>) -> Result<u32, String> {
    let snapshot = state.snapshot().await;
    let installation = match snapshot.arcgis_install {
        ArcGisInstallSnapshot::Ready { installation } => installation,
        _ => return Err("ArcGIS Pro 3.7 is not ready".to_owned()),
    };
    let mut validated =
        validate_installation(&installation.root).map_err(|error| error.to_string())?;
    validated.source = installation.source;
    launch_arcgis_process(&validated).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn addin_uninstall_guidance() -> &'static str {
    uninstall_guidance()
}

pub async fn provider_select_with(
    state: &DesktopState,
    provider: ProviderKind,
) -> Result<DesktopSnapshot, String> {
    let _mutation = state.lock_settings_mutation().await;
    let deepseek_configured = if provider == ProviderKind::DeepSeek {
        deepseek_credential_is_configured(state.secret_store())
            .await
            .map_err(|_| "Unable to read provider credentials".to_owned())?
    } else {
        false
    };
    let mut settings = state
        .settings_store()
        .load()
        .map_err(|_| "Unable to read provider settings".to_owned())?;
    settings.active_provider = provider;
    state
        .settings_store()
        .save(&settings)
        .map_err(|_| "Unable to save provider settings".to_owned())?;
    Ok(state.select_provider(provider, deepseek_configured).await)
}

pub async fn deepseek_configure_with(
    state: &DesktopState,
    secret: &str,
) -> Result<DesktopSnapshot, String> {
    let _mutation = state.lock_settings_mutation().await;
    let settings = state
        .settings_store()
        .load()
        .map_err(|_| "Unable to read provider settings".to_owned())?;
    configure_deepseek_credential(state.secret_store(), secret)
        .await
        .map_err(|_| "Unable to store DeepSeek credentials".to_owned())?;
    if settings.active_provider == ProviderKind::DeepSeek {
        Ok(state.select_provider(ProviderKind::DeepSeek, true).await)
    } else {
        Ok(state.snapshot().await)
    }
}

pub async fn deepseek_clear_with(state: &DesktopState) -> Result<DesktopSnapshot, String> {
    let _mutation = state.lock_settings_mutation().await;
    let settings = state
        .settings_store()
        .load()
        .map_err(|_| "Unable to read provider settings".to_owned())?;
    clear_deepseek_credential(state.secret_store())
        .await
        .map_err(|_| "Unable to clear DeepSeek credentials".to_owned())?;
    if settings.active_provider == ProviderKind::DeepSeek {
        Ok(state.select_provider(ProviderKind::DeepSeek, false).await)
    } else {
        Ok(state.snapshot().await)
    }
}

#[tauri::command]
pub async fn chatgpt_login_start(
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<LoginStartResult, String> {
    let login = chatgpt_login_start_with(&state).await?;
    emit_snapshot(&app, state.snapshot().await);
    Ok(login)
}

#[tauri::command]
pub async fn chatgpt_login_cancel(
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    let snapshot = chatgpt_login_cancel_with(&state).await;
    emit_snapshot(&app, snapshot);
    Ok(())
}

#[tauri::command]
pub async fn chatgpt_logout(app: AppHandle, state: State<'_, DesktopState>) -> Result<(), String> {
    let snapshot = chatgpt_logout_with(&state).await?;
    emit_snapshot(&app, snapshot);
    Ok(())
}

#[tauri::command]
pub async fn conversation_start(
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<ConversationStartResult, String> {
    let result = conversation_start_with(&state).await?;
    emit_snapshot(&app, state.snapshot().await);
    Ok(result)
}

#[tauri::command]
pub async fn turn_start(
    message: String,
    state: State<'_, DesktopState>,
) -> Result<TurnStartResult, String> {
    turn_start_with(&state, message).await
}

#[tauri::command]
pub async fn turn_interrupt(state: State<'_, DesktopState>) -> Result<(), String> {
    turn_interrupt_with(&state).await
}

pub(crate) trait RuntimeServices: Send + Sync {
    fn discover<'a>(
        &'a self,
        local_app_data: &'a Path,
    ) -> BoxFuture<'a, Result<CodexInstallation, CodexDiscoveryError>>;

    fn start<'a>(
        &'a self,
        installation: &'a CodexInstallation,
        local_app_data: &'a Path,
    ) -> BoxFuture<'a, Result<ManagedRuntime, RuntimeStartFailure>>;
}

pub(crate) struct RuntimeStartFailure {
    _error: String,
    runtime: Option<ManagedRuntime>,
}

impl RuntimeStartFailure {
    fn new(error: String, runtime: Option<ManagedRuntime>) -> Self {
        Self {
            _error: error,
            runtime,
        }
    }

    fn into_runtime(self) -> Option<ManagedRuntime> {
        self.runtime
    }
}

struct ProductionRuntimeServices;

pub(crate) trait RuntimeHost: Send + Sync {
    fn emit(&self, snapshot: DesktopSnapshot);
    fn spawn_event_loop(
        &self,
        runtime: ManagedRuntime,
        runtime_epoch: u64,
    ) -> tokio::task::JoinHandle<()>;
    fn spawn_health_poller(&self, runtime_epoch: u64);
}

struct TauriRuntimeHost<R: tauri::Runtime> {
    app: AppHandle<R>,
}

impl<R: tauri::Runtime + 'static> RuntimeHost for TauriRuntimeHost<R> {
    fn emit(&self, snapshot: DesktopSnapshot) {
        emit_snapshot(&self.app, snapshot);
    }

    fn spawn_event_loop(
        &self,
        runtime: ManagedRuntime,
        runtime_epoch: u64,
    ) -> tokio::task::JoinHandle<()> {
        let app = self.app.clone();
        tokio::spawn(async move {
            run_event_loop(app, runtime, runtime_epoch).await;
        })
    }

    fn spawn_health_poller(&self, runtime_epoch: u64) {
        let app = self.app.clone();
        tokio::spawn(async move {
            run_health_poller(app, runtime_epoch).await;
        });
    }
}

impl RuntimeServices for ProductionRuntimeServices {
    fn discover<'a>(
        &'a self,
        local_app_data: &'a Path,
    ) -> BoxFuture<'a, Result<CodexInstallation, CodexDiscoveryError>> {
        Box::pin(async move {
            let path = std::env::var_os("PATH");
            let app_data = std::env::var_os("APPDATA").map(PathBuf::from);
            if codex_candidates(path.as_deref(), app_data.as_deref()).is_empty() {
                return Err(CodexDiscoveryError::NotFound);
            }
            let probe = ProcessCodexVersionProbe::new(codex_home(local_app_data));
            match discover_codex_with(path.as_deref(), app_data.as_deref(), &probe).await {
                Err(CodexDiscoveryError::NotFound) => Err(CodexDiscoveryError::Invalid),
                result => result,
            }
        })
    }

    fn start<'a>(
        &'a self,
        installation: &'a CodexInstallation,
        local_app_data: &'a Path,
    ) -> BoxFuture<'a, Result<ManagedRuntime, RuntimeStartFailure>> {
        Box::pin(async move {
            let (mcp_command, mcp_args) =
                production_mcp_options().map_err(|error| RuntimeStartFailure::new(error, None))?;
            let options = CodexStartOptions {
                codex_command: installation.command.clone(),
                codex_home: codex_home(local_app_data),
                mcp_command,
                mcp_args,
                local_app_data: local_app_data.to_path_buf(),
            };
            match CodexRuntime::start(options).await {
                Ok(runtime) => {
                    let runtime = Arc::new(runtime);
                    Ok(ManagedRuntime::new(runtime.clone(), runtime))
                }
                Err(failure) => Err(runtime_start_failure(failure)),
            }
        })
    }
}

fn runtime_start_failure(failure: CodexStartFailure) -> RuntimeStartFailure {
    let (error, runtime) = failure.into_parts();
    let runtime = runtime.map(|runtime| {
        let runtime = Arc::new(runtime);
        ManagedRuntime::new(runtime.clone(), runtime)
    });
    RuntimeStartFailure::new(error.to_string(), runtime)
}

#[cfg(debug_assertions)]
fn production_mcp_options() -> Result<(PathBuf, Vec<OsString>), String> {
    let current_exe = std::env::current_exe().ok();
    Ok((
        resolve_mcp_command(
            std::env::var_os("ARCGIS_AGENT_MCP_COMMAND").as_deref(),
            current_exe.as_deref(),
            Path::is_file,
        ),
        environment_args("ARCGIS_AGENT_MCP_ARGS"),
    ))
}

#[cfg(not(debug_assertions))]
fn production_mcp_options() -> Result<(PathBuf, Vec<OsString>), String> {
    let current_exe = std::env::current_exe().map_err(|_| "codex_incompatible".to_owned())?;
    let command = resolve_release_mcp_command(Some(&current_exe))
        .map_err(|_| "codex_incompatible".to_owned())?;
    Ok((command, Vec::new()))
}

pub async fn initialize_runtime(app: AppHandle) {
    let state = app.state::<DesktopState>();
    discover_arcgis_with(&state).await;
    emit_snapshot(&app, state.snapshot().await);
    restart_codex_runtime(&app, &state).await;
}

#[tauri::command]
pub async fn rediscover_codex(
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String> {
    restart_codex_runtime(&app, &state).await;
    Ok(state.snapshot().await)
}

async fn restart_codex_runtime<R: tauri::Runtime>(app: &AppHandle<R>, state: &DesktopState)
where
    R: 'static,
{
    restart_codex_runtime_with_services(
        state,
        Arc::new(ProductionRuntimeServices),
        Arc::new(TauriRuntimeHost { app: app.clone() }),
    )
    .await;
}

pub(crate) async fn restart_codex_runtime_with_services(
    state: &DesktopState,
    services: Arc<dyn RuntimeServices>,
    host: Arc<dyn RuntimeHost>,
) {
    let observed_epoch = state.runtime_epoch().await;
    restart_codex_runtime_after_observation(state, services, host, observed_epoch).await;
}

pub(crate) async fn restart_codex_runtime_after_observation(
    state: &DesktopState,
    services: Arc<dyn RuntimeServices>,
    host: Arc<dyn RuntimeHost>,
    observed_epoch: u64,
) {
    let _restart = state.lock_runtime_restart().await;
    if state.restart_completed_since(observed_epoch).await {
        return;
    }
    if !process_quarantined_runtime(state).await {
        return;
    }

    let (runtime_epoch, old_runtime, starting) = state.begin_runtime_restart().await;
    host.emit(starting);
    if let Some(old_runtime) = old_runtime {
        if let Err(old_runtime) = stop_owned_runtime(old_runtime).await {
            state.restore_quarantined_runtime(old_runtime).await;
            publish_incompatible_if_current(host.as_ref(), state, runtime_epoch).await;
            return;
        }
    }

    let installation = match services.discover(state.local_app_data()).await {
        Ok(installation) => installation,
        Err(error) => {
            let code = match error {
                CodexDiscoveryError::NotFound => "codex_not_found",
                CodexDiscoveryError::Invalid => "codex_invalid",
            };
            if let Some(snapshot) = state.mark_runtime_error_if_epoch(runtime_epoch, code).await {
                host.emit(snapshot);
            }
            return;
        }
    };
    let runtime = match services.start(&installation, state.local_app_data()).await {
        Ok(runtime) => runtime,
        Err(failure) => {
            if let Some(runtime) = failure.into_runtime() {
                state
                    .restore_quarantined_runtime(RuntimeOwnership::failed_start(runtime, None))
                    .await;
            }
            if let Some(snapshot) = state
                .mark_runtime_error_if_epoch(runtime_epoch, "codex_incompatible")
                .await
            {
                host.emit(snapshot);
            }
            return;
        }
    };

    let event_task = host.spawn_event_loop(runtime.clone(), runtime_epoch);
    let consumer_started = tokio::time::timeout(EVENT_CONSUMER_START_TIMEOUT, async {
        loop {
            if runtime.process().persistent_event_waiter_count() > 0 {
                return true;
            }
            if event_task.is_finished() {
                return false;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or(false);
    if !consumer_started {
        stop_or_quarantine(
            state,
            RuntimeOwnership::failed_start(runtime.clone(), Some(event_task)),
        )
        .await;
        publish_incompatible_if_current(host.as_ref(), state, runtime_epoch).await;
        return;
    }

    let account = tokio::time::timeout(
        ACCOUNT_READ_TIMEOUT,
        runtime
            .client()
            .request("account/read", json!({"refreshToken": false})),
    )
    .await;
    let account = match account {
        Ok(Ok(response)) => match parse_startup_account_snapshot(&response) {
            Ok(account) => account,
            Err(()) => {
                stop_or_quarantine(
                    state,
                    RuntimeOwnership::failed_start(runtime.clone(), Some(event_task)),
                )
                .await;
                publish_incompatible_if_current(host.as_ref(), state, runtime_epoch).await;
                return;
            }
        },
        _ => {
            stop_or_quarantine(
                state,
                RuntimeOwnership::failed_start(runtime.clone(), Some(event_task)),
            )
            .await;
            publish_incompatible_if_current(host.as_ref(), state, runtime_epoch).await;
            return;
        }
    };

    let published = state
        .publish_runtime_ready(
            runtime_epoch,
            runtime.clone(),
            installation.version,
            installation.confidence == CodexVersionConfidence::Tested,
            account,
        )
        .await;
    if !published {
        stop_or_quarantine(
            state,
            RuntimeOwnership::failed_start(runtime.clone(), Some(event_task)),
        )
        .await;
        return;
    }
    if let Err(event_task) = state
        .install_runtime_event_task(runtime_epoch, event_task)
        .await
    {
        stop_or_quarantine(
            state,
            RuntimeOwnership::failed_start(runtime, Some(event_task)),
        )
        .await;
        return;
    }
    host.emit(state.snapshot().await);
    host.spawn_health_poller(runtime_epoch);
}

async fn terminate_runtime(runtime: &ManagedRuntime) -> Result<(), String> {
    match runtime.process().shutdown().await {
        Ok(()) => Ok(()),
        Err(_) => runtime.process().ensure_terminated().await,
    }
}

async fn process_quarantined_runtime(state: &DesktopState) -> bool {
    let Some(runtime) = state.take_quarantined_runtime().await else {
        return true;
    };
    match stop_owned_runtime(runtime).await {
        Ok(()) => true,
        Err(runtime) => {
            state.restore_quarantined_runtime(runtime).await;
            false
        }
    }
}

async fn stop_or_quarantine(state: &DesktopState, runtime: RuntimeOwnership) {
    if let Err(runtime) = stop_owned_runtime(runtime).await {
        state.restore_quarantined_runtime(runtime).await;
    }
}

async fn quarantine_protocol_error_runtime(state: &DesktopState, runtime_epoch: u64) {
    let _restart = state.lock_runtime_restart().await;
    let Some(runtime) = state.take_runtime_if_epoch(runtime_epoch).await else {
        return;
    };
    stop_or_quarantine(state, runtime).await;
}

async fn stop_owned_runtime(runtime: RuntimeOwnership) -> Result<(), RuntimeOwnership> {
    if terminate_runtime(runtime.runtime()).await.is_err() {
        return Err(runtime);
    }
    let (_, event_task, join_event_task) = runtime.into_parts();
    if join_event_task
        && let Some(mut event_task) = event_task
        && tokio::time::timeout(EVENT_CONSUMER_START_TIMEOUT, &mut event_task)
            .await
            .is_err()
    {
        event_task.abort();
        let _ = event_task.await;
    }
    Ok(())
}

async fn publish_incompatible_if_current(
    host: &dyn RuntimeHost,
    state: &DesktopState,
    runtime_epoch: u64,
) {
    if let Some(snapshot) = state
        .mark_runtime_error_if_epoch(runtime_epoch, "codex_incompatible")
        .await
    {
        host.emit(snapshot);
    }
}

pub async fn shutdown_runtime(app: AppHandle) {
    let state = app.state::<DesktopState>();
    state.poll_gate().cancel();
    let _restart = state.lock_runtime_restart().await;
    if let Some(runtime) = state.take_runtime().await {
        if let Err(runtime) = stop_owned_runtime(runtime).await {
            state.restore_quarantined_runtime(runtime).await;
        }
    }
    if let Some(runtime) = state.take_quarantined_runtime().await
        && let Err(runtime) = stop_owned_runtime(runtime).await
    {
        state.restore_quarantined_runtime(runtime).await;
    }
}

async fn refresh_account<R: tauri::Runtime>(app: &AppHandle<R>, state: &DesktopState) {
    let snapshot = match refresh_account_with(state).await {
        Ok(snapshot) => snapshot,
        Err(_) => state.apply_account(AccountSnapshot::SignedOut).await,
    };
    emit_snapshot(app, snapshot);
}

async fn run_event_loop<R: tauri::Runtime>(
    app: AppHandle<R>,
    runtime: ManagedRuntime,
    runtime_epoch: u64,
) {
    while let Some(event) = runtime.process().next_event().await {
        let state = app.state::<DesktopState>();
        match event {
            CodexEvent::Notification { method, params } => {
                handle_notification(&app, &state, runtime_epoch, &method, params).await;
            }
            CodexEvent::ServerRequest { id, method, params } => {
                if let Ok(Some(request)) =
                    handle_server_request_with(runtime.client().as_ref(), id, &method, params).await
                {
                    let _ = app.emit(
                        "desktop://event",
                        json!({
                            "type": "mcpServer/elicitation/declined",
                            "request": request
                        }),
                    );
                }
            }
            CodexEvent::ProtocolError { .. } => {
                if let Some(snapshot) = state
                    .mark_runtime_protocol_error_if_epoch(runtime_epoch)
                    .await
                {
                    emit_snapshot(&app, snapshot);
                }
                let cleanup_app = app.clone();
                tokio::spawn(async move {
                    let state = cleanup_app.state::<DesktopState>();
                    quarantine_protocol_error_runtime(&state, runtime_epoch).await;
                });
                break;
            }
            CodexEvent::ProcessExited { .. } => {
                if let Some(snapshot) = state.mark_runtime_stopped_if_epoch(runtime_epoch).await {
                    emit_snapshot(&app, snapshot);
                }
                break;
            }
        }
    }
}

async fn handle_notification<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    runtime_epoch: u64,
    method: &str,
    params: Value,
) {
    match method {
        "account/login/completed" | "account/updated" => {
            refresh_account(app, state).await;
        }
        CURRENT_STATUS_METHOD | LEGACY_STATUS_METHOD => {
            handle_mcp_status_notification_with(state, runtime_epoch, method, params).await;
        }
        "turn/completed" => {
            if let Some(event) = handle_turn_completed_event(state, params).await {
                let _ = app.emit("desktop://event", event);
            }
        }
        "item/agentMessage/delta" => {
            if let (Some(item_id), Some(text)) = (
                params.get("itemId").and_then(Value::as_str),
                params.get("delta").and_then(Value::as_str),
            ) {
                let _ = app.emit(
                    "desktop://event",
                    ProviderEvent::TextDelta {
                        item_id: item_id.to_owned(),
                        text: text.to_owned(),
                    },
                );
            }
        }
        "item/completed" => {
            if let Some(event) = tool_completion_event(&params) {
                if let Some(item) = event.get("item").cloned() {
                    let _ = app.emit("desktop://event", ProviderEvent::ToolCompleted { item });
                }
            }
        }
        _ => {}
    }
}

async fn run_health_poller<R: tauri::Runtime>(app: AppHandle<R>, runtime_epoch: u64) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let state = app.state::<DesktopState>();
        if state.poll_gate().is_cancelled() || !state.runtime_is_current(runtime_epoch).await {
            break;
        }
        let visible = app
            .get_webview_window("main")
            .and_then(|window| window.is_visible().ok())
            .unwrap_or(false);
        state.set_visible(visible).await;
        let prepared = prepare_health_refresh(&state, &current_timestamp()).await;
        let visible_after = app
            .get_webview_window("main")
            .and_then(|window| window.is_visible().ok())
            .unwrap_or(false);
        state.set_visible(visible_after).await;
        if let Ok(Some((lease, snapshot))) = prepared
            && state.commit_health(&lease, snapshot).await
        {
            emit_snapshot(&app, state.snapshot().await);
        }
        if !state.runtime_is_current(runtime_epoch).await {
            break;
        }
    }
}

fn emit_snapshot<R: tauri::Runtime>(app: &AppHandle<R>, snapshot: DesktopSnapshot) {
    let _ = app.emit(
        "desktop://event",
        json!({"type": "snapshot", "snapshot": snapshot}),
    );
}

#[cfg(debug_assertions)]
fn environment_args(variable: &str) -> Vec<OsString> {
    std::env::var_os(variable)
        .map(|value| {
            value
                .to_string_lossy()
                .split_whitespace()
                .map(OsString::from)
                .collect()
        })
        .unwrap_or_default()
}

pub fn current_timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod runtime_recovery_tests {
    use std::{
        collections::VecDeque,
        path::Path,
        sync::{
            Arc, Mutex as StdMutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use serde_json::{Value, json};
    use tokio::sync::{Mutex, Semaphore, mpsc};

    use super::*;
    use crate::{
        app_state::{CodexSnapshot, ManagedRuntime, RuntimeProcess},
        codex::{CodexDiscoveryError, CodexInstallation, CodexVersionConfidence},
    };

    struct WaiterCount<'a>(&'a AtomicUsize);

    impl Drop for WaiterCount<'_> {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::AcqRel);
        }
    }

    struct FakeRuntime {
        account_result: Result<Value, String>,
        account_observed: Option<mpsc::UnboundedSender<()>>,
        account_release: Option<Arc<Semaphore>>,
        required_shutdown: Option<Arc<FakeRuntime>>,
        events: Mutex<mpsc::UnboundedReceiver<CodexEvent>>,
        event_sender: mpsc::UnboundedSender<CodexEvent>,
        event_waiters: AtomicUsize,
        shutdowns: AtomicUsize,
        shutdown_failures: AtomicUsize,
        termination_confirmed: AtomicBool,
        exit_on_shutdown: bool,
    }

    impl FakeRuntime {
        fn new(account_result: Result<Value, String>, exit_on_shutdown: bool) -> Arc<Self> {
            Self::with_shutdown_failures(account_result, exit_on_shutdown, 0)
        }

        fn with_shutdown_failures(
            account_result: Result<Value, String>,
            exit_on_shutdown: bool,
            shutdown_failures: usize,
        ) -> Arc<Self> {
            let (event_sender, events) = mpsc::unbounded_channel();
            Arc::new(Self {
                account_result,
                account_observed: None,
                account_release: None,
                required_shutdown: None,
                events: Mutex::new(events),
                event_sender,
                event_waiters: AtomicUsize::new(0),
                shutdowns: AtomicUsize::new(0),
                shutdown_failures: AtomicUsize::new(shutdown_failures),
                termination_confirmed: AtomicBool::new(false),
                exit_on_shutdown,
            })
        }

        fn blocking_account(
            account_result: Result<Value, String>,
            observed: mpsc::UnboundedSender<()>,
            release: Arc<Semaphore>,
            required_shutdown: Arc<FakeRuntime>,
        ) -> Arc<Self> {
            let (event_sender, events) = mpsc::unbounded_channel();
            Arc::new(Self {
                account_result,
                account_observed: Some(observed),
                account_release: Some(release),
                required_shutdown: Some(required_shutdown),
                events: Mutex::new(events),
                event_sender,
                event_waiters: AtomicUsize::new(0),
                shutdowns: AtomicUsize::new(0),
                shutdown_failures: AtomicUsize::new(0),
                termination_confirmed: AtomicBool::new(false),
                exit_on_shutdown: false,
            })
        }

        fn managed(self: &Arc<Self>) -> ManagedRuntime {
            ManagedRuntime::new(self.clone(), self.clone())
        }

        fn exit(&self) {
            let _ = self
                .event_sender
                .send(CodexEvent::ProcessExited { code: Some(0) });
        }

        fn protocol_error(&self) {
            let _ = self.event_sender.send(CodexEvent::ProtocolError {
                message: "scripted live protocol error".to_owned(),
            });
        }

        fn shutdown_count(&self) -> usize {
            self.shutdowns.load(Ordering::Acquire)
        }

        fn termination_is_confirmed(&self) -> bool {
            self.termination_confirmed.load(Ordering::Acquire)
        }
    }

    impl AppServerClient for FakeRuntime {
        fn request<'a>(&'a self, method: &'a str, _params: Value) -> AppServerFuture<'a, Value> {
            Box::pin(async move {
                if method != "account/read" {
                    return Err(format!("unexpected request: {method}"));
                }
                if let Some(required) = &self.required_shutdown {
                    assert_eq!(
                        required.shutdown_count(),
                        1,
                        "the old runtime must be shut down before replacement self-test/publication"
                    );
                }
                if let Some(observed) = &self.account_observed {
                    observed.send(()).expect("account observation channel");
                }
                if let Some(release) = &self.account_release {
                    release
                        .acquire()
                        .await
                        .expect("account release semaphore")
                        .forget();
                }
                self.account_result.clone()
            })
        }

        fn respond<'a>(&'a self, _id: Value, _reply: ServerReply) -> AppServerFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }
    }

    impl RuntimeProcess for FakeRuntime {
        fn next_event<'a>(&'a self) -> BoxFuture<'a, Option<CodexEvent>> {
            Box::pin(async move {
                self.event_waiters.fetch_add(1, Ordering::AcqRel);
                let _waiter = WaiterCount(&self.event_waiters);
                self.events.lock().await.recv().await
            })
        }

        fn persistent_event_waiter_count(&self) -> usize {
            self.event_waiters.load(Ordering::Acquire)
        }

        fn shutdown<'a>(&'a self) -> BoxFuture<'a, Result<(), String>> {
            Box::pin(async move {
                self.shutdowns.fetch_add(1, Ordering::AcqRel);
                if self
                    .shutdown_failures
                    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                        remaining.checked_sub(1)
                    })
                    .is_ok()
                {
                    return Err("scripted shutdown timeout".to_owned());
                }
                self.termination_confirmed.store(true, Ordering::Release);
                if self.exit_on_shutdown {
                    self.exit();
                }
                Ok(())
            })
        }

        fn ensure_terminated<'a>(&'a self) -> BoxFuture<'a, Result<(), String>> {
            self.shutdown()
        }
    }

    struct FakeRuntimeServices {
        runtimes: StdMutex<VecDeque<ManagedRuntime>>,
        discoveries: AtomicUsize,
        starts: AtomicUsize,
    }

    struct TestRuntimeHost {
        state: Arc<DesktopState>,
        emitted: Arc<StdMutex<Vec<DesktopSnapshot>>>,
        terminal_events: Arc<AtomicUsize>,
        event_loops_finished: Arc<AtomicUsize>,
    }

    impl TestRuntimeHost {
        fn new(state: Arc<DesktopState>) -> Arc<Self> {
            Arc::new(Self {
                state,
                emitted: Arc::new(StdMutex::new(Vec::new())),
                terminal_events: Arc::new(AtomicUsize::new(0)),
                event_loops_finished: Arc::new(AtomicUsize::new(0)),
            })
        }
    }

    impl RuntimeHost for TestRuntimeHost {
        fn emit(&self, snapshot: DesktopSnapshot) {
            self.emitted
                .lock()
                .expect("emitted snapshots")
                .push(snapshot);
        }

        fn spawn_event_loop(
            &self,
            runtime: ManagedRuntime,
            runtime_epoch: u64,
        ) -> tokio::task::JoinHandle<()> {
            let state = self.state.clone();
            let terminal_events = self.terminal_events.clone();
            let event_loops_finished = self.event_loops_finished.clone();
            tokio::spawn(async move {
                struct Finished(Arc<AtomicUsize>);
                impl Drop for Finished {
                    fn drop(&mut self) {
                        self.0.fetch_add(1, Ordering::AcqRel);
                    }
                }
                let _finished = Finished(event_loops_finished);
                while let Some(event) = runtime.process().next_event().await {
                    match event {
                        CodexEvent::ProtocolError { .. } => {
                            state
                                .mark_runtime_protocol_error_if_epoch(runtime_epoch)
                                .await;
                            terminal_events.fetch_add(1, Ordering::AcqRel);
                            quarantine_protocol_error_runtime(&state, runtime_epoch).await;
                            break;
                        }
                        CodexEvent::ProcessExited { .. } => {
                            state.mark_runtime_stopped_if_epoch(runtime_epoch).await;
                            terminal_events.fetch_add(1, Ordering::AcqRel);
                            break;
                        }
                        _ => {}
                    }
                }
            })
        }

        fn spawn_health_poller(&self, _runtime_epoch: u64) {}
    }

    impl FakeRuntimeServices {
        fn new(runtimes: impl IntoIterator<Item = ManagedRuntime>) -> Arc<Self> {
            Arc::new(Self {
                runtimes: StdMutex::new(runtimes.into_iter().collect()),
                discoveries: AtomicUsize::new(0),
                starts: AtomicUsize::new(0),
            })
        }
    }

    impl RuntimeServices for FakeRuntimeServices {
        fn discover<'a>(
            &'a self,
            _local_app_data: &'a Path,
        ) -> BoxFuture<'a, Result<CodexInstallation, CodexDiscoveryError>> {
            self.discoveries.fetch_add(1, Ordering::AcqRel);
            Box::pin(async {
                Ok(CodexInstallation {
                    command: PathBuf::from(r"C:\fake\codex.exe"),
                    version: "0.149.0".to_owned(),
                    confidence: CodexVersionConfidence::Tested,
                })
            })
        }

        fn start<'a>(
            &'a self,
            _installation: &'a CodexInstallation,
            _local_app_data: &'a Path,
        ) -> BoxFuture<'a, Result<ManagedRuntime, RuntimeStartFailure>> {
            self.starts.fetch_add(1, Ordering::AcqRel);
            let runtime = self
                .runtimes
                .lock()
                .expect("runtime queue")
                .pop_front()
                .expect("scripted runtime");
            Box::pin(async move { Ok(runtime) })
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_rediscovery_coalesces_and_late_old_exit_cannot_replace_new_epoch() {
        let directory = tempfile::tempdir().unwrap();
        let state = Arc::new(DesktopState::new(directory.path().to_owned()));
        let host = TestRuntimeHost::new(state.clone());
        let old = FakeRuntime::new(Ok(json!({"account": null})), false);
        let (account_observed_tx, mut account_observed_rx) = mpsc::unbounded_channel();
        let account_release = Arc::new(Semaphore::new(0));
        let replacement = FakeRuntime::blocking_account(
            Ok(json!({"account": null})),
            account_observed_tx,
            account_release.clone(),
            old.clone(),
        );
        let unused = FakeRuntime::new(Ok(json!({"account": null})), true);
        let services =
            FakeRuntimeServices::new([old.managed(), replacement.managed(), unused.managed()]);

        restart_codex_runtime_with_services(&state, services.clone(), host.clone()).await;

        let first_state = state.clone();
        let first_services: Arc<dyn RuntimeServices> = services.clone();
        let first_host: Arc<dyn RuntimeHost> = host.clone();
        let first = tokio::spawn(async move {
            restart_codex_runtime_with_services(&first_state, first_services, first_host).await;
        });
        tokio::time::timeout(Duration::from_secs(1), account_observed_rx.recv())
            .await
            .expect("replacement account/read must start")
            .expect("account observation channel");

        let second_state = state.clone();
        let second_services: Arc<dyn RuntimeServices> = services.clone();
        let second_host: Arc<dyn RuntimeHost> = host.clone();
        let second_observed_epoch = state.runtime_epoch().await;
        let second = tokio::spawn(async move {
            restart_codex_runtime_after_observation(
                &second_state,
                second_services,
                second_host,
                second_observed_epoch,
            )
            .await;
        });
        account_release.add_permits(1);
        first.await.unwrap();
        second.await.unwrap();

        assert_eq!(services.starts.load(Ordering::Acquire), 2);
        assert_eq!(old.shutdown_count(), 1);
        assert!(matches!(
            state.snapshot().await.codex,
            CodexSnapshot::Ready { .. }
        ));

        old.exit();
        tokio::time::timeout(Duration::from_secs(1), async {
            while host.terminal_events.load(Ordering::Acquire) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("old terminal event must be processed");
        assert!(matches!(
            state.snapshot().await.codex,
            CodexSnapshot::Ready { .. }
        ));
        replacement.exit();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn account_read_failure_is_incompatible_and_failed_child_is_joined() {
        let directory = tempfile::tempdir().unwrap();
        let state = Arc::new(DesktopState::new(directory.path().to_owned()));
        let host = TestRuntimeHost::new(state.clone());
        let failed = FakeRuntime::new(Err("account/read failed".to_owned()), true);
        let services = FakeRuntimeServices::new([failed.managed()]);
        restart_codex_runtime_with_services(&state, services, host).await;

        assert_eq!(state.snapshot().await.account, AccountSnapshot::Checking);
        assert_eq!(
            state.snapshot().await.provider.runtime,
            crate::providers::ProviderRuntimeSnapshot::Error {
                code: "codex_incompatible".to_owned()
            }
        );
        assert_eq!(failed.shutdown_count(), 1);
        assert_eq!(failed.event_waiters.load(Ordering::Acquire), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn replacement_does_not_start_when_old_runtime_termination_is_unconfirmed() {
        let directory = tempfile::tempdir().unwrap();
        let state = Arc::new(DesktopState::new(directory.path().to_owned()));
        let host = TestRuntimeHost::new(state.clone());
        let old =
            FakeRuntime::with_shutdown_failures(Ok(json!({"account": null})), false, usize::MAX);
        let replacement = FakeRuntime::new(Ok(json!({"account": null})), true);
        let services = FakeRuntimeServices::new([old.managed(), replacement.managed()]);

        restart_codex_runtime_with_services(&state, services.clone(), host).await;
        restart_codex_runtime_with_services(
            &state,
            services.clone(),
            TestRuntimeHost::new(state.clone()),
        )
        .await;

        assert_eq!(
            services.starts.load(Ordering::Acquire),
            1,
            "replacement startup must not proceed without confirmed old-process termination"
        );
        assert!(!old.termination_is_confirmed());
        assert_eq!(
            state.snapshot().await.provider.runtime,
            crate::providers::ProviderRuntimeSnapshot::Error {
                code: "codex_incompatible".to_owned()
            }
        );
        old.exit();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn failed_account_self_test_confirms_termination_and_joins_event_task() {
        let directory = tempfile::tempdir().unwrap();
        let state = Arc::new(DesktopState::new(directory.path().to_owned()));
        let host = TestRuntimeHost::new(state.clone());
        let failed =
            FakeRuntime::with_shutdown_failures(Err("account/read failed".to_owned()), true, 1);
        let services = FakeRuntimeServices::new([failed.managed()]);

        restart_codex_runtime_with_services(&state, services, host.clone()).await;

        assert!(
            failed.termination_is_confirmed(),
            "restart must force and confirm termination after graceful shutdown fails"
        );
        assert_eq!(failed.shutdown_count(), 2);
        assert_eq!(host.event_loops_finished.load(Ordering::Acquire), 1);
        assert_eq!(failed.event_waiters.load(Ordering::Acquire), 0);
    }

    async fn assert_malformed_startup_account_is_incompatible(response: Value) {
        let directory = tempfile::tempdir().unwrap();
        let state = Arc::new(DesktopState::new(directory.path().to_owned()));
        let host = TestRuntimeHost::new(state.clone());
        let failed = FakeRuntime::new(Ok(response), true);
        let services = FakeRuntimeServices::new([failed.managed()]);

        restart_codex_runtime_with_services(&state, services, host.clone()).await;

        assert_eq!(state.snapshot().await.account, AccountSnapshot::Checking);
        assert_eq!(
            state.snapshot().await.provider.runtime,
            crate::providers::ProviderRuntimeSnapshot::Error {
                code: "codex_incompatible".to_owned()
            }
        );
        assert!(failed.termination_is_confirmed());
        assert_eq!(host.event_loops_finished.load(Ordering::Acquire), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn missing_startup_account_is_incompatible() {
        assert_malformed_startup_account_is_incompatible(json!({})).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn structurally_empty_startup_account_is_incompatible() {
        assert_malformed_startup_account_is_incompatible(json!({"account": {}})).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unsupported_startup_account_is_incompatible() {
        assert_malformed_startup_account_is_incompatible(json!({
            "account": {"type": "apiKey", "apiKey": "must-not-read"}
        }))
        .await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn malformed_chatgpt_startup_account_is_incompatible() {
        assert_malformed_startup_account_is_incompatible(json!({
            "account": {"type": "chatgpt", "email": 7, "planType": "plus"}
        }))
        .await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn structurally_valid_chatgpt_startup_account_is_ready() {
        let directory = tempfile::tempdir().unwrap();
        let state = Arc::new(DesktopState::new(directory.path().to_owned()));
        let host = TestRuntimeHost::new(state.clone());
        let runtime = FakeRuntime::new(
            Ok(json!({
                "account": {
                    "type": "chatgpt",
                    "email": "map@example.com",
                    "planType": "plus"
                }
            })),
            true,
        );
        let services = FakeRuntimeServices::new([runtime.managed()]);

        restart_codex_runtime_with_services(&state, services, host).await;

        assert_eq!(
            state.snapshot().await.account,
            AccountSnapshot::SignedIn {
                email: Some("map@example.com".to_owned()),
                plan_type: "plus".to_owned()
            }
        );
        assert!(matches!(
            state.snapshot().await.codex,
            CodexSnapshot::Ready { .. }
        ));
        runtime.exit();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn quarantined_old_runtime_blocks_discovery_and_start_on_every_retry() {
        let directory = tempfile::tempdir().unwrap();
        let state = Arc::new(DesktopState::new(directory.path().to_owned()));
        let host = TestRuntimeHost::new(state.clone());
        let old =
            FakeRuntime::with_shutdown_failures(Ok(json!({"account": null})), false, usize::MAX);
        let replacement = FakeRuntime::new(Ok(json!({"account": null})), true);
        let services = FakeRuntimeServices::new([old.managed(), replacement.managed()]);

        restart_codex_runtime_with_services(&state, services.clone(), host.clone()).await;
        restart_codex_runtime_with_services(&state, services.clone(), host.clone()).await;
        restart_codex_runtime_with_services(&state, services.clone(), host).await;

        assert_eq!(services.discoveries.load(Ordering::Acquire), 1);
        assert_eq!(services.starts.load(Ordering::Acquire), 1);
        assert_eq!(
            old.shutdown_count(),
            4,
            "each retry must re-attempt bounded termination of the same quarantined runtime"
        );
        assert!(!old.termination_is_confirmed());
        assert_eq!(
            state.snapshot().await.provider.runtime,
            crate::providers::ProviderRuntimeSnapshot::Error {
                code: "codex_incompatible".to_owned()
            }
        );
        old.exit();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn quarantined_failed_new_runtime_blocks_discovery_and_start_on_retry() {
        let directory = tempfile::tempdir().unwrap();
        let state = Arc::new(DesktopState::new(directory.path().to_owned()));
        let host = TestRuntimeHost::new(state.clone());
        let failed = FakeRuntime::with_shutdown_failures(
            Err("account/read failed".to_owned()),
            false,
            usize::MAX,
        );
        let replacement = FakeRuntime::new(Ok(json!({"account": null})), true);
        let services = FakeRuntimeServices::new([failed.managed(), replacement.managed()]);

        restart_codex_runtime_with_services(&state, services.clone(), host.clone()).await;
        restart_codex_runtime_with_services(&state, services.clone(), host).await;

        assert_eq!(services.discoveries.load(Ordering::Acquire), 1);
        assert_eq!(services.starts.load(Ordering::Acquire), 1);
        assert_eq!(
            failed.shutdown_count(),
            4,
            "retry must terminate the failed child already in quarantine before discovery"
        );
        assert!(!failed.termination_is_confirmed());
        assert_eq!(
            state.snapshot().await.provider.runtime,
            crate::providers::ProviderRuntimeSnapshot::Error {
                code: "codex_incompatible".to_owned()
            }
        );
        failed.exit();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn live_protocol_error_and_rediscovery_keep_one_cleanup_owner_quarantined() {
        let directory = tempfile::tempdir().unwrap();
        let state = Arc::new(DesktopState::new(directory.path().to_owned()));
        let host = TestRuntimeHost::new(state.clone());
        let live =
            FakeRuntime::with_shutdown_failures(Ok(json!({"account": null})), false, usize::MAX);
        let replacement = FakeRuntime::new(Ok(json!({"account": null})), true);
        let services = FakeRuntimeServices::new([live.managed(), replacement.managed()]);

        restart_codex_runtime_with_services(&state, services.clone(), host.clone()).await;
        let serialization = state.lock_runtime_restart().await;
        live.protocol_error();
        tokio::time::timeout(Duration::from_secs(1), async {
            while host.terminal_events.load(Ordering::Acquire) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("live ProtocolError must reach the event loop");

        let retry_state = state.clone();
        let retry_services: Arc<dyn RuntimeServices> = services.clone();
        let retry_host: Arc<dyn RuntimeHost> = host.clone();
        let retry = tokio::spawn(async move {
            restart_codex_runtime_with_services(&retry_state, retry_services, retry_host).await;
        });
        drop(serialization);
        retry.await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while host.event_loops_finished.load(Ordering::Acquire) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("ProtocolError cleanup must finish without self-joining");

        assert_eq!(services.discoveries.load(Ordering::Acquire), 1);
        assert_eq!(services.starts.load(Ordering::Acquire), 1);
        assert!(matches!(live.shutdown_count(), 2 | 4));
        assert!(!live.termination_is_confirmed());
        assert_eq!(
            state.snapshot().await.provider.runtime,
            crate::providers::ProviderRuntimeSnapshot::Error {
                code: "codex_incompatible".to_owned()
            }
        );
        live.exit();
    }
}

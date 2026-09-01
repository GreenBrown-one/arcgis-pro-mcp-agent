use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{mpsc, oneshot, watch},
    time::timeout,
};

use crate::{
    arcgis_tool_client::{ArcGisToolClient, BoxFuture, McpTool, McpToolResult, ToolClientError},
    runtime_secret::{RuntimeFile, create_runtime_file},
};

const INITIAL_REQUEST_ID: u64 = 1;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const CALL_TIMEOUT: Duration = Duration::from_secs(5);
const WRITER_CAPACITY: usize = 64;
const STDERR_CAPACITY: usize = 200;
const MAX_FRAME_BYTES: usize = 1_048_576;
const MAX_SCHEMA_BYTES: usize = 65_536;
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const CHILD_EXIT_TIMEOUT: Duration = Duration::from_secs(2);
const BASELINE_TOOLS: [&str; 17] = [
    "arcgis_connection_status",
    "arcgis_capabilities",
    "arcgis_describe_context",
    "arcgis_list_layers",
    "arcgis_describe_layer",
    "arcgis_list_fields",
    "arcgis_count_features",
    "arcgis_query_features",
    "arcgis_query_spatial",
    "arcgis_get_selection",
    "arcgis_select_by_attribute",
    "arcgis_select_by_location",
    "arcgis_clear_selection",
    "arcgis_activate_view",
    "arcgis_zoom_to_layer",
    "arcgis_zoom_to_extent",
    "arcgis_flash_features",
];

type Pending = Arc<StdMutex<HashMap<u64, oneshot::Sender<Result<Value, ToolClientError>>>>>;

#[derive(Clone)]
struct Redactor(Arc<[String]>);
impl Redactor {
    fn new(mut values: Vec<String>) -> Self {
        values.retain(|value| !value.is_empty());
        values.sort_by_key(|value| std::cmp::Reverse(value.len()));
        values.dedup();
        Self(values.into())
    }
    fn redact(&self, text: &str) -> String {
        self.0.iter().fold(text.to_owned(), |value, secret| {
            value.replace(secret, "[REDACTED]")
        })
    }
}

enum WriterCommand {
    Frame(Vec<u8>, oneshot::Sender<Result<(), ToolClientError>>),
    Close,
}

pub struct McpStartOptions {
    pub local_app_data: PathBuf,
}

pub fn resolve_private_mcp_command(
    current_exe: Option<&Path>,
    is_file: impl Fn(&Path) -> bool,
) -> Result<PathBuf, ToolClientError> {
    let desktop = current_exe
        .and_then(Path::parent)
        .filter(|directory| {
            directory
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("desktop"))
        })
        .ok_or(ToolClientError::Unavailable)?;
    let command = desktop
        .parent()
        .map(|root| root.join("mcp").join("ArcGISProAgent.Mcp.exe"))
        .ok_or(ToolClientError::Unavailable)?;
    if command
        .file_name()
        .is_some_and(|name| name == "ArcGISProAgent.Mcp.exe")
        && is_file(&command)
    {
        Ok(command)
    } else {
        Err(ToolClientError::Unavailable)
    }
}

fn build_mcp_command(runtime_file: &RuntimeFile) -> Result<Command, ToolClientError> {
    let current_exe = std::env::current_exe().map_err(|_| ToolClientError::Unavailable)?;
    let command_path = resolve_private_mcp_command(Some(&current_exe), Path::is_file)?;
    let mut command = Command::new(command_path);
    command
        .env_clear()
        .env("ARCGIS_AGENT_RUNTIME_FILE", runtime_file.path());
    Ok(command)
}

pub struct McpRuntime {
    writer: mpsc::Sender<WriterCommand>,
    pending: Pending,
    next_id: AtomicU64,
    stderr: Arc<StdMutex<VecDeque<String>>>,
    exited: Arc<AtomicBool>,
    stop: watch::Sender<bool>,
    exit: watch::Receiver<bool>,
    _runtime_file: Option<RuntimeFile>,
    tools: Vec<McpTool>,
}

impl McpRuntime {
    pub async fn start(options: McpStartOptions) -> Result<Self, ToolClientError> {
        let runtime_file = create_runtime_file(&options.local_app_data)
            .map_err(|_| ToolClientError::Unavailable)?;
        let command = build_mcp_command(&runtime_file)?;
        Self::start_inner(command, Some(runtime_file), Vec::new(), false).await
    }

    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub async fn start_with_command(command: Command) -> Result<Self, ToolClientError> {
        let secrets = redaction_secrets_from_command(&command);
        Self::start_inner(command, None, secrets, true).await
    }

    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub async fn start_with_command_and_secrets(
        command: Command,
        secrets: Vec<String>,
    ) -> Result<Self, ToolClientError> {
        Self::start_inner(command, None, secrets, true).await
    }

    async fn start_inner(
        mut command: Command,
        runtime_file: Option<RuntimeFile>,
        mut secrets: Vec<String>,
        allow_test_harness_prelude: bool,
    ) -> Result<Self, ToolClientError> {
        if let Some(file) = &runtime_file {
            secrets.push(file.redaction_secret().to_owned());
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut command = tokio::process::Command::from(command);
        command.kill_on_drop(true);
        let mut child = command.spawn().map_err(|_| ToolClientError::Unavailable)?;
        let stdin = child.stdin.take().ok_or(ToolClientError::Protocol)?;
        let stdout = child.stdout.take().ok_or(ToolClientError::Protocol)?;
        let stderr_stream = child.stderr.take().ok_or(ToolClientError::Protocol)?;
        let pending = Pending::default();
        let stderr = Arc::new(StdMutex::new(VecDeque::with_capacity(STDERR_CAPACITY)));
        let exited = Arc::new(AtomicBool::new(false));
        let (writer, writer_rx) = mpsc::channel(WRITER_CAPACITY);
        let (stop, stop_rx) = watch::channel(false);
        let (exit_tx, exit) = watch::channel(false);
        tokio::spawn(write_stdin(stdin, writer_rx, stop_rx.clone()));
        tokio::spawn(read_stdout(
            stdout,
            pending.clone(),
            exited.clone(),
            stop.clone(),
            allow_test_harness_prelude,
        ));
        tokio::spawn(read_stderr(
            stderr_stream,
            stderr.clone(),
            Redactor::new(secrets),
        ));
        tokio::spawn(supervise(
            child,
            stop_rx,
            pending.clone(),
            exited.clone(),
            exit_tx,
        ));
        let mut runtime = Self {
            writer,
            pending,
            next_id: AtomicU64::new(INITIAL_REQUEST_ID),
            stderr,
            exited,
            stop,
            exit,
            _runtime_file: runtime_file,
            tools: Vec::new(),
        };
        timeout(
            STARTUP_TIMEOUT,
            runtime.request(
                "initialize",
                json!({
                    "protocolVersion": "2025-11-25", "capabilities": {},
                    "clientInfo": {"name": "arcgis-pro-agent-desktop", "version": "0.2.0-preview.1"}
                }),
            ),
        )
        .await
        .map_err(|_| ToolClientError::TimedOut)??;
        runtime
            .notify("notifications/initialized", json!({}))
            .await?;
        let result = timeout(STARTUP_TIMEOUT, runtime.request("tools/list", json!({})))
            .await
            .map_err(|_| ToolClientError::TimedOut)??;
        runtime.tools = validate_tools(&result)?;
        Ok(runtime)
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, ToolClientError> {
        if self.exited.load(Ordering::Acquire) {
            return Err(ToolClientError::ProcessExited);
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let bytes = serde_json::to_vec(
            &json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params}),
        )
        .map_err(|_| ToolClientError::Protocol)?;
        let (tx, rx) = oneshot::channel();
        self.pending.lock().expect("pending mutex").insert(id, tx);
        let _guard = PendingGuard {
            id,
            pending: self.pending.clone(),
        };
        self.send(bytes).await?;
        rx.await.unwrap_or(Err(ToolClientError::ProcessExited))
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), ToolClientError> {
        let bytes = serde_json::to_vec(&json!({"jsonrpc":"2.0", "method":method, "params":params}))
            .map_err(|_| ToolClientError::Protocol)?;
        self.send(bytes).await
    }

    async fn send(&self, bytes: Vec<u8>) -> Result<(), ToolClientError> {
        let (tx, rx) = oneshot::channel();
        timeout(
            CALL_TIMEOUT,
            self.writer.send(WriterCommand::Frame(bytes, tx)),
        )
        .await
        .map_err(|_| ToolClientError::TimedOut)?
        .map_err(|_| ToolClientError::ProcessExited)?;
        timeout(CALL_TIMEOUT, rx)
            .await
            .map_err(|_| ToolClientError::TimedOut)?
            .unwrap_or(Err(ToolClientError::ProcessExited))
    }

    pub async fn stderr_lines(&self) -> Vec<String> {
        self.stderr
            .lock()
            .expect("stderr mutex")
            .iter()
            .cloned()
            .collect()
    }
    pub async fn shutdown(&self) -> Result<(), ToolClientError> {
        let _ = self.stop.send(true);
        let _ = self.writer.try_send(WriterCommand::Close);
        let mut exit = self.exit.clone();
        if *exit.borrow() {
            return Ok(());
        }
        timeout(SHUTDOWN_TIMEOUT, exit.changed())
            .await
            .map_err(|_| ToolClientError::TimedOut)?
            .map_err(|_| ToolClientError::ProcessExited)?;
        (*exit.borrow())
            .then_some(())
            .ok_or(ToolClientError::ProcessExited)
    }
}

#[cfg(debug_assertions)]
fn redaction_secrets_from_command(command: &Command) -> Vec<String> {
    let runtime_file = command
        .get_envs()
        .find_map(|(name, value)| {
            (name == "ARCGIS_AGENT_RUNTIME_FILE")
                .then_some(value)
                .flatten()
        })
        .map(PathBuf::from);
    runtime_file
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|contents| serde_json::from_slice::<Value>(&contents).ok())
        .and_then(|contents| {
            contents
                .get("token")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .into_iter()
        .collect()
}

impl ArcGisToolClient for McpRuntime {
    fn list_tools<'a>(&'a self) -> BoxFuture<'a, Result<Vec<McpTool>, ToolClientError>> {
        Box::pin(async move { Ok(self.tools.clone()) })
    }
    fn call_tool<'a>(
        &'a self,
        name: &'a str,
        arguments: Value,
    ) -> BoxFuture<'a, Result<McpToolResult, ToolClientError>> {
        Box::pin(async move {
            if !BASELINE_TOOLS.contains(&name) || !arguments.is_object() {
                return Err(ToolClientError::Protocol);
            }
            let result = timeout(
                CALL_TIMEOUT,
                self.request("tools/call", json!({"name":name, "arguments":arguments})),
            )
            .await
            .map_err(|_| ToolClientError::TimedOut)??;
            Ok(McpToolResult {
                content: result.get("content").cloned().unwrap_or(Value::Null),
                structured_content: result
                    .get("structuredContent")
                    .cloned()
                    .unwrap_or(Value::Null),
                is_error: result
                    .get("isError")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
    }
}

impl Drop for McpRuntime {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

struct PendingGuard {
    id: u64,
    pending: Pending,
}
impl Drop for PendingGuard {
    fn drop(&mut self) {
        self.pending.lock().expect("pending mutex").remove(&self.id);
    }
}

async fn write_stdin(
    mut stdin: tokio::process::ChildStdin,
    mut commands: mpsc::Receiver<WriterCommand>,
    mut stop: watch::Receiver<bool>,
) {
    loop {
        let command = tokio::select! { changed = stop.changed() => { if changed.is_err() || *stop.borrow() { return; } continue; }, command = commands.recv() => command };
        match command {
            Some(WriterCommand::Frame(bytes, done)) => {
                let result = async {
                    stdin
                        .write_all(&bytes)
                        .await
                        .map_err(|_| ToolClientError::ProcessExited)?;
                    stdin
                        .write_all(b"\n")
                        .await
                        .map_err(|_| ToolClientError::ProcessExited)?;
                    stdin
                        .flush()
                        .await
                        .map_err(|_| ToolClientError::ProcessExited)
                }
                .await;
                let _ = done.send(result);
            }
            Some(WriterCommand::Close) | None => return,
        }
    }
}

async fn read_stdout(
    stdout: tokio::process::ChildStdout,
    pending: Pending,
    exited: Arc<AtomicBool>,
    stop: watch::Sender<bool>,
    allow_test_harness_prelude: bool,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        let line = match read_frame(&mut reader).await {
            Ok(Some(line)) => line,
            Ok(None) => {
                break fail_runtime(&pending, &exited, &stop, ToolClientError::ProcessExited);
            }
            Err(error) => break fail_runtime(&pending, &exited, &stop, error),
        };
        if line.trim().is_empty() || (allow_test_harness_prelude && line == "running 1 test") {
            continue;
        }
        let value = match serde_json::from_str::<Value>(&line) {
            Ok(value) => value,
            Err(_) => break fail_runtime(&pending, &exited, &stop, ToolClientError::Protocol),
        };
        let Some(id) = value.get("id").and_then(Value::as_u64) else {
            continue;
        };
        let result = match (value.get("result"), value.get("error")) {
            (Some(result), None) => Ok(result.clone()),
            (None, Some(_)) => Err(ToolClientError::Server),
            _ => break fail_runtime(&pending, &exited, &stop, ToolClientError::Protocol),
        };
        if let Some(sender) = pending.lock().expect("pending mutex").remove(&id) {
            let _ = sender.send(result);
        }
    }
}

fn fail_runtime(
    pending: &Pending,
    exited: &AtomicBool,
    stop: &watch::Sender<bool>,
    error: ToolClientError,
) {
    exited.store(true, Ordering::Release);
    for (_, sender) in pending.lock().expect("pending mutex").drain() {
        let _ = sender.send(Err(error.clone()));
    }
    let _ = stop.send(true);
}

async fn read_frame(
    reader: &mut BufReader<tokio::process::ChildStdout>,
) -> Result<Option<String>, ToolClientError> {
    read_bounded_frame(reader).await
}

async fn read_stderr(
    stderr: tokio::process::ChildStderr,
    ring: Arc<StdMutex<VecDeque<String>>>,
    redactor: Redactor,
) {
    let mut reader = BufReader::new(stderr);
    while let Ok(Some(line)) = read_stderr_frame(&mut reader).await {
        let mut ring = ring.lock().expect("stderr mutex");
        if ring.len() == STDERR_CAPACITY {
            ring.pop_front();
        }
        ring.push_back(redactor.redact(&line));
    }
}
async fn read_stderr_frame(
    reader: &mut BufReader<tokio::process::ChildStderr>,
) -> Result<Option<String>, ToolClientError> {
    read_bounded_frame(reader).await
}

async fn read_bounded_frame<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<Option<String>, ToolClientError> {
    let mut bytes = Vec::new();
    loop {
        let available = reader
            .fill_buf()
            .await
            .map_err(|_| ToolClientError::ProcessExited)?;
        if available.is_empty() {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err(ToolClientError::Protocol)
            };
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |position| position + 1);
        let payload_len = take - usize::from(newline.is_some());
        if bytes.len() + payload_len > MAX_FRAME_BYTES {
            return Err(ToolClientError::Protocol);
        }
        bytes.extend_from_slice(&available[..payload_len]);
        reader.consume(take);
        if newline.is_some() {
            break;
        }
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| ToolClientError::Protocol)
}

async fn supervise(
    mut child: tokio::process::Child,
    mut stop: watch::Receiver<bool>,
    pending: Pending,
    exited: Arc<AtomicBool>,
    exit: watch::Sender<bool>,
) {
    tokio::select! {
        _ = child.wait() => {},
        changed = stop.changed() => {
            if changed.is_ok() && *stop.borrow() {
                let _ = child.start_kill();
                let _ = timeout(CHILD_EXIT_TIMEOUT, child.wait()).await;
            }
        }
    }
    exited.store(true, Ordering::Release);
    for (_, sender) in pending.lock().expect("pending mutex").drain() {
        let _ = sender.send(Err(ToolClientError::ProcessExited));
    }
    let _ = exit.send(true);
}

fn validate_tools(result: &Value) -> Result<Vec<McpTool>, ToolClientError> {
    let tools = result
        .get("tools")
        .and_then(Value::as_array)
        .ok_or(ToolClientError::Protocol)?;
    if tools.len() != BASELINE_TOOLS.len() {
        return Err(ToolClientError::Protocol);
    }
    let mut names = HashSet::new();
    let mut validated = Vec::with_capacity(tools.len());
    for tool in tools {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| BASELINE_TOOLS.contains(name))
            .ok_or(ToolClientError::Protocol)?;
        if !names.insert(name) {
            return Err(ToolClientError::Protocol);
        }
        let schema = tool
            .get("inputSchema")
            .filter(|schema| schema.is_object())
            .ok_or(ToolClientError::Protocol)?;
        if serde_json::to_vec(schema)
            .map_err(|_| ToolClientError::Protocol)?
            .len()
            > MAX_SCHEMA_BYTES
        {
            return Err(ToolClientError::Protocol);
        }
        validated.push(McpTool {
            name: name.to_owned(),
            description: tool
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_owned),
            input_schema: schema.clone(),
        });
    }
    if !BASELINE_TOOLS.iter().all(|name| names.contains(name)) {
        return Err(ToolClientError::Protocol);
    }
    validated.sort_by_key(|tool| {
        BASELINE_TOOLS
            .iter()
            .position(|name| *name == tool.name)
            .expect("allowlisted")
    });
    Ok(validated)
}

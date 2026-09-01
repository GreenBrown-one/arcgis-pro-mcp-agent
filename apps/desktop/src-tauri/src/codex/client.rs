use std::{
    collections::{HashMap, VecDeque},
    ffi::OsString,
    fmt,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{ChildStderr, ChildStdout},
    sync::{Notify, broadcast, mpsc, oneshot, watch},
    task::JoinHandle,
    time::{Instant, timeout, timeout_at},
};

use super::protocol::{CodexEvent, WireNotification, WireRequest};
use crate::runtime_secret::{RuntimeFile, create_runtime_file};

const INITIAL_REQUEST_ID: u64 = 1;
const EVENT_CAPACITY: usize = 256;
const PERSISTENT_EVENT_CAPACITY: usize = 256;
const PERSISTENT_EVENT_LAGGED_MESSAGE: &str =
    "persistent event queue lagged; oldest events were dropped";
const STDERR_CAPACITY: usize = 200;
const WRITER_CAPACITY: usize = 64;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const WRITER_QUEUE_TIMEOUT: Duration = Duration::from_secs(5);
const WRITER_IO_TIMEOUT: Duration = Duration::from_secs(5);
const WRITER_ACK_TIMEOUT: Duration = Duration::from_secs(15);
const CHILD_KILL_TIMEOUT: Duration = Duration::from_secs(5);
const PROCESS_TASK_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
const SHUTDOWN_COORDINATION_TIMEOUT: Duration = Duration::from_secs(1);
const SHUTDOWN_TERMINAL_TIMEOUT: Duration = Duration::from_secs(11);

type Pending = Arc<StdMutex<HashMap<u64, oneshot::Sender<Result<Value, CodexError>>>>>;

#[derive(Clone, Default)]
struct SecretRedactor {
    secrets: Arc<[String]>,
}

impl SecretRedactor {
    fn new(mut secrets: Vec<String>) -> Self {
        secrets.retain(|secret| !secret.is_empty());
        secrets.sort_by_key(|secret| std::cmp::Reverse(secret.len()));
        secrets.dedup();
        Self {
            secrets: secrets.into(),
        }
    }

    fn redact(&self, value: &str) -> String {
        self.secrets
            .iter()
            .fold(value.to_owned(), |redacted, secret| {
                redacted.replace(secret, "[REDACTED]")
            })
    }
}

impl fmt::Debug for SecretRedactor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretRedactor([REDACTED])")
    }
}

impl fmt::Display for SecretRedactor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Clone)]
struct Events {
    live: broadcast::Sender<CodexEvent>,
    reliable: Arc<ReliableEvents>,
}

impl Events {
    fn new(live: broadcast::Sender<CodexEvent>) -> Self {
        Self {
            live,
            reliable: Arc::new(ReliableEvents::new()),
        }
    }

    fn publish(&self, event: CodexEvent) {
        if self.reliable.publish(event.clone()) {
            let _ = self.live.send(event);
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<CodexEvent> {
        self.live.subscribe()
    }

    async fn next(&self) -> Option<CodexEvent> {
        self.reliable.next().await
    }
}

struct ReliableEvents {
    state: StdMutex<ReliableEventState>,
    changed: Notify,
    terminal: watch::Sender<bool>,
    active_waiters: AtomicUsize,
}

#[derive(Default)]
struct ReliableEventState {
    queue: VecDeque<CodexEvent>,
    terminal: bool,
}

impl ReliableEvents {
    fn new() -> Self {
        let (terminal, _) = watch::channel(false);
        Self {
            state: StdMutex::new(ReliableEventState::default()),
            changed: Notify::new(),
            terminal,
            active_waiters: AtomicUsize::new(0),
        }
    }

    fn publish(&self, event: CodexEvent) -> bool {
        let terminal = matches!(event, CodexEvent::ProcessExited { .. });
        let mut state = self.state.lock().expect("reliable event mutex poisoned");
        if state.terminal {
            return false;
        }
        push_persistent_event(&mut state.queue, event, terminal);
        state.terminal = terminal;
        drop(state);
        if terminal {
            self.terminal.send_replace(true);
        } else {
            self.changed.notify_one();
        }
        true
    }

    async fn next(&self) -> Option<CodexEvent> {
        let _waiter = ActiveEventWaiter::new(&self.active_waiters);
        let mut terminal = self.terminal.subscribe();
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let mut state = self.state.lock().expect("reliable event mutex poisoned");
                if let Some(event) = state.queue.pop_front() {
                    return Some(event);
                }
                if *terminal.borrow() {
                    return None;
                }
            }
            tokio::select! {
                _ = &mut changed => {}
                result = terminal.changed() => {
                    if result.is_err() {
                        return None;
                    }
                }
            }
        }
    }

    fn active_waiter_count(&self) -> usize {
        self.active_waiters.load(Ordering::Acquire)
    }
}

fn push_persistent_event(queue: &mut VecDeque<CodexEvent>, event: CodexEvent, terminal: bool) {
    if terminal {
        make_persistent_room(queue, 1, true);
        queue.push_back(event);
        return;
    }

    if queue.len() < PERSISTENT_EVENT_CAPACITY {
        queue.push_back(event);
        return;
    }

    let has_lagged_diagnostic = queue.iter().any(is_persistent_lagged_diagnostic);
    let required = if has_lagged_diagnostic { 1 } else { 2 };
    make_persistent_room(queue, required, has_lagged_diagnostic);
    if !has_lagged_diagnostic {
        queue.push_back(CodexEvent::ProtocolError {
            message: PERSISTENT_EVENT_LAGGED_MESSAGE.to_owned(),
        });
    }
    queue.push_back(event);
}

fn make_persistent_room(
    queue: &mut VecDeque<CodexEvent>,
    required: usize,
    preserve_lagged_diagnostic: bool,
) {
    while queue.len() + required > PERSISTENT_EVENT_CAPACITY {
        let position = preserve_lagged_diagnostic
            .then(|| {
                queue
                    .iter()
                    .position(|event| !is_persistent_lagged_diagnostic(event))
            })
            .flatten()
            .unwrap_or(0);
        queue.remove(position);
    }
}

fn is_persistent_lagged_diagnostic(event: &CodexEvent) -> bool {
    matches!(event, CodexEvent::ProtocolError { message } if message == PERSISTENT_EVENT_LAGGED_MESSAGE)
}

struct ActiveEventWaiter<'a> {
    count: &'a AtomicUsize,
}

impl<'a> ActiveEventWaiter<'a> {
    fn new(count: &'a AtomicUsize) -> Self {
        count.fetch_add(1, Ordering::AcqRel);
        Self { count }
    }
}

impl Drop for ActiveEventWaiter<'_> {
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::AcqRel);
    }
}

pub enum CodexError {
    Io {
        operation: &'static str,
        os_code: Option<i32>,
    },
    Protocol {
        operation: &'static str,
    },
    Server,
    ProcessExited {
        code: Option<i32>,
    },
    StartupTimedOut,
    ShutdownTimedOut,
    RuntimeCredentials,
    WriterTimedOut {
        operation: &'static str,
    },
}

impl CodexError {
    fn io(operation: &'static str, error: &std::io::Error) -> Self {
        Self::Io {
            operation,
            os_code: error.raw_os_error(),
        }
    }
}

impl fmt::Debug for CodexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl fmt::Display for CodexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, os_code } => {
                write!(formatter, "Codex process I/O failed: {operation}")?;
                if let Some(code) = os_code {
                    write!(formatter, " (OS error {code})")?;
                }
                Ok(())
            }
            Self::Protocol { operation } => write!(formatter, "Codex protocol failed: {operation}"),
            Self::Server => formatter.write_str("Codex app-server returned an error"),
            Self::ProcessExited { code } => {
                write!(formatter, "Codex app-server exited (code {code:?})")
            }
            Self::StartupTimedOut => {
                formatter.write_str("Codex app-server initialization timed out")
            }
            Self::ShutdownTimedOut => formatter.write_str("Codex app-server shutdown timed out"),
            Self::RuntimeCredentials => {
                formatter.write_str("Codex runtime credentials could not be prepared")
            }
            Self::WriterTimedOut { operation } => {
                write!(formatter, "Codex writer timed out: {operation}")
            }
        }
    }
}

impl std::error::Error for CodexError {}

pub(crate) struct CodexStartFailure {
    error: CodexError,
    runtime: Option<CodexRuntime>,
}

impl CodexStartFailure {
    fn new(error: CodexError, runtime: Option<CodexRuntime>) -> Self {
        Self { error, runtime }
    }

    pub(crate) fn into_parts(self) -> (CodexError, Option<CodexRuntime>) {
        (self.error, self.runtime)
    }

    fn into_error(self) -> CodexError {
        self.error
    }
}

impl From<CodexError> for CodexStartFailure {
    fn from(error: CodexError) -> Self {
        Self::new(error, None)
    }
}

impl fmt::Debug for CodexStartFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl fmt::Display for CodexStartFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, formatter)
    }
}

impl std::error::Error for CodexStartFailure {}

enum ProcessControl {
    Kill { completion_deadline: Instant },
}

enum WriterCommand {
    Frame {
        bytes: Vec<u8>,
        completion: oneshot::Sender<Result<(), CodexError>>,
    },
    Close {
        completion: oneshot::Sender<Result<(), CodexError>>,
    },
}

pub struct CodexRuntime {
    writer: mpsc::Sender<WriterCommand>,
    pending: Pending,
    next_id: AtomicU64,
    events: Events,
    stderr: Arc<StdMutex<VecDeque<String>>>,
    exited: Arc<AtomicBool>,
    exit: watch::Receiver<Option<Option<i32>>>,
    process_control: mpsc::Sender<ProcessControl>,
    _runtime_file: Option<RuntimeFile>,
}

pub struct CodexStartOptions {
    pub codex_command: PathBuf,
    pub codex_home: PathBuf,
    pub mcp_command: PathBuf,
    pub mcp_args: Vec<OsString>,
    pub local_app_data: PathBuf,
}

pub fn build_codex_command(options: &CodexStartOptions, runtime_file: &RuntimeFile) -> Command {
    let mcp_command = toml_string(&options.mcp_command.to_string_lossy());
    let mcp_args = format!(
        "[{}]",
        options
            .mcp_args
            .iter()
            .map(|argument| toml_string(&argument.to_string_lossy()))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let runtime_path = toml_string(&runtime_file.path().to_string_lossy());
    let runtime_environment = format!("{{ ARCGIS_AGENT_RUNTIME_FILE = {runtime_path} }}");

    let mut command = Command::new(&options.codex_command);
    command
        .arg("app-server")
        .arg("--stdio")
        .arg("-c")
        .arg("mcp_servers={}")
        .arg("-c")
        .arg(format!("mcp_servers.arcgis.command={mcp_command}"))
        .arg("-c")
        .arg(format!("mcp_servers.arcgis.args={mcp_args}"))
        .arg("-c")
        .arg(format!("mcp_servers.arcgis.env={runtime_environment}"))
        .env("CODEX_HOME", &options.codex_home)
        .env_remove("OPENAI_API_KEY")
        .env_remove("AZURE_OPENAI_API_KEY")
        .env_remove("CODEX_API_KEY");
    command
}

fn toml_string(value: &str) -> String {
    serde_json::to_string(value).expect("serializing a string cannot fail")
}

impl CodexRuntime {
    pub(crate) async fn start(options: CodexStartOptions) -> Result<Self, CodexStartFailure> {
        let runtime_file = create_runtime_file(&options.local_app_data)
            .map_err(|_| CodexStartFailure::from(CodexError::RuntimeCredentials))?;
        let command = build_codex_command(&options, &runtime_file);
        let redaction_secrets = vec![runtime_file.redaction_secret().to_owned()];
        Self::start_inner(command, Some(runtime_file), redaction_secrets).await
    }

    pub async fn start_with_command(command: Command) -> Result<Self, CodexError> {
        Self::start_inner(command, None, Vec::new())
            .await
            .map_err(CodexStartFailure::into_error)
    }

    #[doc(hidden)]
    pub async fn start_with_command_and_secrets(
        command: Command,
        redaction_secrets: Vec<String>,
    ) -> Result<Self, CodexError> {
        Self::start_inner(command, None, redaction_secrets)
            .await
            .map_err(CodexStartFailure::into_error)
    }

    async fn start_inner(
        mut command: Command,
        runtime_file: Option<RuntimeFile>,
        redaction_secrets: Vec<String>,
    ) -> Result<Self, CodexStartFailure> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child_command = tokio::process::Command::from(command);
        child_command.kill_on_drop(true);
        let mut child = child_command
            .spawn()
            .map_err(|error| CodexStartFailure::from(CodexError::io("spawn app-server", &error)))?;
        let stdin = child.stdin.take().ok_or(CodexError::Protocol {
            operation: "capture app-server stdin",
        })?;
        let stdout = child.stdout.take().ok_or(CodexError::Protocol {
            operation: "capture app-server stdout",
        })?;
        let stderr = child.stderr.take().ok_or(CodexError::Protocol {
            operation: "capture app-server stderr",
        })?;

        let pending = Pending::default();
        let stderr_ring = Arc::new(StdMutex::new(VecDeque::with_capacity(STDERR_CAPACITY)));
        let redactor = SecretRedactor::new(redaction_secrets);
        let (live_events, _) = broadcast::channel(EVENT_CAPACITY);
        let events = Events::new(live_events);
        let (exit_tx, exit) = watch::channel(None);
        let (process_control, process_commands) = mpsc::channel(1);
        let (writer, writer_commands) = mpsc::channel(WRITER_CAPACITY);
        let (writer_stop, writer_stop_receiver) = watch::channel(false);
        let exited = Arc::new(AtomicBool::new(false));

        let writer_task = tokio::spawn(write_stdin(
            stdin,
            writer_commands,
            writer_stop_receiver,
            process_control.clone(),
        ));
        let stdout_task = tokio::spawn(read_stdout(stdout, pending.clone(), events.clone()));
        let stderr_task = tokio::spawn(read_stderr(stderr, stderr_ring.clone(), redactor));
        tokio::spawn(supervise_process(
            child,
            process_commands,
            writer_stop,
            stdout_task,
            stderr_task,
            writer_task,
            pending.clone(),
            events.clone(),
            exited.clone(),
            exit_tx,
        ));

        let runtime = Self {
            writer,
            pending,
            next_id: AtomicU64::new(INITIAL_REQUEST_ID),
            events,
            stderr: stderr_ring,
            exited,
            exit,
            process_control,
            _runtime_file: runtime_file,
        };

        let startup = async {
            timeout(
                STARTUP_TIMEOUT,
                runtime.request(
                    "initialize",
                    json!({
                        "clientInfo": {
                            "name": "arcgis_pro_agent",
                            "title": "ArcGIS Pro Agent",
                            "version": "0.1.0"
                        },
                        "capabilities": {
                            "mcpServerOpenaiFormElicitation": true
                        }
                    }),
                ),
            )
            .await
            .map_err(|_| CodexError::StartupTimedOut)??;
            runtime.send_notification("initialized", json!({})).await
        }
        .await;
        if let Err(error) = startup {
            let runtime = runtime.shutdown().await.err().map(|_| runtime);
            return Err(CodexStartFailure::new(error, runtime));
        }

        Ok(runtime)
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value, CodexError> {
        if self.exited.load(Ordering::Acquire) {
            return Err(CodexError::ProcessExited {
                code: self.exit.borrow().flatten(),
            });
        }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let bytes = serde_json::to_vec(&WireRequest { method, id, params }).map_err(|_| {
            CodexError::Protocol {
                operation: "serialize request",
            }
        })?;
        let (sender, receiver) = oneshot::channel();
        self.pending
            .lock()
            .expect("pending request mutex poisoned")
            .insert(id, sender);
        let _pending_guard = PendingGuard::new(id, self.pending.clone());
        if self.exited.load(Ordering::Acquire) {
            return Err(CodexError::ProcessExited {
                code: self.exit.borrow().flatten(),
            });
        }

        self.send_frame(bytes).await?;
        receiver.await.unwrap_or(Err(CodexError::ProcessExited {
            code: self.exit.borrow().flatten(),
        }))
    }

    pub(crate) async fn send_server_response(&self, response: Value) -> Result<(), CodexError> {
        let bytes = serde_json::to_vec(&response).map_err(|_| CodexError::Protocol {
            operation: "serialize server response",
        })?;
        self.send_frame(bytes).await
    }

    /// Subscribes to future live events through a bounded, lossy broadcast stream.
    ///
    /// Events published before this call are not replayed, slow receivers report
    /// [`broadcast::error::RecvError::Lagged`], and a receiver created after
    /// `ProcessExited` will not see that terminal event. Do not mix this API with
    /// [`Self::next_event`] for the same logical consumer.
    pub fn subscribe(&self) -> broadcast::Receiver<CodexEvent> {
        self.events.subscribe()
    }

    /// Removes the next event from the bounded persistent event queue.
    ///
    /// Calls compete for events: with concurrent callers, each queued event is
    /// delivered to at most one caller. The queue retains pre-subscription events,
    /// drops the oldest events on overflow with one retained lag diagnostic, and
    /// always retains `ProcessExited`. After that terminal event is consumed, this
    /// method returns `None` for every current and future waiter. Do not mix this
    /// API with [`Self::subscribe`] for the same logical consumer.
    pub async fn next_event(&self) -> Option<CodexEvent> {
        self.events.next().await
    }

    #[doc(hidden)]
    pub fn persistent_event_waiter_count(&self) -> usize {
        self.events.reliable.active_waiter_count()
    }

    pub async fn stderr_lines(&self) -> Vec<String> {
        self.stderr
            .lock()
            .expect("stderr ring mutex poisoned")
            .iter()
            .cloned()
            .collect()
    }

    #[doc(hidden)]
    pub async fn pending_request_count(&self) -> usize {
        self.pending
            .lock()
            .expect("pending request mutex poisoned")
            .len()
    }

    pub async fn shutdown(&self) -> Result<(), CodexError> {
        let mut exit = self.exit.clone();
        if exit.borrow().is_some() {
            return Ok(());
        }
        let graceful_deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        if !matches!(
            timeout_at(graceful_deadline, self.close_writer()).await,
            Ok(Ok(()))
        ) {
            return self.ensure_terminated().await;
        }
        if wait_for_exit_until(&mut exit, graceful_deadline).await {
            return Ok(());
        }

        self.ensure_terminated().await
    }

    pub async fn ensure_terminated(&self) -> Result<(), CodexError> {
        let mut exit = self.exit.clone();
        if exit.borrow().is_some() {
            return Ok(());
        }
        let forced_deadline = Instant::now() + SHUTDOWN_TERMINAL_TIMEOUT;
        self.request_process_kill_by(forced_deadline);
        wait_for_exit_until(&mut exit, forced_deadline)
            .await
            .then_some(())
            .ok_or(CodexError::ShutdownTimedOut)
    }

    async fn send_notification(&self, method: &str, params: Value) -> Result<(), CodexError> {
        let bytes = serde_json::to_vec(&WireNotification { method, params }).map_err(|_| {
            CodexError::Protocol {
                operation: "serialize notification",
            }
        })?;
        self.send_frame(bytes).await
    }

    async fn send_frame(&self, mut bytes: Vec<u8>) -> Result<(), CodexError> {
        bytes.push(b'\n');
        let (completion, completed) = oneshot::channel();
        timeout(
            WRITER_QUEUE_TIMEOUT,
            self.writer.send(WriterCommand::Frame { bytes, completion }),
        )
        .await
        .map_err(|_| {
            self.request_process_kill();
            CodexError::WriterTimedOut {
                operation: "queue frame",
            }
        })?
        .map_err(|_| CodexError::ProcessExited {
            code: self.exit.borrow().flatten(),
        })?;
        timeout(WRITER_ACK_TIMEOUT, completed)
            .await
            .map_err(|_| {
                self.request_process_kill();
                CodexError::WriterTimedOut {
                    operation: "complete frame",
                }
            })?
            .unwrap_or(Err(CodexError::ProcessExited {
                code: self.exit.borrow().flatten(),
            }))
    }

    async fn close_writer(&self) -> Result<(), CodexError> {
        let (completion, completed) = oneshot::channel();
        timeout(
            WRITER_QUEUE_TIMEOUT,
            self.writer.send(WriterCommand::Close { completion }),
        )
        .await
        .map_err(|_| CodexError::WriterTimedOut {
            operation: "queue close",
        })?
        .map_err(|_| CodexError::ProcessExited {
            code: self.exit.borrow().flatten(),
        })?;
        timeout(WRITER_ACK_TIMEOUT, completed)
            .await
            .map_err(|_| CodexError::WriterTimedOut {
                operation: "close stdin",
            })?
            .unwrap_or(Err(CodexError::ProcessExited {
                code: self.exit.borrow().flatten(),
            }))
    }

    fn request_process_kill(&self) {
        self.request_process_kill_by(Instant::now() + SHUTDOWN_TERMINAL_TIMEOUT);
    }

    fn request_process_kill_by(&self, completion_deadline: Instant) {
        let _ = self.process_control.try_send(ProcessControl::Kill {
            completion_deadline,
        });
    }
}

struct PendingGuard {
    id: u64,
    pending: Pending,
}

impl PendingGuard {
    fn new(id: u64, pending: Pending) -> Self {
        Self { id, pending }
    }
}

impl Drop for PendingGuard {
    fn drop(&mut self) {
        self.pending
            .lock()
            .expect("pending request mutex poisoned")
            .remove(&self.id);
    }
}

impl Drop for CodexRuntime {
    fn drop(&mut self) {
        let _ = self.process_control.try_send(ProcessControl::Kill {
            completion_deadline: Instant::now() + SHUTDOWN_TERMINAL_TIMEOUT,
        });
    }
}

async fn read_stdout(stdout: ChildStdout, pending: Pending, events: Events) {
    let mut lines = BufReader::new(stdout).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => route_stdout_line(&line, &pending, &events).await,
            Ok(None) => break,
            Err(_) => {
                events.publish(CodexEvent::ProtocolError {
                    message: "failed to read app-server stdout".to_owned(),
                });
                break;
            }
        }
    }
}

async fn route_stdout_line(line: &str, pending: &Pending, events: &Events) {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        events.publish(CodexEvent::ProtocolError {
            message: "app-server emitted malformed JSONL".to_owned(),
        });
        return;
    };

    let has_result = value.get("result").is_some();
    let has_error = value.get("error").is_some();
    if has_result || has_error {
        let response_id = value.get("id").and_then(Value::as_u64);
        if response_id.is_none() || has_result == has_error || value.get("method").is_some() {
            events.publish(CodexEvent::ProtocolError {
                message: "app-server emitted a malformed response object".to_owned(),
            });
            return;
        }

        if let Some(sender) = pending
            .lock()
            .expect("pending request mutex poisoned")
            .remove(&response_id.expect("checked above"))
        {
            let response = value.get("result").cloned().ok_or(CodexError::Server);
            let _ = sender.send(response);
        }
        return;
    }

    if let Some(method) = value.get("method").and_then(Value::as_str) {
        let params = value.get("params").cloned().unwrap_or(Value::Null);
        let event = match value.get("id") {
            Some(id) => CodexEvent::ServerRequest {
                id: id.clone(),
                method: method.to_owned(),
                params,
            },
            None => CodexEvent::Notification {
                method: method.to_owned(),
                params,
            },
        };
        events.publish(event);
        return;
    }

    events.publish(CodexEvent::ProtocolError {
        message: "app-server emitted an unrecognized protocol message".to_owned(),
    });
}

async fn read_stderr(
    stderr: ChildStderr,
    ring: Arc<StdMutex<VecDeque<String>>>,
    redactor: SecretRedactor,
) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let mut ring = ring.lock().expect("stderr ring mutex poisoned");
        if ring.len() == STDERR_CAPACITY {
            ring.pop_front();
        }
        ring.push_back(redactor.redact(&line));
    }
}

async fn write_stdin(
    mut stdin: tokio::process::ChildStdin,
    mut commands: mpsc::Receiver<WriterCommand>,
    mut stop: watch::Receiver<bool>,
    process_control: mpsc::Sender<ProcessControl>,
) {
    loop {
        let command = tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    return;
                }
                continue;
            }
            command = commands.recv() => command,
        };
        let Some(command) = command else {
            return;
        };
        match command {
            WriterCommand::Frame { bytes, completion } => {
                let result = write_frame(&mut stdin, &bytes).await;
                let failed = result.is_err();
                let _ = completion.send(result);
                if failed {
                    let _ = process_control.try_send(ProcessControl::Kill {
                        completion_deadline: Instant::now() + SHUTDOWN_TERMINAL_TIMEOUT,
                    });
                    break;
                }
            }
            WriterCommand::Close { completion } => {
                drop(stdin);
                let _ = completion.send(Ok(()));
                return;
            }
        }
    }
}

async fn write_frame(
    stdin: &mut tokio::process::ChildStdin,
    bytes: &[u8],
) -> Result<(), CodexError> {
    timeout(WRITER_IO_TIMEOUT, stdin.write_all(bytes))
        .await
        .map_err(|_| CodexError::WriterTimedOut {
            operation: "write frame",
        })?
        .map_err(|error| CodexError::io("write frame", &error))?;
    timeout(WRITER_IO_TIMEOUT, stdin.flush())
        .await
        .map_err(|_| CodexError::WriterTimedOut {
            operation: "flush frame",
        })?
        .map_err(|error| CodexError::io("flush frame", &error))
}

#[allow(clippy::too_many_arguments)]
async fn supervise_process(
    mut child: tokio::process::Child,
    mut process_commands: mpsc::Receiver<ProcessControl>,
    writer_stop: watch::Sender<bool>,
    stdout_task: JoinHandle<()>,
    stderr_task: JoinHandle<()>,
    writer_task: JoinHandle<()>,
    pending: Pending,
    events: Events,
    exited: Arc<AtomicBool>,
    exit_tx: watch::Sender<Option<Option<i32>>>,
) {
    let child_exit = wait_for_child(&mut child, &mut process_commands).await;
    let code = child_exit.code;
    exited.store(true, Ordering::Release);
    let _ = writer_stop.send(true);

    finish_process_tasks(
        stdout_task,
        stderr_task,
        writer_task,
        child_exit.task_deadline,
        &events,
    )
    .await;

    let mut pending = pending.lock().expect("pending request mutex poisoned");
    for (_, sender) in pending.drain() {
        let _ = sender.send(Err(CodexError::ProcessExited { code }));
    }
    drop(pending);
    events.publish(CodexEvent::ProcessExited { code });
    let _ = exit_tx.send(Some(code));
}

async fn wait_for_child(
    child: &mut tokio::process::Child,
    process_commands: &mut mpsc::Receiver<ProcessControl>,
) -> ChildExit {
    loop {
        tokio::select! {
            biased;
            status = child.wait() => {
                if let Ok(status) = status {
                    return ChildExit {
                        code: status.code(),
                        task_deadline: Instant::now() + PROCESS_TASK_DRAIN_TIMEOUT,
                    };
                }
                tokio::task::yield_now().await;
            },
            command = process_commands.recv() => {
                let (completion_deadline, control_closed) = match command {
                    Some(ProcessControl::Kill { completion_deadline }) => (completion_deadline, false),
                    None => (Instant::now() + SHUTDOWN_TERMINAL_TIMEOUT, true),
                };
                let _ = child.start_kill();
                let kill_deadline = std::cmp::min(
                    Instant::now() + CHILD_KILL_TIMEOUT,
                    completion_deadline,
                );
                if let Ok(Ok(status)) = timeout_at(kill_deadline, child.wait()).await {
                    let supervisor_deadline = completion_deadline
                        .checked_sub(SHUTDOWN_COORDINATION_TIMEOUT)
                        .unwrap_or(completion_deadline);
                    return ChildExit {
                        code: status.code(),
                        task_deadline: std::cmp::min(
                            Instant::now() + PROCESS_TASK_DRAIN_TIMEOUT,
                            supervisor_deadline,
                        ),
                    };
                }
                if control_closed {
                    loop {
                        if let Ok(status) = child.wait().await {
                            return ChildExit {
                                code: status.code(),
                                task_deadline: Instant::now() + PROCESS_TASK_DRAIN_TIMEOUT,
                            };
                        }
                        tokio::task::yield_now().await;
                    }
                }
            }
        }
    }
}

struct ChildExit {
    code: Option<i32>,
    task_deadline: Instant,
}

async fn finish_process_tasks(
    stdout_task: JoinHandle<()>,
    stderr_task: JoinHandle<()>,
    writer_task: JoinHandle<()>,
    deadline: Instant,
    events: &Events,
) {
    let (stdout_error, stderr_error, writer_error) = tokio::join!(
        finish_task(
            stdout_task,
            deadline,
            "stdout reader did not finish after process exit"
        ),
        finish_task(
            stderr_task,
            deadline,
            "stderr reader did not finish after process exit"
        ),
        finish_task(
            writer_task,
            deadline,
            "stdin writer did not finish after process exit"
        ),
    );
    for message in [stdout_error, stderr_error, writer_error]
        .into_iter()
        .flatten()
    {
        events.publish(CodexEvent::ProtocolError {
            message: message.to_owned(),
        });
    }
}

async fn finish_task(
    mut task: JoinHandle<()>,
    deadline: Instant,
    timeout_message: &'static str,
) -> Option<&'static str> {
    match timeout_at(deadline, &mut task).await {
        Ok(Ok(())) => None,
        Ok(Err(_)) => Some("Codex process task stopped unexpectedly"),
        Err(_) => {
            task.abort();
            let _ = task.await;
            Some(timeout_message)
        }
    }
}

async fn wait_for_exit_until(
    exit: &mut watch::Receiver<Option<Option<i32>>>,
    deadline: Instant,
) -> bool {
    if exit.borrow().is_some() {
        return true;
    }
    matches!(timeout_at(deadline, exit.changed()).await, Ok(Ok(()))) && exit.borrow().is_some()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use tokio::sync::Barrier;

    use super::*;

    const TEST_WATCHDOG: Duration = Duration::from_secs(2);

    struct DropProbe(Arc<AtomicUsize>);

    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::AcqRel);
        }
    }

    fn parked_process_task(ready: Arc<Barrier>, dropped: Arc<AtomicUsize>) -> JoinHandle<()> {
        tokio::spawn(async move {
            let _probe = DropProbe(dropped);
            ready.wait().await;
            std::future::pending::<()>().await;
        })
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn overdue_process_tasks_are_aborted_and_joined_before_terminal_can_publish() {
        let ready = Arc::new(Barrier::new(4));
        let dropped = Arc::new(AtomicUsize::new(0));
        let stdout = parked_process_task(ready.clone(), dropped.clone());
        let stderr = parked_process_task(ready.clone(), dropped.clone());
        let writer = parked_process_task(ready.clone(), dropped.clone());
        timeout(TEST_WATCHDOG, ready.wait())
            .await
            .expect("watchdog expired while parking process tasks");
        let (live, _) = broadcast::channel(EVENT_CAPACITY);
        let events = Events::new(live);

        timeout(
            TEST_WATCHDOG,
            finish_process_tasks(stdout, stderr, writer, tokio::time::Instant::now(), &events),
        )
        .await
        .expect("watchdog expired while aborting and joining process tasks");
        assert_eq!(dropped.load(Ordering::Acquire), 3);

        events.publish(CodexEvent::ProcessExited { code: Some(47) });
        let mut saw_terminal = false;
        while let Some(event) = timeout(TEST_WATCHDOG, events.next())
            .await
            .expect("watchdog expired while draining finalization diagnostics")
        {
            if matches!(event, CodexEvent::ProcessExited { code: Some(47) }) {
                saw_terminal = true;
                break;
            }
        }
        assert!(saw_terminal);
        assert_eq!(
            timeout(TEST_WATCHDOG, events.next())
                .await
                .expect("watchdog expired while checking terminal state"),
            None
        );
    }

    #[test]
    fn shutdown_terminal_budget_covers_kill_parallel_drain_and_coordination() {
        assert_eq!(
            SHUTDOWN_TERMINAL_TIMEOUT,
            CHILD_KILL_TIMEOUT + PROCESS_TASK_DRAIN_TIMEOUT + SHUTDOWN_COORDINATION_TIMEOUT
        );
    }

    #[test]
    fn persistent_overflow_diagnostic_remains_merged_during_continuous_overflow() {
        let mut queue = VecDeque::new();
        for sequence in 0..(PERSISTENT_EVENT_CAPACITY * 3) {
            push_persistent_event(
                &mut queue,
                CodexEvent::Notification {
                    method: "test/progress".to_owned(),
                    params: json!({ "sequence": sequence }),
                },
                false,
            );
            if sequence >= PERSISTENT_EVENT_CAPACITY {
                assert_eq!(
                    queue
                        .iter()
                        .filter(|event| is_persistent_lagged_diagnostic(event))
                        .count(),
                    1,
                    "overflow diagnostic must remain merged at sequence {sequence}"
                );
            }
        }
    }
}

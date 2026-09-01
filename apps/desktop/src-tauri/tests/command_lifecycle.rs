use std::{
    collections::VecDeque,
    fs,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use arcgis_pro_agent_desktop_lib::{
    app_state::{
        AccountSnapshot, ActiveViewSnapshot, AppServerClient, BridgeSnapshot, BridgeStatus,
        DesktopState, LayerSnapshot, McpDiscoveryLease, ServerReply,
    },
    commands::{
        handle_mcp_status_notification_with, handle_server_request_with, handle_turn_completed,
        handle_turn_completed_event, health_refresh_with, refresh_mcp_status_with_timeout,
        run_after_event_consumer_start_with, turn_start_with,
    },
    mcp_status::{CURRENT_STATUS_METHOD, LEGACY_STATUS_METHOD, parse_arcgis_status_notification},
    providers::ProviderEvent,
};
use serde_json::{Value, json};
use tokio::sync::{Semaphore, mpsc, oneshot};

type ClientFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'a>>;

#[test]
fn runtime_recovery_source_uses_one_restart_guard_and_epoch_checked_exit_paths() {
    let state_source = include_str!("../src/app_state.rs");
    let commands_source = include_str!("../src/commands.rs");
    for required in [
        "runtime_restart: Mutex<()>",
        "mark_runtime_stopped_if_epoch",
        "pending_runtime_epoch",
    ] {
        assert!(
            state_source.contains(required),
            "missing state contract: {required}"
        );
    }
    for required in [
        "lock_runtime_restart().await",
        ".request(\"account/read\"",
        "run_health_poller(app, runtime_epoch)",
    ] {
        assert!(
            commands_source.contains(required),
            "missing restart contract: {required}"
        );
    }
    assert!(
        !commands_source
            .contains("state.poll_gate().cancel();\n                state.mark_runtime_stopped")
    );
}

struct BlockingClient {
    calls: Mutex<Vec<(String, Value)>>,
    observed: mpsc::UnboundedSender<()>,
    release: Semaphore,
    response: Value,
}

#[derive(Default)]
struct RecordingClient {
    replies: Mutex<Vec<(Value, ServerReply)>>,
}

struct ScriptedClient {
    calls: Mutex<Vec<(String, Value)>>,
    responses: Mutex<VecDeque<Result<Value, String>>>,
}

struct WorkspaceInspectingClient {
    calls: Mutex<Vec<(String, Value)>>,
    workspace_exists_at_thread_start: Mutex<Option<bool>>,
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "arcgis-pro-agent-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct HealthThenHangingClient {
    calls: Mutex<Vec<(String, Value)>>,
}

struct DropObservedClient {
    calls: Mutex<Vec<(String, Value)>>,
    observed: mpsc::UnboundedSender<()>,
    dropped: mpsc::UnboundedSender<()>,
}

struct DropSignal(mpsc::UnboundedSender<()>);

impl Drop for DropSignal {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

impl ScriptedClient {
    fn new(responses: impl IntoIterator<Item = Result<Value, String>>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            responses: Mutex::new(responses.into_iter().collect()),
        }
    }
}

impl AppServerClient for WorkspaceInspectingClient {
    fn request<'a>(&'a self, method: &'a str, params: Value) -> ClientFuture<'a, Value> {
        self.calls
            .lock()
            .expect("calls lock")
            .push((method.to_owned(), params.clone()));
        let response = match method {
            "thread/start" => {
                let exists = params["cwd"]
                    .as_str()
                    .map(Path::new)
                    .is_some_and(Path::is_dir);
                *self
                    .workspace_exists_at_thread_start
                    .lock()
                    .expect("workspace observation lock") = Some(exists);
                Ok(json!({"thread": {"id": "thread-command"}}))
            }
            "mcpServerStatus/list" => Ok(json!({"data": []})),
            _ => Err(format!("unexpected request: {method}")),
        };
        Box::pin(async move { response })
    }

    fn respond<'a>(&'a self, _id: Value, _reply: ServerReply) -> ClientFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}

impl AppServerClient for ScriptedClient {
    fn request<'a>(&'a self, method: &'a str, params: Value) -> ClientFuture<'a, Value> {
        self.calls
            .lock()
            .expect("calls lock")
            .push((method.to_owned(), params));
        let response = self
            .responses
            .lock()
            .expect("responses lock")
            .pop_front()
            .expect("scripted response");
        Box::pin(async move { response })
    }

    fn respond<'a>(&'a self, _id: Value, _reply: ServerReply) -> ClientFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}

impl AppServerClient for HealthThenHangingClient {
    fn request<'a>(&'a self, method: &'a str, params: Value) -> ClientFuture<'a, Value> {
        let call_index = {
            let mut calls = self.calls.lock().expect("calls lock");
            calls.push((method.to_owned(), params));
            calls.len()
        };
        Box::pin(async move {
            if call_index == 1 {
                Ok(json!({
                    "structuredContent": {
                        "connected": true,
                        "protocolVersion": "1.0",
                        "projectName": "timeout-project"
                    }
                }))
            } else {
                std::future::pending::<Result<Value, String>>().await
            }
        })
    }

    fn respond<'a>(&'a self, _id: Value, _reply: ServerReply) -> ClientFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}

impl AppServerClient for DropObservedClient {
    fn request<'a>(&'a self, method: &'a str, params: Value) -> ClientFuture<'a, Value> {
        self.calls
            .lock()
            .expect("calls lock")
            .push((method.to_owned(), params));
        let observed = self.observed.clone();
        let drop_signal = DropSignal(self.dropped.clone());
        Box::pin(async move {
            let _drop_signal = drop_signal;
            let _ = observed.send(());
            std::future::pending::<Result<Value, String>>().await
        })
    }

    fn respond<'a>(&'a self, _id: Value, _reply: ServerReply) -> ClientFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}

impl AppServerClient for RecordingClient {
    fn request<'a>(&'a self, _method: &'a str, _params: Value) -> ClientFuture<'a, Value> {
        Box::pin(async { Err("unexpected request".to_owned()) })
    }

    fn respond<'a>(&'a self, id: Value, reply: ServerReply) -> ClientFuture<'a, ()> {
        self.replies.lock().expect("replies lock").push((id, reply));
        Box::pin(async { Ok(()) })
    }
}

impl AppServerClient for BlockingClient {
    fn request<'a>(&'a self, method: &'a str, params: Value) -> ClientFuture<'a, Value> {
        self.calls
            .lock()
            .expect("calls lock")
            .push((method.to_owned(), params));
        let _ = self.observed.send(());
        Box::pin(async move {
            self.release
                .acquire()
                .await
                .expect("test release semaphore")
                .forget();
            Ok(self.response.clone())
        })
    }

    fn respond<'a>(&'a self, _id: Value, _reply: ServerReply) -> ClientFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}

fn signed_in() -> AccountSnapshot {
    AccountSnapshot::SignedIn {
        email: Some("map@example.com".to_owned()),
        plan_type: "plus".to_owned(),
    }
}

async fn arcgis_readiness_state(inventory_first: bool) -> (DesktopState, u64, McpDiscoveryLease) {
    let client = Arc::new(ScriptedClient::new(Vec::<Result<Value, String>>::new()));
    let state = DesktopState::new(PathBuf::from("test-data"));
    state.apply_account(signed_in()).await;
    let runtime_epoch = state.install_client(client).await;
    let discovery =
        establish_arcgis_readiness(&state, runtime_epoch, "thread-context", inventory_first).await;
    (state, runtime_epoch, discovery)
}

async fn establish_arcgis_readiness(
    state: &DesktopState,
    runtime_epoch: u64,
    thread_id: &str,
    inventory_first: bool,
) -> McpDiscoveryLease {
    let (conversation, _) = state.begin_conversation().await.unwrap();
    let discovery = state
        .commit_conversation(&conversation, thread_id.to_owned())
        .await
        .unwrap();
    let update = parse_arcgis_status_notification(
        CURRENT_STATUS_METHOD,
        &json!({"name": "arcgis", "status": "ready", "threadId": thread_id}),
    )
    .unwrap();
    if inventory_first {
        assert!(state.apply_arcgis_inventory(&discovery, true).await);
        assert!(state.apply_arcgis_status(runtime_epoch, update).await);
    } else {
        assert!(state.apply_arcgis_status(runtime_epoch, update).await);
        assert!(state.apply_arcgis_inventory(&discovery, true).await);
    }
    assert!(state.arcgis_ready().await);
    discovery
}

#[tokio::test]
async fn arcgis_readiness_accepts_evidence_in_either_order_and_duplicates_are_idempotent() {
    for inventory_first in [false, true] {
        let (state, runtime_epoch, discovery) = arcgis_readiness_state(inventory_first).await;
        state.set_visible(true).await;
        let (health, _) = state.health_lease().await.expect("ready health lease");
        let update = parse_arcgis_status_notification(
            CURRENT_STATUS_METHOD,
            &json!({"name": "arcgis", "status": "ready", "threadId": "thread-context"}),
        )
        .unwrap();
        assert!(!state.apply_arcgis_status(runtime_epoch, update).await);
        assert!(!state.apply_arcgis_inventory(&discovery, true).await);
        assert!(state.health_lease_is_current(&health).await);
    }
}

#[tokio::test]
async fn arcgis_readiness_rejects_wrong_runtime_epoch_and_thread() {
    let (state, runtime_epoch, discovery) = arcgis_readiness_state(false).await;
    let ready = |thread_id: &str| {
        parse_arcgis_status_notification(
            CURRENT_STATUS_METHOD,
            &json!({"name": "arcgis", "status": "ready", "threadId": thread_id}),
        )
        .unwrap()
    };
    assert!(discovery.runtime_epoch() == runtime_epoch);
    assert_eq!(discovery.thread_id(), "thread-context");
    assert!(
        !state
            .apply_arcgis_status(runtime_epoch.wrapping_add(1), ready("thread-context"))
            .await
    );
    assert!(
        !state
            .apply_arcgis_status(runtime_epoch, ready("thread-other"))
            .await
    );
    assert!(state.arcgis_ready().await);
}

#[tokio::test]
async fn arcgis_readiness_global_non_ready_revokes_but_global_ready_is_rejected() {
    let (state, runtime_epoch, discovery) = arcgis_readiness_state(false).await;
    let global = |status: Option<&str>, thread_id: Value| {
        let mut params = json!({"name": "arcgis", "threadId": thread_id});
        if let Some(status) = status {
            params["status"] = json!(status);
        }
        parse_arcgis_status_notification(CURRENT_STATUS_METHOD, &params).unwrap()
    };
    assert!(
        !state
            .apply_arcgis_status(runtime_epoch, global(Some("ready"), Value::Null))
            .await
    );
    assert!(state.arcgis_ready().await);

    for update in [
        global(Some("starting"), Value::Null),
        global(Some("failed"), Value::Null),
        global(Some("cancelled"), Value::Null),
        global(None, Value::Null),
        global(Some("ready"), json!(42)),
    ] {
        assert!(state.apply_arcgis_status(runtime_epoch, update).await);
        assert!(!state.arcgis_ready().await);
        let ready = parse_arcgis_status_notification(
            CURRENT_STATUS_METHOD,
            &json!({"name": "arcgis", "status": "ready", "threadId": "thread-context"}),
        )
        .unwrap();
        assert!(state.apply_arcgis_status(runtime_epoch, ready).await);
        assert!(!state.apply_arcgis_inventory(&discovery, true).await);
        assert!(state.arcgis_ready().await);
    }
}

#[tokio::test]
async fn arcgis_readiness_resets_on_runtime_account_and_conversation_changes() {
    let (runtime_state, _, runtime_discovery) = arcgis_readiness_state(false).await;
    runtime_state.mark_runtime_stopped().await;
    assert!(!runtime_state.arcgis_ready().await);
    assert!(
        !runtime_state
            .apply_arcgis_inventory(&runtime_discovery, true)
            .await
    );

    let (account_state, _, account_discovery) = arcgis_readiness_state(false).await;
    account_state
        .apply_account(AccountSnapshot::SignedIn {
            email: Some("other@example.com".to_owned()),
            plan_type: "team".to_owned(),
        })
        .await;
    assert!(!account_state.arcgis_ready().await);
    assert!(
        !account_state
            .apply_arcgis_inventory(&account_discovery, true)
            .await
    );

    let (conversation_state, _, old_discovery) = arcgis_readiness_state(false).await;
    let (conversation, _) = conversation_state.begin_conversation().await.unwrap();
    let replacement = conversation_state
        .commit_conversation(&conversation, "thread-replacement".to_owned())
        .await
        .unwrap();
    assert_eq!(replacement.thread_id(), "thread-replacement");
    assert!(!conversation_state.arcgis_ready().await);
    assert!(
        !conversation_state
            .apply_arcgis_inventory(&old_discovery, true)
            .await
    );
}

#[tokio::test]
async fn stale_conversation_start_is_released_when_client_rotates() {
    let client_a: Arc<dyn AppServerClient> =
        Arc::new(ScriptedClient::new(Vec::<Result<Value, String>>::new()));
    let client_b: Arc<dyn AppServerClient> =
        Arc::new(ScriptedClient::new(Vec::<Result<Value, String>>::new()));
    let state = DesktopState::new(PathBuf::from("test-data"));
    state.apply_account(signed_in()).await;
    state.install_client(client_a).await;

    let (stale, _) = state.begin_conversation().await.unwrap();
    state.install_client(client_b.clone()).await;
    assert!(
        state
            .commit_conversation(&stale, "thread-stale".to_owned())
            .await
            .is_err()
    );
    state.abort_conversation(&stale).await;

    let (_, current_client) = state
        .begin_conversation()
        .await
        .expect("client rotation must release the stale conversation-start latch");
    assert!(Arc::ptr_eq(&current_client, &client_b));
}

#[tokio::test]
async fn concurrent_turn_start_is_serialized_and_old_completion_cannot_clear_new_turn() {
    let (observed_tx, mut observed_rx) = mpsc::unbounded_channel();
    let client = Arc::new(BlockingClient {
        calls: Mutex::new(Vec::new()),
        observed: observed_tx,
        release: Semaphore::new(0),
        response: json!({"turn": {"id": "turn-new"}}),
    });
    let state = Arc::new(DesktopState::new(PathBuf::from("test-data")));
    state.install_client(client.clone()).await;
    state.apply_account(signed_in()).await;
    state.set_active_thread("thread-new".to_owned()).await;

    let first_state = state.clone();
    let first =
        tokio::spawn(async move { turn_start_with(&first_state, "first".to_owned()).await });
    tokio::time::timeout(Duration::from_secs(1), observed_rx.recv())
        .await
        .expect("first turn request must start")
        .expect("request observation channel");

    let second = turn_start_with(&state, "second".to_owned()).await;
    assert!(second.is_err(), "a second concurrent turn must be rejected");
    assert_eq!(client.calls.lock().expect("calls lock").len(), 1);

    client.release.add_permits(1);
    let started = tokio::time::timeout(Duration::from_secs(1), first)
        .await
        .expect("first turn watchdog")
        .expect("first task")
        .expect("first turn result");
    assert_eq!(started.turn_id, "turn-new");

    assert!(
        !handle_turn_completed(
            &state,
            json!({
                "threadId": "thread-old",
                "turn": {"id": "turn-new"}
            })
        )
        .await
    );
    assert_eq!(state.active_ids().await.1.as_deref(), Some("turn-new"));

    assert!(
        handle_turn_completed(
            &state,
            json!({
                "threadId": "thread-new",
                "turn": {"id": "turn-new"}
            })
        )
        .await
    );
    assert_eq!(state.active_ids().await.1, None);
}

#[tokio::test]
async fn nested_turn_completion_produces_the_neutral_provider_event() {
    let state = DesktopState::new(PathBuf::from("test-data"));
    state.set_active_thread("thread-nested".to_owned()).await;
    state.set_active_turn("turn-nested".to_owned()).await;

    assert_eq!(
        handle_turn_completed_event(
            &state,
            json!({
                "threadId": "thread-nested",
                "turn": {"id": "turn-nested"}
            })
        )
        .await,
        Some(ProviderEvent::TurnCompleted {
            turn_id: "turn-nested".to_owned()
        })
    );
    assert_eq!(state.active_ids().await.1, None);
}

#[tokio::test]
async fn every_non_signed_in_account_invalidates_session_and_turn_backend_revalidates() {
    let (observed_tx, mut observed_rx) = mpsc::unbounded_channel();
    let client = Arc::new(BlockingClient {
        calls: Mutex::new(Vec::new()),
        observed: observed_tx,
        release: Semaphore::new(0),
        response: json!({"turn": {"id": "turn-new"}}),
    });
    let state = DesktopState::new(PathBuf::from("test-data"));
    let runtime_epoch = state.install_client(client).await;
    state.apply_account(signed_in()).await;
    establish_arcgis_readiness(&state, runtime_epoch, "thread-1", false).await;
    let generation = state.session_generation().await;

    for account in [
        AccountSnapshot::SignedOut,
        AccountSnapshot::UnsupportedAuth,
        AccountSnapshot::LoginPending {
            login_id: "login-2".to_owned(),
        },
    ] {
        state.apply_account(account).await;
        assert_eq!(state.active_ids().await, (None, None));
        assert!(!state.arcgis_ready().await);
        assert!(state.session_generation().await > generation);
        assert!(
            turn_start_with(&state, "must fail".to_owned())
                .await
                .is_err()
        );
    }

    assert!(
        tokio::time::timeout(Duration::from_millis(50), observed_rx.recv())
            .await
            .is_err(),
        "signed-out turn validation must happen before any backend request"
    );
}

#[tokio::test]
async fn changed_signed_in_account_invalidates_generation_conversation_and_readiness() {
    let state = DesktopState::new(PathBuf::from("test-data"));
    let client = Arc::new(ScriptedClient::new(Vec::<Result<Value, String>>::new()));
    let runtime_epoch = state.install_client(client).await;
    state.apply_account(signed_in()).await;
    establish_arcgis_readiness(&state, runtime_epoch, "thread-old-account", false).await;
    state.set_active_turn("turn-old-account".to_owned()).await;
    let generation = state.session_generation().await;

    state
        .apply_account(AccountSnapshot::SignedIn {
            email: Some("other@example.com".to_owned()),
            plan_type: "team".to_owned(),
        })
        .await;

    assert!(state.session_generation().await > generation);
    assert_eq!(state.active_ids().await, (None, None));
    assert!(!state.arcgis_ready().await);
    let arcgis = serde_json::to_string(&state.snapshot().await.arcgis).unwrap();
    assert!(!arcgis.contains("map@example.com"));
    assert!(!arcgis.contains("other@example.com"));
}

#[derive(Clone, Copy)]
enum InvalidatingChange {
    Logout,
    ThreadSwitch,
    Hidden,
    CodexExit,
}

async fn assert_stale_health_is_dropped(change: InvalidatingChange) {
    let (observed_tx, mut observed_rx) = mpsc::unbounded_channel();
    let client = Arc::new(BlockingClient {
        calls: Mutex::new(Vec::new()),
        observed: observed_tx,
        release: Semaphore::new(0),
        response: json!({
            "structuredContent": {"connected": true, "projectName": "stale-project"}
        }),
    });
    let state = Arc::new(DesktopState::new(PathBuf::from("test-data")));
    let runtime_epoch = state.install_client(client.clone()).await;
    state.apply_account(signed_in()).await;
    establish_arcgis_readiness(&state, runtime_epoch, "thread-health", false).await;
    state.set_visible(true).await;

    let refresh_state = state.clone();
    let refresh =
        tokio::spawn(
            async move { health_refresh_with(&refresh_state, "2026-07-19T12:00:00Z").await },
        );
    tokio::time::timeout(Duration::from_secs(1), observed_rx.recv())
        .await
        .expect("health request must start")
        .expect("health observation channel");

    match change {
        InvalidatingChange::Logout => {
            state.apply_account(AccountSnapshot::SignedOut).await;
        }
        InvalidatingChange::ThreadSwitch => {
            establish_arcgis_readiness(&state, runtime_epoch, "thread-new", false).await;
        }
        InvalidatingChange::Hidden => state.set_visible(false).await,
        InvalidatingChange::CodexExit => {
            state.mark_runtime_stopped().await;
        }
    }

    client.release.add_permits(1);
    let committed = tokio::time::timeout(Duration::from_secs(1), refresh)
        .await
        .expect("health refresh watchdog")
        .expect("health refresh task")
        .expect("health refresh result");
    assert!(!committed, "stale health response must be dropped");
    assert_ne!(
        state.snapshot().await.arcgis.project_name.as_deref(),
        Some("stale-project")
    );
}

#[tokio::test]
async fn health_lease_drops_logout_thread_switch_hidden_and_codex_exit_responses() {
    for change in [
        InvalidatingChange::Logout,
        InvalidatingChange::ThreadSwitch,
        InvalidatingChange::Hidden,
        InvalidatingChange::CodexExit,
    ] {
        assert_stale_health_is_dropped(change).await;
    }
}

#[tokio::test]
async fn health_lease_drops_response_when_arcgis_readiness_is_revoked_while_awaited() {
    let (observed_tx, mut observed_rx) = mpsc::unbounded_channel();
    let client = Arc::new(BlockingClient {
        calls: Mutex::new(Vec::new()),
        observed: observed_tx,
        release: Semaphore::new(0),
        response: json!({
            "structuredContent": {"connected": true, "projectName": "late-project"}
        }),
    });
    let (ready_state, runtime_epoch) = ready_poll_state_with_epoch(client.clone()).await;
    let state = Arc::new(ready_state);

    let refresh_state = state.clone();
    let refresh =
        tokio::spawn(
            async move { health_refresh_with(&refresh_state, "2026-07-29T01:02:03Z").await },
        );
    tokio::time::timeout(Duration::from_secs(1), observed_rx.recv())
        .await
        .expect("health request must start")
        .expect("health observation channel");

    let update = parse_arcgis_status_notification(
        CURRENT_STATUS_METHOD,
        &json!({"name": "arcgis", "status": "starting", "threadId": null}),
    )
    .unwrap();
    assert!(state.apply_arcgis_status(runtime_epoch, update).await);
    client.release.add_permits(1);

    let committed = tokio::time::timeout(Duration::from_secs(1), refresh)
        .await
        .expect("health refresh watchdog")
        .expect("health refresh task")
        .expect("health refresh result");
    assert!(!committed, "late health response must not commit");
    assert_ne!(
        state.snapshot().await.arcgis.project_name.as_deref(),
        Some("late-project")
    );
}

#[tokio::test]
async fn server_requests_receive_exact_safe_responses_without_hanging() {
    let client = RecordingClient::default();
    let event = tokio::time::timeout(
        Duration::from_secs(1),
        handle_server_request_with(
            &client,
            json!(77),
            "mcpServer/elicitation/request",
            json!({
                "serverName": "arcgis",
                "threadId": "thread-1",
                "mode": "form",
                "message": "<img src=x onerror=steal()>",
                "requestedSchema": {"type": "object", "description": "<script>steal()</script>"}
            }),
        ),
    )
    .await
    .expect("elicitation response watchdog")
    .expect("elicitation handling")
    .expect("safe ArcGIS event");

    assert_eq!(event.request_id, json!(77));
    assert_eq!(event.server_name, "arcgis");
    assert_eq!(event.thread_id, "thread-1");
    assert_eq!(event.message, "<img src=x onerror=steal()>");
    assert_eq!(event.outcome, "declined");
    assert_eq!(
        client.replies.lock().expect("replies lock").as_slice(),
        &[(json!(77), ServerReply::Result(json!({"action": "decline"})))]
    );
    let serialized = serde_json::to_string(&event).expect("serialize safe event");
    assert!(!serialized.contains("requestedSchema"));
    assert!(!serialized.contains("<script>"));

    let rejected = handle_server_request_with(
        &client,
        json!(78),
        "permissions/request",
        json!({"serverName": "arcgis"}),
    )
    .await
    .expect("unsupported response");
    assert!(rejected.is_none());
    assert_eq!(
        client.replies.lock().expect("replies lock")[1],
        (
            json!(78),
            ServerReply::Error {
                code: -32601,
                message: "Unsupported server request".to_owned(),
            }
        )
    );
}

#[tokio::test]
async fn account_commands_use_the_installed_client_and_reset_login_state_atomically() {
    use arcgis_pro_agent_desktop_lib::commands::{
        chatgpt_login_cancel_with, chatgpt_login_start_with, chatgpt_logout_with,
        refresh_account_with,
    };

    let client = Arc::new(ScriptedClient::new([
        Ok(json!({
            "type": "chatgpt",
            "loginId": "login-1",
            "authUrl": "https://auth.openai.com/oauth/authorize",
            "accessToken": "must-not-leak"
        })),
        Ok(json!({
            "account": {"type": "chatgpt", "email": "map@example.com", "planType": "plus"}
        })),
        Ok(json!({})),
    ]));
    let state = DesktopState::new(PathBuf::from("test-data"));
    state.install_client(client.clone()).await;

    let login = chatgpt_login_start_with(&state).await.expect("start login");
    assert_eq!(login.login_id, "login-1");
    assert!(matches!(
        state.snapshot().await.account,
        AccountSnapshot::LoginPending { .. }
    ));

    chatgpt_login_cancel_with(&state).await;
    assert_eq!(state.snapshot().await.account, AccountSnapshot::SignedOut);

    let account = refresh_account_with(&state).await.expect("refresh account");
    assert_eq!(account.account, signed_in());
    state.apply_account(signed_in()).await;
    state
        .set_active_thread("thread-before-logout".to_owned())
        .await;

    chatgpt_logout_with(&state).await.expect("logout");
    assert_eq!(state.snapshot().await.account, AccountSnapshot::SignedOut);
    assert_eq!(state.active_ids().await, (None, None));

    let calls = client.calls.lock().expect("calls lock");
    assert_eq!(
        calls[0],
        (
            "account/login/start".to_owned(),
            json!({
                "type": "chatgpt",
                "codexStreamlinedLogin": true,
                "useHostedLoginSuccessPage": true,
                "appBrand": "codex"
            })
        )
    );
    assert_eq!(
        calls[1],
        ("account/read".to_owned(), json!({"refreshToken": false}))
    );
    assert_eq!(calls[2], ("account/logout".to_owned(), json!({})));
}

#[tokio::test]
async fn conversation_command_seam_sends_the_normalized_thread_request() {
    use arcgis_pro_agent_desktop_lib::commands::conversation_start_with;

    let test_directory = TestDirectory::new("normalized-thread-request");
    let expected_workspace = test_directory
        .path()
        .join("ArcGISProAgent")
        .join("workspace")
        .to_string_lossy()
        .into_owned();
    let client = Arc::new(ScriptedClient::new([
        Ok(json!({"thread": {"id": "thread-command"}})),
        Ok(json!({"data": [{"name": "arcgis", "tools": {
            "arcgis_connection_status": {},
            "arcgis_describe_context": {},
            "arcgis_list_layers": {}
        }}]})),
    ]));
    let state = DesktopState::new(test_directory.path().to_owned());
    state.install_client(client.clone()).await;
    state.apply_account(signed_in()).await;

    let result = conversation_start_with(&state)
        .await
        .expect("start conversation");
    assert_eq!(result.thread_id, "thread-command");
    assert_eq!(
        state.active_ids().await.0.as_deref(),
        Some("thread-command")
    );

    let calls = client.calls.lock().expect("calls lock");
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].0, "thread/start");
    assert_eq!(calls[0].1["sandbox"], "read-only");
    assert_eq!(calls[0].1["approvalPolicy"], "never");
    assert_eq!(calls[0].1["cwd"], expected_workspace);
    assert_eq!(
        calls[1],
        (
            "mcpServerStatus/list".to_owned(),
            json!({"threadId": "thread-command", "detail": "toolsAndAuthOnly"})
        )
    );
    assert!(!state.arcgis_ready().await);
}

#[tokio::test]
async fn conversation_command_creates_private_workspace_before_thread_start() {
    use arcgis_pro_agent_desktop_lib::commands::conversation_start_with;

    let test_directory = TestDirectory::new("workspace-creation");
    let workspace = test_directory
        .path()
        .join("ArcGISProAgent")
        .join("workspace");
    assert!(!workspace.exists());

    let client = Arc::new(WorkspaceInspectingClient {
        calls: Mutex::new(Vec::new()),
        workspace_exists_at_thread_start: Mutex::new(None),
    });
    let state = DesktopState::new(test_directory.path().to_owned());
    state.install_client(client.clone()).await;
    state.apply_account(signed_in()).await;

    conversation_start_with(&state)
        .await
        .expect("start conversation");

    assert_eq!(
        *client
            .workspace_exists_at_thread_start
            .lock()
            .expect("workspace observation lock"),
        Some(true)
    );
}

#[tokio::test]
async fn conversation_command_reuses_an_existing_private_workspace() {
    use arcgis_pro_agent_desktop_lib::commands::conversation_start_with;

    let test_directory = TestDirectory::new("workspace-idempotence");
    let workspace = test_directory
        .path()
        .join("ArcGISProAgent")
        .join("workspace");
    fs::create_dir_all(&workspace).expect("pre-create private workspace");

    let client = Arc::new(WorkspaceInspectingClient {
        calls: Mutex::new(Vec::new()),
        workspace_exists_at_thread_start: Mutex::new(None),
    });
    let state = DesktopState::new(test_directory.path().to_owned());
    state.install_client(client.clone()).await;
    state.apply_account(signed_in()).await;

    conversation_start_with(&state)
        .await
        .expect("start conversation with an existing workspace");

    assert_eq!(
        *client
            .workspace_exists_at_thread_start
            .lock()
            .expect("workspace observation lock"),
        Some(true)
    );
}

#[tokio::test]
async fn conversation_command_workspace_failure_is_safe_and_does_not_latch() {
    use arcgis_pro_agent_desktop_lib::commands::conversation_start_with;

    let test_directory = TestDirectory::new("workspace-failure");
    let application_directory = test_directory.path().join("ArcGISProAgent");
    fs::write(&application_directory, b"blocks workspace directory")
        .expect("create workspace blocker");

    let client = Arc::new(WorkspaceInspectingClient {
        calls: Mutex::new(Vec::new()),
        workspace_exists_at_thread_start: Mutex::new(None),
    });
    let state = DesktopState::new(test_directory.path().to_owned());
    state.install_client(client.clone()).await;
    state.apply_account(signed_in()).await;
    let snapshot_before_failure = state.snapshot().await;

    let error = conversation_start_with(&state)
        .await
        .expect_err("workspace creation must fail locally");

    assert_eq!(error, "Unable to prepare ArcGIS conversation");
    assert!(client.calls.lock().expect("calls lock").is_empty());
    assert_eq!(state.active_ids().await, (None, None));
    assert_eq!(state.snapshot().await, snapshot_before_failure);

    fs::remove_file(&application_directory).expect("remove workspace blocker");
    conversation_start_with(&state)
        .await
        .expect("workspace failure must not latch conversation startup");
    assert_eq!(
        state.active_ids().await.0.as_deref(),
        Some("thread-command")
    );
}

#[tokio::test]
async fn conversation_command_keeps_the_thread_when_status_discovery_fails() {
    use arcgis_pro_agent_desktop_lib::commands::conversation_start_with;

    let client = Arc::new(ScriptedClient::new([
        Ok(json!({"thread": {"id": "thread-command"}})),
        Err("status unavailable".to_owned()),
    ]));
    let state = DesktopState::new(PathBuf::from("test-data"));
    state.install_client(client.clone()).await;
    state.apply_account(signed_in()).await;

    let result = conversation_start_with(&state)
        .await
        .expect("status discovery must not invalidate a conversation");

    assert_eq!(result.thread_id, "thread-command");
    assert_eq!(
        state.active_ids().await.0.as_deref(),
        Some("thread-command")
    );
    assert!(!state.arcgis_ready().await);
    let calls = client.calls.lock().expect("calls lock");
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1].0, "mcpServerStatus/list");
}

#[tokio::test]
async fn event_consumer_start_gate_blocks_account_follow_up_until_rendezvous() {
    let (started_tx, started_rx) = oneshot::channel();
    let (account_tx, mut account_rx) = mpsc::unbounded_channel();
    let gated = tokio::spawn(run_after_event_consumer_start_with(
        async move { started_rx.await.unwrap_or(false) },
        async move {
            account_tx.send("refresh_account").unwrap();
        },
    ));

    assert!(
        tokio::time::timeout(Duration::from_millis(50), account_rx.recv())
            .await
            .is_err(),
        "account follow-up must remain blocked before the consumer rendezvous"
    );
    started_tx.send(true).unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), account_rx.recv())
            .await
            .expect("account follow-up watchdog"),
        Some("refresh_account")
    );
    assert!(gated.await.expect("gated startup task"));
}

#[tokio::test]
async fn event_consumer_start_failure_does_not_run_account_follow_up() {
    let (account_tx, mut account_rx) = mpsc::unbounded_channel();
    assert!(
        !run_after_event_consumer_start_with(async { false }, async move {
            account_tx.send("must-not-run").unwrap();
        })
        .await
    );
    assert!(account_rx.try_recv().is_err());
}

#[tokio::test]
async fn mcp_status_command_routes_current_and_legacy_events_through_one_seam() {
    for method in [CURRENT_STATUS_METHOD, LEGACY_STATUS_METHOD] {
        let client = Arc::new(ScriptedClient::new(Vec::<Result<Value, String>>::new()));
        let state = DesktopState::new(PathBuf::from("test-data"));
        let runtime_epoch = state.install_client(client).await;
        state.apply_account(signed_in()).await;
        let (conversation, _) = state.begin_conversation().await.unwrap();
        let discovery = state
            .commit_conversation(&conversation, "thread-status".to_owned())
            .await
            .unwrap();
        assert!(state.apply_arcgis_inventory(&discovery, true).await);

        assert!(
            handle_mcp_status_notification_with(
                &state,
                runtime_epoch,
                method,
                json!({"name": "arcgis", "status": "ready", "threadId": "thread-status"}),
            )
            .await
        );
        assert!(state.arcgis_ready().await, "method {method}");
    }
}

#[tokio::test]
async fn mcp_status_command_rejects_unrelated_or_stale_events_and_fails_closed() {
    let client = Arc::new(ScriptedClient::new(Vec::<Result<Value, String>>::new()));
    let state = DesktopState::new(PathBuf::from("test-data"));
    let runtime_epoch = state.install_client(client).await;
    state.apply_account(signed_in()).await;
    let (conversation, _) = state.begin_conversation().await.unwrap();
    let discovery = state
        .commit_conversation(&conversation, "thread-status".to_owned())
        .await
        .unwrap();
    assert!(state.apply_arcgis_inventory(&discovery, true).await);

    assert!(
        !handle_mcp_status_notification_with(
            &state,
            runtime_epoch,
            CURRENT_STATUS_METHOD,
            json!({"name": "other", "status": "ready", "threadId": "thread-status"}),
        )
        .await
    );
    assert!(
        !handle_mcp_status_notification_with(
            &state,
            runtime_epoch,
            CURRENT_STATUS_METHOD,
            json!({"name": "arcgis", "status": "ready", "threadId": "thread-other"}),
        )
        .await
    );
    assert!(
        !handle_mcp_status_notification_with(
            &state,
            runtime_epoch.wrapping_add(1),
            CURRENT_STATUS_METHOD,
            json!({"name": "arcgis", "status": "ready", "threadId": "thread-status"}),
        )
        .await
    );
    assert!(
        !handle_mcp_status_notification_with(
            &state,
            runtime_epoch,
            "mcpServer/futureStatus/updated",
            json!({"name": "arcgis", "status": "ready", "threadId": "thread-status"}),
        )
        .await
    );
    assert!(
        handle_mcp_status_notification_with(
            &state,
            runtime_epoch,
            CURRENT_STATUS_METHOD,
            json!({"name": "arcgis", "status": "future-state", "threadId": "thread-status"}),
        )
        .await
    );
    assert!(!state.arcgis_ready().await);
}

#[tokio::test]
async fn mcp_status_discovery_timeout_cancels_future_and_preserves_conversation() {
    let (observed_tx, mut observed_rx) = mpsc::unbounded_channel();
    let (dropped_tx, mut dropped_rx) = mpsc::unbounded_channel();
    let client = Arc::new(DropObservedClient {
        calls: Mutex::new(Vec::new()),
        observed: observed_tx,
        dropped: dropped_tx,
    });
    let state = Arc::new(DesktopState::new(PathBuf::from("test-data")));
    state.install_client(client.clone()).await;
    state.apply_account(signed_in()).await;
    let (conversation, _) = state.begin_conversation().await.unwrap();
    let discovery = state
        .commit_conversation(&conversation, "thread-timeout".to_owned())
        .await
        .unwrap();

    let refresh_state = state.clone();
    let refresh_client = client.clone();
    let refresh = tokio::spawn(async move {
        refresh_mcp_status_with_timeout(
            &refresh_state,
            refresh_client.as_ref(),
            &discovery,
            Duration::from_millis(25),
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(1), observed_rx.recv())
        .await
        .expect("status discovery must start")
        .expect("request observation channel");
    assert_eq!(
        state.active_ids().await.0.as_deref(),
        Some("thread-timeout")
    );
    assert!(!state.arcgis_ready().await);

    assert!(
        !tokio::time::timeout(Duration::from_secs(1), refresh)
            .await
            .expect("short status timeout watchdog")
            .expect("status refresh task")
    );
    tokio::time::timeout(Duration::from_secs(1), dropped_rx.recv())
        .await
        .expect("timed out request future must be dropped")
        .expect("future drop observation channel");
    assert_eq!(
        state.active_ids().await.0.as_deref(),
        Some("thread-timeout")
    );
    assert!(!state.arcgis_ready().await);
    assert_eq!(
        client.calls.lock().expect("calls lock")[0],
        (
            "mcpServerStatus/list".to_owned(),
            json!({"threadId": "thread-timeout", "detail": "toolsAndAuthOnly"})
        )
    );
}

#[derive(Clone, Copy)]
enum DiscoveryInvalidation {
    Runtime,
    Thread,
}

#[tokio::test]
async fn mcp_status_discovery_rejects_late_inventory_after_runtime_or_thread_change() {
    for invalidation in [
        DiscoveryInvalidation::Runtime,
        DiscoveryInvalidation::Thread,
    ] {
        let (observed_tx, mut observed_rx) = mpsc::unbounded_channel();
        let client = Arc::new(BlockingClient {
            calls: Mutex::new(Vec::new()),
            observed: observed_tx,
            release: Semaphore::new(0),
            response: json!({"data": [{"name": "arcgis", "tools": {
                "arcgis_connection_status": {},
                "arcgis_describe_context": {},
                "arcgis_list_layers": {}
            }}]}),
        });
        let state = Arc::new(DesktopState::new(PathBuf::from("test-data")));
        state.install_client(client.clone()).await;
        state.apply_account(signed_in()).await;
        let (conversation, _) = state.begin_conversation().await.unwrap();
        let discovery = state
            .commit_conversation(&conversation, "thread-late".to_owned())
            .await
            .unwrap();

        let refresh_state = state.clone();
        let refresh_client = client.clone();
        let refresh = tokio::spawn(async move {
            refresh_mcp_status_with_timeout(
                &refresh_state,
                refresh_client.as_ref(),
                &discovery,
                Duration::from_secs(1),
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(1), observed_rx.recv())
            .await
            .expect("status discovery must start")
            .expect("request observation channel");

        match invalidation {
            DiscoveryInvalidation::Runtime => {
                state
                    .install_client(Arc::new(ScriptedClient::new(
                        Vec::<Result<Value, String>>::new(),
                    )))
                    .await;
            }
            DiscoveryInvalidation::Thread => {
                state.set_active_thread("thread-new".to_owned()).await;
            }
        }
        client.release.add_permits(1);

        assert!(
            !tokio::time::timeout(Duration::from_secs(1), refresh)
                .await
                .expect("late status response watchdog")
                .expect("status refresh task")
        );
        assert!(!state.arcgis_ready().await);
    }
}

async fn ready_poll_state(client: Arc<dyn AppServerClient>) -> DesktopState {
    ready_poll_state_with_epoch(client).await.0
}

async fn ready_poll_state_with_epoch(client: Arc<dyn AppServerClient>) -> (DesktopState, u64) {
    let state = DesktopState::new(PathBuf::from("test-data"));
    let runtime_epoch = state.install_client(client).await;
    state.apply_account(signed_in()).await;
    establish_arcgis_readiness(&state, runtime_epoch, "thread-context", false).await;
    state.set_visible(true).await;
    (state, runtime_epoch)
}

#[tokio::test]
async fn no_project_publishes_a_current_empty_context_without_project_calls() {
    let client = Arc::new(ScriptedClient::new([Ok(json!({
        "structuredContent": {
            "connected": true,
            "protocolVersion": "2.0",
            "projectName": null,
            "email": "tool-result@example.com",
            "apiToken": "sk-proj-must-not-leak"
        }
    }))]));
    let state = ready_poll_state(client.clone()).await;

    assert!(
        health_refresh_with(&state, "2026-07-29T01:02:03Z")
            .await
            .expect("refresh without project")
    );
    let snapshot = state.snapshot().await.arcgis;
    assert_eq!(snapshot.status, BridgeStatus::Connected);
    assert!(snapshot.context_is_live);
    assert_eq!(snapshot.project_name, None);
    assert_eq!(snapshot.active_view, None);
    assert!(snapshot.layers.is_empty());
    assert_eq!(client.calls.lock().expect("calls lock").len(), 1);
    let serialized = serde_json::to_string(&snapshot).expect("serialize current context");
    assert!(!serialized.contains("tool-result@example.com"));
    assert!(!serialized.contains("sk-proj"));
}

#[tokio::test]
async fn layout_context_skips_layer_listing_but_remains_current() {
    let client = Arc::new(ScriptedClient::new([
        Ok(json!({"structuredContent": {"connected": true, "projectName": "布局工程"}})),
        Ok(json!({"structuredContent": {
            "project": {"name": "布局工程", "path": r"C:\private\layout.aprx", "hasUnsavedChanges": false, "items": []},
            "activeView": {"uri": "layout://print", "name": "打印布局", "kind": "layout", "extent": null}
        }})),
    ]));
    let state = ready_poll_state(client.clone()).await;

    assert!(
        health_refresh_with(&state, "2026-07-29T01:02:03Z")
            .await
            .expect("layout refresh")
    );
    let snapshot = state.snapshot().await.arcgis;
    assert!(snapshot.context_is_live);
    assert_eq!(snapshot.project_name.as_deref(), Some("布局工程"));
    assert_eq!(
        snapshot.active_view.as_ref().map(|view| view.kind.as_str()),
        Some("layout")
    );
    assert!(snapshot.layers.is_empty());
    let calls = client.calls.lock().expect("calls lock");
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1].1["tool"], "arcgis_describe_context");
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains("layout.aprx")
    );
}

fn retained_snapshot() -> BridgeSnapshot {
    BridgeSnapshot {
        status: BridgeStatus::Connected,
        context_is_live: true,
        protocol_version: Some("old-health".to_owned()),
        add_in_version: Some("old-addin".to_owned()),
        arc_gis_pro_version: Some("old-pro".to_owned()),
        project_name: Some("保留工程".to_owned()),
        project_has_unsaved_changes: Some(true),
        active_map_name: Some("保留地图".to_owned()),
        active_view: Some(ActiveViewSnapshot {
            uri: "map://retained".to_owned(),
            name: "保留地图".to_owned(),
            kind: "map".to_owned(),
            extent: None,
        }),
        layers: vec![LayerSnapshot {
            uri: "layer://retained".to_owned(),
            name: "保留图层".to_owned(),
            long_name: "保留图层".to_owned(),
            layer_type: "FeatureLayer".to_owned(),
            parent_uri: None,
            depth: 0,
            visible: true,
            is_feature_layer: true,
        }],
        last_updated: Some("2026-07-29T00:00:00Z".to_owned()),
        error: None,
    }
}

#[tokio::test]
async fn context_failure_retains_safe_context_but_commits_current_health_as_stale() {
    let client = Arc::new(ScriptedClient::new([
        Ok(json!({"structuredContent": {
            "connected": true, "protocolVersion": "new-health", "projectName": "当前工程"
        }})),
        Err(r"Bearer secret-token at C:\Users\Alice\private\project.aprx".to_owned()),
    ]));
    let state = ready_poll_state(client.clone()).await;
    state
        .update_snapshot(|snapshot| snapshot.arcgis = retained_snapshot())
        .await;

    assert!(
        health_refresh_with(&state, "2026-07-29T01:02:03Z")
            .await
            .expect("stale context refresh")
    );
    let snapshot = state.snapshot().await.arcgis;
    assert_eq!(snapshot.status, BridgeStatus::Connected);
    assert_eq!(snapshot.protocol_version.as_deref(), Some("new-health"));
    assert!(!snapshot.context_is_live);
    assert_eq!(snapshot.project_name.as_deref(), Some("保留工程"));
    assert_eq!(snapshot.layers[0].name, "保留图层");
    assert_eq!(
        snapshot.last_updated.as_deref(),
        Some("2026-07-29T00:00:00Z")
    );
    assert_eq!(snapshot.error.as_deref(), Some("ArcGIS 上下文刷新失败"));
    let serialized = serde_json::to_string(&snapshot).expect("serialize stale context");
    assert!(!serialized.contains("secret-token"));
    assert!(!serialized.contains("Alice"));
    assert!(!serialized.contains("project.aprx"));
}

#[tokio::test]
async fn layer_failure_has_the_same_retained_context_safety() {
    let client = Arc::new(ScriptedClient::new([
        Ok(json!({"structuredContent": {"connected": true, "projectName": "当前工程"}})),
        Ok(json!({"structuredContent": {
            "project": {"name": "当前工程", "path": null, "hasUnsavedChanges": false, "items": []},
            "activeView": {"uri": "map://current", "name": "当前地图", "kind": "map", "extent": null}
        }})),
        Ok(
            json!({"isError": true, "content": [{"type": "text", "text": "SQL SELECT secret FROM roads"}]}),
        ),
    ]));
    let state = ready_poll_state(client).await;
    state
        .update_snapshot(|snapshot| snapshot.arcgis = retained_snapshot())
        .await;

    assert!(
        health_refresh_with(&state, "2026-07-29T01:02:03Z")
            .await
            .expect("stale layer refresh")
    );
    let snapshot = state.snapshot().await.arcgis;
    assert_eq!(snapshot.status, BridgeStatus::Connected);
    assert!(!snapshot.context_is_live);
    assert_eq!(snapshot.project_name.as_deref(), Some("保留工程"));
    assert_eq!(snapshot.layers[0].uri, "layer://retained");
    assert_eq!(
        snapshot.last_updated.as_deref(),
        Some("2026-07-29T00:00:00Z")
    );
    assert!(!serde_json::to_string(&snapshot).unwrap().contains("SELECT"));
}

#[tokio::test]
async fn every_mcp_poll_call_has_a_five_second_timeout() {
    let client = Arc::new(HealthThenHangingClient {
        calls: Mutex::new(Vec::new()),
    });
    let state = ready_poll_state(client.clone()).await;
    state
        .update_snapshot(|snapshot| snapshot.arcgis = retained_snapshot())
        .await;

    let started = std::time::Instant::now();
    assert!(
        health_refresh_with(&state, "2026-07-29T01:02:03Z")
            .await
            .expect("bounded timeout refresh")
    );
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(4_800),
        "timeout returned too early: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_millis(7_500),
        "timeout was not five seconds: {elapsed:?}"
    );
    assert_eq!(client.calls.lock().expect("calls lock").len(), 2);
    let snapshot = state.snapshot().await.arcgis;
    assert!(!snapshot.context_is_live);
    assert_eq!(snapshot.project_name.as_deref(), Some("保留工程"));
}

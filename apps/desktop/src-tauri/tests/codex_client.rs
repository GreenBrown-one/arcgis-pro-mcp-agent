use std::{
    fs::{self, File, OpenOptions},
    future::Future,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use arcgis_pro_agent_desktop_lib::{
    app_state::{AppServerClient, ServerReply},
    codex::{CodexError, CodexEvent, CodexRuntime, CodexStartOptions, build_codex_command},
    runtime_secret::create_runtime_file,
};
use serde_json::{Value, json};
use tokio::time::timeout;

const FAKE_SCENARIO_ENV: &str = "ARCGIS_CODEX_FAKE_SCENARIO";
const FAKE_CAPTURE_ENV: &str = "ARCGIS_CODEX_FAKE_CAPTURE";
const FAKE_GATE_ENV: &str = "ARCGIS_CODEX_FAKE_GATE";
const FAKE_SECRET_ENV: &str = "ARCGIS_CODEX_FAKE_SECRET";
const WATCHDOG: Duration = Duration::from_secs(10);
const PERSISTENT_EVENT_CAPACITY: usize = 256;

#[test]
fn runtime_file_contains_a_fresh_redacted_secret() {
    let local_app_data = TestDir::new();

    let runtime_file = create_runtime_file(&local_app_data.path).expect("create runtime file");
    let json: Value = serde_json::from_slice(
        &fs::read(runtime_file.path()).expect("read generated runtime file"),
    )
    .expect("parse generated runtime file");
    let token = json["token"].as_str().expect("runtime token is a string");

    assert_eq!(json["pipeName"], "ArcGISProAgent.Bridge.v1");
    assert_eq!(token.len(), 43, "32 base64url bytes without padding");
    assert!(!token.contains('='));
    assert!(!format!("{runtime_file:?}").contains(token));
    assert!(!format!("{runtime_file}").contains(token));
}

#[test]
fn runtime_file_is_atomically_replaced_without_temporary_secret_files() {
    let local_app_data = TestDir::new();
    let first = create_runtime_file(&local_app_data.path).expect("create first runtime file");
    let first_json: Value =
        serde_json::from_slice(&fs::read(first.path()).expect("read first runtime file"))
            .expect("parse first runtime file");
    let second = create_runtime_file(&local_app_data.path).expect("replace runtime file");
    let second_json: Value =
        serde_json::from_slice(&fs::read(second.path()).expect("read second runtime file"))
            .expect("parse second runtime file");

    assert_eq!(first.path(), second.path());
    assert_ne!(first_json["token"], second_json["token"]);
    let entries = fs::read_dir(second.path().parent().expect("runtime directory"))
        .expect("list runtime directory")
        .map(|entry| entry.expect("runtime directory entry").file_name())
        .collect::<Vec<_>>();
    assert_eq!(entries, ["bridge.json"]);
}

#[cfg(windows)]
#[test]
fn runtime_file_has_a_protected_two_principal_windows_dacl() {
    let local_app_data = TestDir::new();
    let runtime_file =
        create_runtime_file(&local_app_data.path).expect("create protected runtime file");

    let sddl = windows_file_dacl_sddl(runtime_file.path());
    let principals = sddl
        .split('(')
        .filter_map(|ace| ace.strip_prefix("A;"))
        .filter_map(|ace| ace.trim_end_matches(')').rsplit(';').next())
        .map(str::to_owned)
        .collect::<std::collections::HashSet<_>>();
    let current_user_sid = windows_current_user_sid();
    let current_user_is_local_administrator = current_user_sid.ends_with("-500");

    assert!(sddl.starts_with("D:P"), "DACL must be protected: {sddl}");
    assert_eq!(
        sddl.matches('(').count(),
        2,
        "DACL must contain exactly two ACEs: {sddl}"
    );
    assert!(principals.contains("SY"), "DACL must grant SYSTEM: {sddl}");
    assert!(
        principals.contains(&current_user_sid)
            || (current_user_is_local_administrator && principals.contains("LA")),
        "DACL must grant the current user (Windows may canonicalize RID 500 as LA): {sddl}"
    );
}

#[test]
fn production_command_registers_only_arcgis_without_api_key_or_token() {
    let local_app_data = TestDir::new();
    let runtime_root = local_app_data.path.join("配置 runtime");
    let runtime_file = create_runtime_file(&runtime_root).expect("create command runtime file");
    let runtime_json: Value =
        serde_json::from_slice(&fs::read(runtime_file.path()).expect("read command runtime file"))
            .expect("parse command runtime file");
    let token = runtime_json["token"].as_str().expect("runtime token");
    let mcp_command = r#"C:\Program Files\地图 "quoted"\dotnet.exe"#;
    let mcp_argument = r#"E:\ArcGIS 数据\agent "one".dll"#;
    let options = CodexStartOptions {
        codex_command: PathBuf::from(r"C:\tools\codex.cmd"),
        codex_home: local_app_data.path.join("codex-home"),
        mcp_command: PathBuf::from(mcp_command),
        mcp_args: vec![mcp_argument.into()],
        local_app_data: local_app_data.path.clone(),
    };

    let command = build_codex_command(&options, &runtime_file);
    let args = command
        .get_args()
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let rendered = args.join(" ");
    let removed_environment = command
        .get_envs()
        .filter_map(|(key, value)| value.is_none().then(|| key.to_string_lossy().into_owned()))
        .collect::<Vec<_>>();

    assert_eq!(command.get_program(), r"C:\tools\codex.cmd");
    assert_eq!(&args[..2], ["app-server", "--stdio"]);
    let config_pairs = args[2..]
        .chunks_exact(2)
        .map(|pair| {
            assert_eq!(pair[0], "-c");
            pair[1].clone()
        })
        .collect::<Vec<_>>();
    assert_eq!(config_pairs.len(), 4);
    assert_eq!(config_pairs[0], "mcp_servers={}");
    assert!(config_pairs[1].starts_with("mcp_servers.arcgis.command="));
    assert!(config_pairs[2].starts_with("mcp_servers.arcgis.args="));
    assert!(config_pairs[3].starts_with("mcp_servers.arcgis.env="));

    let empty_config: toml::Table =
        toml::from_str(&config_pairs[0]).expect("parse empty MCP config");
    assert_eq!(
        empty_config["mcp_servers"].as_table().map(toml::Table::len),
        Some(0)
    );
    let command_config: toml::Table =
        toml::from_str(&config_pairs[1]).expect("parse MCP command config");
    assert_eq!(
        command_config["mcp_servers"]["arcgis"]["command"].as_str(),
        Some(mcp_command)
    );
    let args_config: toml::Table = toml::from_str(&config_pairs[2]).expect("parse MCP args config");
    assert_eq!(
        args_config["mcp_servers"]["arcgis"]["args"]
            .as_array()
            .and_then(|values| values.first())
            .and_then(toml::Value::as_str),
        Some(mcp_argument)
    );
    let env_config: toml::Table =
        toml::from_str(&config_pairs[3]).expect("parse MCP environment config");
    assert_eq!(
        env_config["mcp_servers"]["arcgis"]["env"]["ARCGIS_AGENT_RUNTIME_FILE"].as_str(),
        Some(runtime_file.path().to_string_lossy().as_ref())
    );
    assert!(!rendered.contains(token));
    assert!(!rendered.contains("api_key"));
    assert!(removed_environment.contains(&"OPENAI_API_KEY".to_owned()));
    assert!(removed_environment.contains(&"AZURE_OPENAI_API_KEY".to_owned()));
    assert!(removed_environment.contains(&"CODEX_API_KEY".to_owned()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn initializes_once_and_routes_responses_by_id() {
    let directory = TestDir::new();
    let capture = directory.path.join("received.jsonl");
    let runtime = watchdog(
        "start fake app-server",
        CodexRuntime::start_with_command(fake_command("route", &capture)),
    )
    .await
    .expect("initialize fake app-server");

    let account = watchdog(
        "account/read response",
        runtime.request("account/read", json!({ "refreshToken": false })),
    )
    .await
    .expect("route account/read response");
    assert_eq!(account["account"], Value::Null);
    assert_eq!(account["requiresOpenaiAuth"], true);

    watchdog("shut down fake app-server", runtime.shutdown())
        .await
        .expect("shut down fake app-server");

    let received = read_json_lines(&capture);
    let initialize = received
        .iter()
        .filter(|message| message["method"] == "initialize")
        .collect::<Vec<_>>();
    let initialized = received
        .iter()
        .filter(|message| message["method"] == "initialized")
        .collect::<Vec<_>>();
    assert_eq!(initialize.len(), 1, "initialize must be sent exactly once");
    assert_eq!(
        initialized.len(),
        1,
        "initialized must be sent exactly once"
    );
    assert_eq!(initialize[0]["id"], 1);
    assert_eq!(
        initialize[0]["params"]["clientInfo"]["name"],
        "arcgis_pro_agent"
    );
    assert_eq!(initialize[0]["params"]["clientInfo"]["version"], "0.1.0");
    assert_eq!(
        initialize[0]["params"]["capabilities"]["mcpServerOpenaiFormElicitation"],
        true
    );
    assert_eq!(initialized[0]["params"], json!({}));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn preserves_unknown_notifications_and_server_requests() {
    let directory = TestDir::new();
    let capture = directory.path.join("received.jsonl");
    let runtime = watchdog(
        "start event fake app-server",
        CodexRuntime::start_with_command(fake_command("events", &capture)),
    )
    .await
    .expect("initialize event fake app-server");
    let mut events = runtime.subscribe();

    watchdog(
        "request fake events",
        runtime.request("test/events", json!({})),
    )
    .await
    .expect("fake event request response");

    let notification = watchdog("receive unknown notification", events.recv())
        .await
        .expect("unknown notification event");
    let server_request = watchdog("receive unknown server request", events.recv())
        .await
        .expect("unknown server request event");
    assert_eq!(
        notification,
        CodexEvent::Notification {
            method: "future/notification".to_owned(),
            params: json!({ "kept": true }),
        }
    );
    assert_eq!(
        server_request,
        CodexEvent::ServerRequest {
            id: json!("server-request-1"),
            method: "future/request".to_owned(),
            params: json!({ "kept": 42 }),
        }
    );

    watchdog("shut down event fake app-server", runtime.shutdown())
        .await
        .expect("shut down event fake app-server");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn app_server_client_response_writer_sends_exact_jsonl_to_the_real_runtime() {
    let directory = TestDir::new();
    let capture = directory.path.join("server-response.jsonl");
    let runtime = watchdog(
        "start response-writer fake app-server",
        CodexRuntime::start_with_command(fake_command("events", &capture)),
    )
    .await
    .expect("initialize response-writer fake app-server");
    let mut events = runtime.subscribe();

    watchdog(
        "request response-writer fake event",
        runtime.request("test/events", json!({})),
    )
    .await
    .expect("fake event request response");
    let _notification = watchdog("receive response-writer notification", events.recv())
        .await
        .expect("notification event");
    let request = watchdog("receive response-writer server request", events.recv())
        .await
        .expect("server request event");
    let id = match request {
        CodexEvent::ServerRequest { id, .. } => id,
        other => panic!("expected server request, got {other:?}"),
    };

    watchdog(
        "write server response",
        runtime.respond(
            id.clone(),
            ServerReply::Result(json!({"action": "decline"})),
        ),
    )
    .await
    .expect("write server response");
    watchdog("shut down response-writer fake", runtime.shutdown())
        .await
        .expect("shut down response-writer fake");

    assert!(
        read_json_lines(&capture).contains(&json!({
            "id": id,
            "result": {"action": "decline"}
        })),
        "the trait implementation must use the runtime JSONL response writer"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn routes_concurrent_responses_by_id_when_they_arrive_out_of_order() {
    let directory = TestDir::new();
    let capture = directory.path.join("received.jsonl");
    let runtime = watchdog(
        "start out-of-order fake app-server",
        CodexRuntime::start_with_command(fake_command("out-of-order", &capture)),
    )
    .await
    .expect("initialize out-of-order fake app-server");

    let (first, second) = watchdog("route out-of-order responses", async {
        tokio::join!(
            runtime.request("test/first", json!({ "value": 1 })),
            runtime.request("test/second", json!({ "value": 2 }))
        )
    })
    .await;
    assert_eq!(first.expect("first response")["method"], "test/first");
    assert_eq!(second.expect("second response")["method"], "test/second");

    watchdog("shut down out-of-order fake", runtime.shutdown())
        .await
        .expect("shut down out-of-order fake");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_request_does_not_interrupt_queued_frame_or_leak_pending_id() {
    let directory = TestDir::new();
    let capture = directory.path.join("received.jsonl");
    let gate = fake_gate(&capture);
    let runtime = Arc::new(
        watchdog(
            "start cancellation fake app-server",
            CodexRuntime::start_with_command(fake_command("cancel-during-write", &capture)),
        )
        .await
        .expect("initialize cancellation fake app-server"),
    );
    let mut events = runtime.subscribe();
    let request_runtime = runtime.clone();
    let request = tokio::spawn(async move {
        request_runtime
            .request(
                "test/cancel-write",
                json!({ "payload": "x".repeat(4 * 1024 * 1024) }),
            )
            .await
    });

    watchdog("observe registered cancellable request", async {
        while runtime.pending_request_count().await != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await;
    request.abort();
    watchdog("join cancelled request task", request)
        .await
        .expect_err("request task is cancelled");
    File::create(&gate).expect("open fake write gate");

    assert_eq!(runtime.pending_request_count().await, 0);
    assert_eq!(
        watchdog(
            "receive complete cancelled frame notification",
            events.recv()
        )
        .await
        .expect("complete cancelled frame notification"),
        CodexEvent::Notification {
            method: "test/cancel-frame-received".to_owned(),
            params: json!({ "bytes": 4 * 1024 * 1024 }),
        }
    );
    let follow_up = watchdog(
        "complete request after cancelled frame",
        runtime.request("test/after-cancel", json!({})),
    )
    .await
    .expect("writer remains usable after cancellation");
    assert_eq!(follow_up, json!({ "ok": true }));
    assert_eq!(runtime.pending_request_count().await, 0);

    watchdog("shut down cancellation fake", runtime.shutdown())
        .await
        .expect("shut down cancellation fake");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_stdout_is_only_a_diagnostic_and_does_not_satisfy_pending() {
    let directory = TestDir::new();
    let capture = directory.path.join("received.jsonl");
    let runtime = watchdog(
        "start malformed fake app-server",
        CodexRuntime::start_with_command(fake_command("malformed", &capture)),
    )
    .await
    .expect("initialize malformed fake app-server");
    let mut events = runtime.subscribe();

    let result = watchdog(
        "receive valid response after malformed line",
        runtime.request("test/malformed", json!({})),
    )
    .await
    .expect("pending request receives only its valid response");
    assert_eq!(result["afterMalformed"], true);
    assert_eq!(
        watchdog("receive malformed-line diagnostic", events.recv())
            .await
            .expect("malformed line diagnostic"),
        CodexEvent::ProtocolError {
            message: "app-server emitted malformed JSONL".to_owned(),
        }
    );

    watchdog("shut down malformed fake", runtime.shutdown())
        .await
        .expect("shut down malformed fake");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_response_shapes_do_not_remove_the_pending_request() {
    let directory = TestDir::new();
    let capture = directory.path.join("received.jsonl");
    let runtime = watchdog(
        "start malformed-response fake app-server",
        CodexRuntime::start_with_command(fake_command("malformed-response", &capture)),
    )
    .await
    .expect("initialize malformed-response fake app-server");
    let mut events = runtime.subscribe();

    let result = watchdog(
        "receive valid response after malformed response objects",
        runtime.request("test/malformed-response", json!({})),
    )
    .await
    .expect("pending request survives malformed response objects");

    assert_eq!(result, json!({ "valid": true }));
    for _ in 0..3 {
        assert!(matches!(
            watchdog("receive malformed-response diagnostic", events.recv())
                .await
                .expect("malformed response diagnostic"),
            CodexEvent::ProtocolError { .. }
        ));
    }
    watchdog("shut down malformed-response fake", runtime.shutdown())
        .await
        .expect("shut down malformed-response fake");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn child_exit_fails_pending_request_and_emits_process_event() {
    let directory = TestDir::new();
    let capture = directory.path.join("received.jsonl");
    let runtime = watchdog(
        "start exit fake app-server",
        CodexRuntime::start_with_command(fake_command("exit", &capture)),
    )
    .await
    .expect("initialize exit fake app-server");
    let mut events = runtime.subscribe();

    let error = watchdog(
        "fail request when child exits",
        runtime.request("test/exit", json!({})),
    )
    .await
    .expect_err("child exit must fail pending request");
    assert!(matches!(error, CodexError::ProcessExited { .. }));
    assert!(matches!(
        watchdog("receive process exit event", events.recv())
            .await
            .expect("process exit event"),
        CodexEvent::ProcessExited { .. }
    ));
    watchdog("observe already-exited shutdown", runtime.shutdown())
        .await
        .expect("shutdown after exit is idempotent");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn final_response_and_tail_notification_are_routed_before_process_exit() {
    let directory = TestDir::new();
    let capture = directory.path.join("received.jsonl");
    let runtime = watchdog(
        "start final-response fake app-server",
        CodexRuntime::start_with_command(fake_command("final-response-exit", &capture)),
    )
    .await
    .expect("initialize final-response fake app-server");
    drain_fake_prelude(&runtime).await;

    let result = watchdog(
        "route final flushed response",
        runtime.request("test/final-exit", json!({})),
    )
    .await
    .expect("final flushed response is not lost on exit");
    assert_eq!(result, json!({ "final": true }));
    assert_eq!(
        watchdog("receive tail notification", runtime.next_event())
            .await
            .expect("tail notification before terminal"),
        CodexEvent::Notification {
            method: "test/tail".to_owned(),
            params: json!({ "sequence": 1 }),
        }
    );
    assert_eq!(
        watchdog("receive ordered process exit", runtime.next_event())
            .await
            .expect("terminal process exit event"),
        CodexEvent::ProcessExited { code: Some(23) }
    );
    assert_eq!(
        watchdog("observe terminal event queue", runtime.next_event()).await,
        None
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn large_tail_frames_and_stderr_are_complete_and_stable_before_terminal() {
    const LARGE_TAIL_BYTES: usize = 2 * 1024 * 1024;

    let directory = TestDir::new();
    let capture = directory.path.join("received.jsonl");
    let runtime = watchdog(
        "start large-tail fake app-server",
        CodexRuntime::start_with_command(fake_command("large-tail-exit", &capture)),
    )
    .await
    .expect("initialize large-tail fake app-server");
    drain_fake_prelude(&runtime).await;

    let response = watchdog(
        "route large final response",
        runtime.request("test/large-tail-exit", json!({})),
    )
    .await
    .expect("large final response is routed before exit");
    assert_eq!(
        response["payload"].as_str().map(str::len),
        Some(LARGE_TAIL_BYTES)
    );
    let notification = watchdog("route large tail notification", runtime.next_event())
        .await
        .expect("large tail notification is retained");
    assert!(matches!(
        notification,
        CodexEvent::Notification { ref method, ref params }
            if method == "test/large-tail"
                && params["payload"].as_str().map(str::len) == Some(LARGE_TAIL_BYTES)
    ));
    assert_eq!(
        watchdog(
            "publish terminal after large readers drain",
            runtime.next_event()
        )
        .await,
        Some(CodexEvent::ProcessExited { code: Some(41) })
    );

    let terminal_stderr = runtime.stderr_lines().await;
    assert_eq!(terminal_stderr.len(), 200);
    assert_eq!(
        terminal_stderr.first().map(String::as_str),
        Some("large-tail-stderr-100")
    );
    assert_eq!(
        terminal_stderr.last().map(String::as_str),
        Some("large-tail-stderr-299")
    );
    tokio::task::yield_now().await;
    assert_eq!(runtime.stderr_lines().await, terminal_stderr);
    let (first_none, second_none) = watchdog("reject late events after large terminal", async {
        tokio::join!(runtime.next_event(), runtime.next_event())
    })
    .await;
    assert_eq!((first_none, second_none), (None, None));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn normal_exit_wakes_every_concurrent_persistent_event_waiter() {
    let directory = TestDir::new();
    let capture = directory.path.join("received.jsonl");
    let runtime = Arc::new(
        watchdog(
            "start concurrent-waiter fake app-server",
            CodexRuntime::start_with_command(fake_command("exit", &capture)),
        )
        .await
        .expect("initialize concurrent-waiter fake app-server"),
    );
    drain_fake_prelude(&runtime).await;

    let first = tokio::spawn({
        let runtime = runtime.clone();
        async move { runtime.next_event().await }
    });
    let second = tokio::spawn({
        let runtime = runtime.clone();
        async move { runtime.next_event().await }
    });
    watchdog("park both persistent event waiters", async {
        while runtime.persistent_event_waiter_count() != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await;

    let error = watchdog(
        "trigger normal exit with concurrent waiters",
        runtime.request("test/exit", json!({})),
    )
    .await
    .expect_err("exit request fails when fake exits");
    assert!(matches!(error, CodexError::ProcessExited { .. }));
    let (first, second) = watchdog("wake both terminal waiters", async {
        tokio::join!(first, second)
    })
    .await;
    let terminal_results = [
        first.expect("first waiter task completes"),
        second.expect("second waiter task completes"),
    ];
    assert_eq!(
        terminal_results
            .iter()
            .filter(|event| matches!(event, Some(CodexEvent::ProcessExited { code: Some(17) })))
            .count(),
        1
    );
    assert_eq!(
        terminal_results
            .iter()
            .filter(|event| event.is_none())
            .count(),
        1
    );

    let (first_none, second_none) = watchdog("retain terminal state for every waiter", async {
        tokio::join!(runtime.next_event(), runtime.next_event())
    })
    .await;
    assert_eq!((first_none, second_none), (None, None));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn early_events_are_persisted_and_terminal_means_stderr_is_stable() {
    let directory = TestDir::new();
    let capture = directory.path.join("received.jsonl");
    let runtime = watchdog(
        "start early-event fake app-server",
        CodexRuntime::start_with_command(fake_command("early-events-exit", &capture)),
    )
    .await
    .expect("initialize early-event fake app-server");
    drain_fake_prelude(&runtime).await;

    assert_eq!(
        watchdog("receive early notification", runtime.next_event())
            .await
            .expect("early notification persisted"),
        CodexEvent::Notification {
            method: "test/early-notification".to_owned(),
            params: json!({ "early": true }),
        }
    );
    assert_eq!(
        watchdog("receive early server request", runtime.next_event())
            .await
            .expect("early server request persisted"),
        CodexEvent::ServerRequest {
            id: json!("early-server-request"),
            method: "test/early-request".to_owned(),
            params: json!({ "early": 2 }),
        }
    );
    assert_eq!(
        watchdog("receive early process exit", runtime.next_event())
            .await
            .expect("early process exit persisted"),
        CodexEvent::ProcessExited { code: Some(29) }
    );
    assert_eq!(runtime.stderr_lines().await, ["early-stderr-tail"]);
    let (first_none, second_none) = watchdog("wake early-exit terminal waiters", async {
        tokio::join!(runtime.next_event(), runtime.next_event())
    })
    .await;
    assert_eq!((first_none, second_none), (None, None));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn persistent_event_queue_is_bounded_and_keeps_overflow_and_terminal_diagnostics() {
    let directory = TestDir::new();
    let capture = directory.path.join("received.jsonl");
    let runtime = watchdog(
        "start event-flood fake app-server",
        CodexRuntime::start_with_command(fake_command("event-flood-exit", &capture)),
    )
    .await
    .expect("initialize event-flood fake app-server");
    watchdog(
        "wait for event-flood terminal publication",
        runtime.shutdown(),
    )
    .await
    .expect("event-flood fake reaches terminal before inspection");

    let mut retained = Vec::new();
    loop {
        let event = watchdog("drain bounded persistent events", runtime.next_event())
            .await
            .expect("terminal event remains in bounded queue");
        let terminal = matches!(event, CodexEvent::ProcessExited { code: Some(37) });
        retained.push(event);
        if terminal {
            break;
        }
    }

    assert!(retained.len() <= PERSISTENT_EVENT_CAPACITY);
    assert_eq!(
        retained
            .iter()
            .filter(|event| matches!(event, CodexEvent::ProtocolError { message } if message == "persistent event queue lagged; oldest events were dropped"))
            .count(),
        1
    );
    assert!(matches!(
        retained.last(),
        Some(CodexEvent::ProcessExited { code: Some(37) })
    ));
    assert_eq!(
        watchdog("observe bounded queue terminal state", runtime.next_event()).await,
        None
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stderr_is_kept_as_a_two_hundred_line_diagnostic_ring() {
    let directory = TestDir::new();
    let capture = directory.path.join("received.jsonl");
    let runtime = watchdog(
        "start stderr fake app-server",
        CodexRuntime::start_with_command(fake_command("stderr", &capture)),
    )
    .await
    .expect("initialize stderr fake app-server");

    watchdog(
        "ask fake to emit stderr",
        runtime.request("test/stderr", json!({})),
    )
    .await
    .expect("stderr fake response");
    watchdog("shut down stderr fake", runtime.shutdown())
        .await
        .expect("shut down stderr fake");
    let lines = runtime.stderr_lines().await;

    assert_eq!(lines.len(), 200);
    assert_eq!(lines.first().map(String::as_str), Some("stderr-line-050"));
    assert_eq!(lines.last().map(String::as_str), Some("stderr-line-249"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stderr_and_error_diagnostics_redact_every_passed_secret() {
    let directory = TestDir::new();
    let capture = directory.path.join("received.jsonl");
    let secret = "runtime-token-never-visible-0123456789".to_owned();
    let mut command = fake_command("secret-stderr", &capture);
    command.env(FAKE_SECRET_ENV, &secret);
    let runtime = watchdog(
        "start secret stderr fake app-server",
        CodexRuntime::start_with_command_and_secrets(command, vec![secret.clone()]),
    )
    .await
    .expect("initialize secret stderr fake app-server");
    drain_fake_prelude(&runtime).await;

    let error = watchdog(
        "receive redacted server error",
        runtime.request("test/secret-stderr", json!({})),
    )
    .await
    .expect_err("fake returns server error");
    while !matches!(
        watchdog("wait for secret fake terminal", runtime.next_event()).await,
        Some(CodexEvent::ProcessExited { .. })
    ) {}
    let lines = runtime.stderr_lines().await;
    let diagnostics = format!("{lines:?} {error:?} {error}");

    assert_eq!(lines, ["sensitive=[REDACTED]"]);
    assert!(!diagnostics.contains(&secret));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_kills_a_child_that_refuses_to_exit_after_stdin_closes() {
    let directory = TestDir::new();
    let capture = directory.path.join("received.jsonl");
    let runtime = watchdog(
        "start shutdown-hang fake app-server",
        CodexRuntime::start_with_command(fake_command("hang-on-shutdown", &capture)),
    )
    .await
    .expect("initialize shutdown-hang fake app-server");
    drain_fake_prelude(&runtime).await;

    watchdog("force bounded shutdown of hung child", runtime.shutdown())
        .await
        .expect("shutdown kills hung child and reaches terminal");
    assert!(matches!(
        watchdog("receive killed child terminal", runtime.next_event()).await,
        Some(CodexEvent::ProcessExited { .. })
    ));
    assert_eq!(
        watchdog("finish killed child terminal queue", runtime.next_event()).await,
        None
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ensure_terminated_forces_and_awaits_a_live_child() {
    let directory = TestDir::new();
    let capture = directory.path.join("received.jsonl");
    let runtime = watchdog(
        "start force-termination fake app-server",
        CodexRuntime::start_with_command(fake_command("hang-on-shutdown", &capture)),
    )
    .await
    .expect("initialize force-termination fake app-server");
    drain_fake_prelude(&runtime).await;

    watchdog(
        "force and confirm child termination",
        runtime.ensure_terminated(),
    )
    .await
    .expect("forced child termination is confirmed");
    assert!(matches!(
        watchdog(
            "receive force-terminated child terminal",
            runtime.next_event()
        )
        .await,
        Some(CodexEvent::ProcessExited { .. })
    ));
    assert_eq!(
        watchdog(
            "finish force-terminated terminal queue",
            runtime.next_event()
        )
        .await,
        None
    );
}

#[cfg(windows)]
#[tokio::test(flavor = "current_thread")]
async fn initialize_failure_confirms_the_real_child_exited_before_returning() {
    let directory = TestDir::new();
    let capture = directory.path.join("initialize-failure.jsonl");

    let result = CodexRuntime::start_with_command(fake_command("initialize-error", &capture)).await;
    assert!(result.is_err(), "fake initialize error must fail startup");
    let pid = fs::read_to_string(fake_pid_path(&capture))
        .expect("read fake child PID")
        .parse::<u32>()
        .expect("fake child PID is numeric");

    assert!(
        !windows_process_is_running(pid),
        "start failure must await confirmed termination instead of relying on Drop"
    );
}

#[test]
#[ignore = "launched by integration tests as a fake stdio app-server"]
fn fake_codex_app_server_process() {
    let Some(scenario) = std::env::var_os(FAKE_SCENARIO_ENV) else {
        return;
    };
    let capture =
        PathBuf::from(std::env::var_os(FAKE_CAPTURE_ENV).expect("fake capture path is configured"));
    let (completed, watchdog_receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if watchdog_receiver.recv_timeout(WATCHDOG * 2).is_err() {
            std::process::exit(124);
        }
    });
    run_fake_server(&scenario.to_string_lossy(), &capture);
    let _ = completed.send(());
}

fn fake_command(scenario: &str, capture: &PathBuf) -> Command {
    let mut command = Command::new(std::env::current_exe().expect("resolve integration test exe"));
    command
        .arg("--ignored")
        .arg("--exact")
        .arg("fake_codex_app_server_process")
        .arg("--quiet")
        .env(FAKE_SCENARIO_ENV, scenario)
        .env(FAKE_CAPTURE_ENV, capture)
        .env(FAKE_GATE_ENV, fake_gate(capture));
    command
}

fn run_fake_server(scenario: &str, capture: &PathBuf) {
    assert!(matches!(
        scenario,
        "route"
            | "events"
            | "out-of-order"
            | "malformed"
            | "malformed-response"
            | "cancel-during-write"
            | "exit"
            | "final-response-exit"
            | "large-tail-exit"
            | "early-events-exit"
            | "event-flood-exit"
            | "stderr"
            | "secret-stderr"
            | "hang-on-shutdown"
            | "initialize-error"
    ));
    fs::write(fake_pid_path(capture), std::process::id().to_string())
        .expect("write fake child PID");
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    let mut delayed_responses = Vec::new();
    let mut capture_file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(capture)
        .expect("create fake capture file");
    write_json_line(
        &mut stdout,
        json!({ "method": "test/fake-ready", "params": {} }),
    );

    for line in BufReader::new(stdin.lock()).lines() {
        let line = line.expect("read client JSONL");
        writeln!(capture_file, "{line}").expect("capture client JSONL");
        capture_file.flush().expect("flush captured client JSONL");
        let message: Value = serde_json::from_str(&line).expect("client writes valid JSONL");
        match message["method"].as_str() {
            Some("initialize") if scenario == "initialize-error" => write_json_line(
                &mut stdout,
                json!({
                    "id": message["id"],
                    "error": {"code": -32000, "message": "scripted initialize failure"}
                }),
            ),
            Some("initialize") => write_json_line(
                &mut stdout,
                json!({
                    "id": message["id"],
                    "result": {
                        "codexHome": "C:\\fake-codex-home",
                        "platformFamily": "windows",
                        "platformOs": "windows",
                        "userAgent": "fake-app-server"
                    }
                }),
            ),
            Some("account/read") => write_json_line(
                &mut stdout,
                json!({
                    "id": message["id"],
                    "result": { "account": null, "requiresOpenaiAuth": true }
                }),
            ),
            Some("test/events") if scenario == "events" => {
                write_json_line(
                    &mut stdout,
                    json!({ "method": "future/notification", "params": { "kept": true } }),
                );
                write_json_line(
                    &mut stdout,
                    json!({
                        "id": "server-request-1",
                        "method": "future/request",
                        "params": { "kept": 42 }
                    }),
                );
                write_json_line(
                    &mut stdout,
                    json!({ "id": message["id"], "result": { "sent": true } }),
                );
            }
            Some(method @ ("test/first" | "test/second")) if scenario == "out-of-order" => {
                delayed_responses.push((message["id"].clone(), method.to_owned()));
                if delayed_responses.len() == 2 {
                    for (id, method) in delayed_responses.drain(..).rev() {
                        write_json_line(
                            &mut stdout,
                            json!({ "id": id, "result": { "method": method } }),
                        );
                    }
                }
            }
            Some("test/malformed") if scenario == "malformed" => {
                stdout
                    .write_all(b"this is not json\n")
                    .expect("write malformed stdout line");
                stdout.flush().expect("flush malformed stdout line");
                write_json_line(
                    &mut stdout,
                    json!({ "id": message["id"], "result": { "afterMalformed": true } }),
                );
            }
            Some("test/malformed-response") if scenario == "malformed-response" => {
                write_json_line(
                    &mut stdout,
                    json!({
                        "id": message["id"],
                        "result": { "invalid": "both" },
                        "error": { "code": -1, "message": "also present" }
                    }),
                );
                write_json_line(
                    &mut stdout,
                    json!({
                        "id": message["id"],
                        "method": "not/a/response",
                        "result": { "invalid": "method" }
                    }),
                );
                write_json_line(
                    &mut stdout,
                    json!({
                        "id": message["id"].to_string(),
                        "result": { "invalid": "string-id" }
                    }),
                );
                write_json_line(
                    &mut stdout,
                    json!({ "id": message["id"], "result": { "valid": true } }),
                );
            }
            Some("test/cancel-write") if scenario == "cancel-during-write" => {
                let bytes = message["params"]["payload"]
                    .as_str()
                    .expect("cancel payload string")
                    .len();
                write_json_line(
                    &mut stdout,
                    json!({
                        "method": "test/cancel-frame-received",
                        "params": { "bytes": bytes }
                    }),
                );
                write_json_line(
                    &mut stdout,
                    json!({ "id": message["id"], "result": { "late": true } }),
                );
            }
            Some("test/after-cancel") if scenario == "cancel-during-write" => write_json_line(
                &mut stdout,
                json!({ "id": message["id"], "result": { "ok": true } }),
            ),
            Some("test/exit") if scenario == "exit" => std::process::exit(17),
            Some("test/final-exit") if scenario == "final-response-exit" => {
                write_json_line(
                    &mut stdout,
                    json!({ "id": message["id"], "result": { "final": true } }),
                );
                write_json_line(
                    &mut stdout,
                    json!({
                        "method": "test/tail",
                        "params": { "sequence": 1 }
                    }),
                );
                std::process::exit(23);
            }
            Some("test/large-tail-exit") if scenario == "large-tail-exit" => {
                let response_payload = "r".repeat(2 * 1024 * 1024);
                let notification_payload = "n".repeat(2 * 1024 * 1024);
                write_json_line(
                    &mut stdout,
                    json!({
                        "id": message["id"],
                        "result": { "payload": response_payload }
                    }),
                );
                write_json_line(
                    &mut stdout,
                    json!({
                        "method": "test/large-tail",
                        "params": { "payload": notification_payload }
                    }),
                );
                for index in 0..300 {
                    writeln!(stderr, "large-tail-stderr-{index:03}")
                        .expect("write large-tail stderr line");
                }
                stderr.flush().expect("flush large-tail stderr lines");
                std::process::exit(41);
            }
            Some("test/stderr") if scenario == "stderr" => {
                for index in 0..250 {
                    writeln!(stderr, "stderr-line-{index:03}").expect("write fake stderr line");
                }
                stderr.flush().expect("flush fake stderr lines");
                write_json_line(
                    &mut stdout,
                    json!({ "id": message["id"], "result": { "stderr": true } }),
                );
            }
            Some("test/secret-stderr") if scenario == "secret-stderr" => {
                let secret = std::env::var(FAKE_SECRET_ENV).expect("fake secret configured");
                writeln!(stderr, "sensitive={secret}").expect("write sensitive fake stderr");
                stderr.flush().expect("flush sensitive fake stderr");
                write_json_line(
                    &mut stdout,
                    json!({
                        "id": message["id"],
                        "error": { "code": -1, "message": secret }
                    }),
                );
                std::process::exit(31);
            }
            Some("initialized") => {
                if scenario == "cancel-during-write" {
                    wait_for_fake_gate();
                }
                if scenario == "early-events-exit" {
                    write_json_line(
                        &mut stdout,
                        json!({
                            "method": "test/early-notification",
                            "params": { "early": true }
                        }),
                    );
                    write_json_line(
                        &mut stdout,
                        json!({
                            "id": "early-server-request",
                            "method": "test/early-request",
                            "params": { "early": 2 }
                        }),
                    );
                    writeln!(stderr, "early-stderr-tail").expect("write early stderr tail");
                    stderr.flush().expect("flush early stderr tail");
                    std::process::exit(29);
                }
                if scenario == "event-flood-exit" {
                    for sequence in 0..(PERSISTENT_EVENT_CAPACITY * 2 - 1) {
                        write_json_line(
                            &mut stdout,
                            json!({
                                "method": "test/progress",
                                "params": { "sequence": sequence }
                            }),
                        );
                    }
                    std::process::exit(37);
                }
                if scenario == "hang-on-shutdown" {
                    let (_never_send, wait) = std::sync::mpsc::channel::<()>();
                    if wait.recv_timeout(WATCHDOG * 2).is_err() {
                        std::process::exit(125);
                    }
                }
            }
            method => panic!("unexpected fake request method: {method:?}"),
        }
    }
}

fn write_json_line(output: &mut impl Write, value: Value) {
    serde_json::to_writer(&mut *output, &value).expect("write fake JSON response");
    output
        .write_all(b"\n")
        .expect("terminate fake JSONL response");
    output.flush().expect("flush fake JSON response");
}

fn read_json_lines(path: &PathBuf) -> Vec<Value> {
    BufReader::new(File::open(path).expect("open fake capture"))
        .lines()
        .map(|line| {
            serde_json::from_str(&line.expect("read captured request"))
                .expect("captured request is JSON")
        })
        .collect()
}

fn fake_gate(capture: &std::path::Path) -> PathBuf {
    capture.with_extension("gate")
}

fn fake_pid_path(capture: &std::path::Path) -> PathBuf {
    capture.with_extension("pid")
}

#[cfg(windows)]
fn windows_process_is_running(pid: u32) -> bool {
    use windows::Win32::{
        Foundation::{CloseHandle, STILL_ACTIVE},
        System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };

    let Ok(process) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) })
    else {
        return false;
    };
    let mut exit_code = 0_u32;
    let queried = unsafe { GetExitCodeProcess(process, &mut exit_code) }.is_ok();
    unsafe { CloseHandle(process) }.expect("close fake process query handle");
    queried && exit_code == STILL_ACTIVE.0 as u32
}

fn wait_for_fake_gate() {
    let gate = PathBuf::from(std::env::var_os(FAKE_GATE_ENV).expect("fake gate path configured"));
    let deadline = std::time::Instant::now() + WATCHDOG;
    while !gate.exists() {
        if std::time::Instant::now() >= deadline {
            panic!("fake gate watchdog expired");
        }
        std::thread::yield_now();
    }
}

async fn watchdog<F>(description: &str, future: F) -> F::Output
where
    F: Future,
{
    timeout(WATCHDOG, future)
        .await
        .unwrap_or_else(|_| panic!("watchdog expired while waiting to {description}"))
}

async fn drain_fake_prelude(runtime: &CodexRuntime) {
    loop {
        let event = watchdog("drain fake harness prelude", runtime.next_event())
            .await
            .expect("fake ready event before terminal");
        if matches!(
            event,
            CodexEvent::Notification { ref method, .. } if method == "test/fake-ready"
        ) {
            return;
        }
    }
}

#[cfg(windows)]
fn windows_file_dacl_sddl(path: &std::path::Path) -> String {
    use std::os::windows::ffi::OsStrExt;

    use windows::{
        Win32::{
            Foundation::{ERROR_SUCCESS, HLOCAL, LocalFree},
            Security::{
                Authorization::{
                    ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW,
                    SDDL_REVISION_1, SE_FILE_OBJECT,
                },
                DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
            },
        },
        core::{PCWSTR, PWSTR},
    };

    let path_wide = path
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect::<Vec<_>>();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    let status = unsafe {
        GetNamedSecurityInfoW(
            PCWSTR(path_wide.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            None,
            None,
            &mut descriptor,
        )
    };
    assert_eq!(status, ERROR_SUCCESS, "read runtime file DACL");

    let mut text = PWSTR::null();
    unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            None,
        )
    }
    .expect("convert runtime file DACL to SDDL");
    let value = unsafe { text.to_string() }.expect("decode runtime file DACL SDDL");
    unsafe {
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
    }
    value
}

#[cfg(windows)]
fn windows_current_user_sid() -> String {
    use windows::{
        Win32::{
            Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree},
            Security::{
                Authorization::ConvertSidToStringSidW, GetTokenInformation, TOKEN_QUERY,
                TOKEN_USER, TokenUser,
            },
            System::Threading::{GetCurrentProcess, OpenProcessToken},
        },
        core::PWSTR,
    };

    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
        .expect("open current process token");
    let mut required = 0_u32;
    let _ = unsafe { GetTokenInformation(token, TokenUser, None, 0, &mut required) };
    assert!(required > 0, "current user token must report a SID size");
    let mut information = vec![0_u8; required as usize];
    unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            Some(information.as_mut_ptr().cast()),
            required,
            &mut required,
        )
    }
    .expect("read current user token");
    let token_user = unsafe { &*(information.as_ptr().cast::<TOKEN_USER>()) };
    let mut sid_text = PWSTR::null();
    unsafe { ConvertSidToStringSidW(token_user.User.Sid, &mut sid_text) }
        .expect("format current user SID");
    let sid = unsafe { sid_text.to_string() }.expect("decode current user SID");
    unsafe {
        let _ = LocalFree(Some(HLOCAL(sid_text.0.cast())));
        CloseHandle(token).expect("close current process token");
    }
    sid
}

static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(1);

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new() -> Self {
        let sequence = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "arcgis-pro-agent-codex-test-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create unique test directory");
        Self { path }
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

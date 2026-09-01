use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use arcgis_pro_agent_desktop_lib::mcp_status::{
    ArcGisMcpReadiness, FailureCategory, Lifecycle, StatusSource, arcgis_inventory_is_valid,
    mcp_status_list_params, parse_arcgis_status_notification,
};
use arcgis_pro_agent_desktop_lib::{
    app_state::{
        AccountSnapshot, ActiveViewSnapshot, BridgeSnapshot, BridgeStatus, CodexSnapshot,
        ContextExtentSnapshot, DesktopState, LayerSnapshot, PollGate, retain_stale_snapshot,
    },
    commands::{
        LoginStartResult, ServerRequestDisposition, classify_server_request, context_call_params,
        current_timestamp, health_call_params, list_layers_call_params, login_start_params,
        parse_account_snapshot, parse_context_response, parse_health_response,
        parse_layers_response, thread_start_params, tool_completion_event, turn_interrupt_params,
        turn_start_params, unsupported_server_response, validate_auth_url,
    },
    paths::resolve_mcp_command,
};
use serde_json::{Value, json};

#[test]
fn mcp_command_resolution_prefers_override_then_installed_sibling_then_bare_name() {
    let exe =
        Path::new(r"C:\Program Files\ArcGISProAgent\0.1.0\desktop\arcgis-pro-agent-desktop.exe");
    let sibling =
        PathBuf::from(r"C:\Program Files\ArcGISProAgent\0.1.0\mcp\ArcGISProAgent.Mcp.exe");
    assert_eq!(
        resolve_mcp_command(
            Some(OsStr::new(r"E:\custom path\mcp.exe")),
            Some(exe),
            |_| true
        ),
        PathBuf::from(r"E:\custom path\mcp.exe")
    );
    assert_eq!(
        resolve_mcp_command(None, Some(exe), |path| path == sibling),
        sibling
    );
    assert_eq!(
        resolve_mcp_command(None, Some(exe), |_| false),
        PathBuf::from("ArcGISProAgent.Mcp.exe")
    );
    assert_eq!(
        resolve_mcp_command(
            None,
            Some(Path::new(r"C:\repo\target\debug\desktop.exe")),
            |_| true
        ),
        PathBuf::from("ArcGISProAgent.Mcp.exe")
    );
}

#[test]
fn production_handler_exposes_no_deepseek_or_provider_switch_command() {
    let source = std::fs::read_to_string("src/lib.rs").unwrap();
    for forbidden in [
        "commands::provider_select",
        "commands::deepseek_configure",
        "commands::deepseek_clear",
    ] {
        assert!(
            !source.contains(forbidden),
            "{forbidden} must not be registered"
        );
    }
    assert!(source.contains("commands::rediscover_codex"));
}

#[test]
fn current_and_legacy_mcp_events_share_one_allowlisted_parser() {
    let params = json!({
        "name": "arcgis",
        "status": "failed",
        "threadId": "thread-1",
        "failureReason": "reauthenticationRequired",
        "error": "must-not-be-retained"
    });
    let current = parse_arcgis_status_notification("mcpServer/startupStatus/updated", &params)
        .expect("current event");
    let legacy = parse_arcgis_status_notification("mcpServer/status/updated", &params)
        .expect("legacy event");
    assert_eq!(current.source, StatusSource::Current);
    assert_eq!(current.lifecycle, Lifecycle::Failed);
    assert_eq!(
        current.failure,
        Some(FailureCategory::ReauthenticationRequired)
    );
    assert_eq!(legacy.source, StatusSource::Legacy);
    assert_eq!(legacy.failure, Some(FailureCategory::StartupFailed));
    assert!(!format!("{current:?}{legacy:?}").contains("must-not-be-retained"));
    assert!(parse_arcgis_status_notification("unknown", &params).is_none());
    assert!(
        parse_arcgis_status_notification(
            "mcpServer/startupStatus/updated",
            &json!({"name": "other", "status": "ready"}),
        )
        .is_none()
    );
    let unknown = parse_arcgis_status_notification(
        "mcpServer/startupStatus/updated",
        &json!({"name": "arcgis", "status": "future-state", "threadId": "thread-1"}),
    )
    .expect("recognized arcgis event must fail closed");
    assert_eq!(unknown.lifecycle, Lifecycle::Unknown);
    assert_eq!(unknown.failure, Some(FailureCategory::Unknown));
}

#[test]
fn runtime_routes_mcp_status_through_one_seam_before_account_self_test() {
    let commands_source = include_str!("../src/commands.rs");
    assert!(
        !commands_source.contains("set_arcgis_ready_for"),
        "notification routing must not bypass the leased MCP state machine"
    );
    assert!(
        commands_source.contains("CURRENT_STATUS_METHOD | LEGACY_STATUS_METHOD"),
        "both protocol event names must share one notification branch"
    );

    let restart = commands_source
        .find("let (runtime_epoch, old_runtime, starting) = state.begin_runtime_restart().await")
        .expect("reserve the replacement runtime epoch");
    let event_spawn = commands_source[restart..]
        .find("host.spawn_event_loop(runtime.clone(), runtime_epoch)")
        .map(|offset| restart + offset)
        .expect("spawn the event loop with the reserved epoch");
    let consumer_rendezvous = commands_source[restart..]
        .find("persistent_event_waiter_count()")
        .map(|offset| restart + offset)
        .expect("wait until the persistent event consumer is actually registered");
    let account_self_test = commands_source[restart..]
        .find(".request(\"account/read\"")
        .map(|offset| restart + offset)
        .expect("self-test account/read after starting the event consumer");
    let publication = commands_source[restart..]
        .find(".publish_runtime_ready(")
        .map(|offset| restart + offset)
        .expect("publish only after account/read succeeds");
    let health_spawn = commands_source[restart..]
        .find("host.spawn_health_poller(runtime_epoch)")
        .map(|offset| restart + offset)
        .expect("start the health poller");
    assert!(event_spawn < account_self_test);
    assert!(consumer_rendezvous < account_self_test);
    assert!(account_self_test < publication);
    assert!(publication < health_spawn);
    for fail_closed_contract in [
        "event_task.is_finished()",
        "event_task.abort()",
        "let _ = event_task.await",
        "publish_incompatible_if_current",
    ] {
        assert!(
            commands_source.contains(fail_closed_contract),
            "missing failed-consumer contract: {fail_closed_contract}"
        );
    }
}

#[test]
fn production_mcp_status_discovery_keeps_the_exact_ten_second_bound() {
    let commands_source = include_str!("../src/commands.rs");
    assert!(
        commands_source.contains("const MCP_STATUS_TIMEOUT: Duration = Duration::from_secs(10);")
    );
    assert!(
        commands_source
            .contains("refresh_mcp_status_with_timeout(state, client, lease, MCP_STATUS_TIMEOUT)")
    );
}

#[test]
fn readiness_requires_lifecycle_and_baseline_inventory_in_either_order() {
    let ready = parse_arcgis_status_notification(
        "mcpServer/startupStatus/updated",
        &json!({"name": "arcgis", "status": "ready", "threadId": "thread-1"}),
    )
    .unwrap();
    let mut first = ArcGisMcpReadiness::default();
    first.apply_status(ready.clone());
    assert!(!first.is_ready());
    first.apply_inventory(true);
    assert!(first.is_ready());

    let mut second = ArcGisMcpReadiness::default();
    second.apply_inventory(true);
    assert!(!second.is_ready());
    second.apply_status(ready);
    assert!(second.is_ready());
}

#[test]
fn current_status_has_precedence_and_duplicate_updates_are_idempotent() {
    let mut readiness = ArcGisMcpReadiness::default();
    let current = parse_arcgis_status_notification(
        "mcpServer/startupStatus/updated",
        &json!({"name": "arcgis", "status": "failed", "failureReason": "reauthenticationRequired"}),
    )
    .unwrap();
    assert!(readiness.apply_status(current.clone()));
    assert!(!readiness.apply_status(current));
    let legacy = parse_arcgis_status_notification(
        "mcpServer/status/updated",
        &json!({"name": "arcgis", "status": "ready"}),
    )
    .unwrap();
    assert!(!readiness.apply_status(legacy));
    assert_eq!(
        readiness.failure(),
        Some(FailureCategory::ReauthenticationRequired)
    );
    assert!(!readiness.is_ready());
}

#[test]
fn list_params_and_inventory_require_only_the_phase_2_baseline() {
    assert_eq!(
        mcp_status_list_params("thread-1"),
        json!({
            "threadId": "thread-1",
            "detail": "toolsAndAuthOnly"
        })
    );
    assert!(arcgis_inventory_is_valid(&json!({"data": [{
        "name": "arcgis",
        "tools": {
            "arcgis_connection_status": {},
            "arcgis_describe_context": {},
            "arcgis_list_layers": {},
            "future_extra_tool": {}
        }
    }]})));
    assert!(!arcgis_inventory_is_valid(&json!({"data": [{
        "name": "arcgis",
        "tools": {"arcgis_connection_status": {}}
    }]})));
}

#[test]
fn auth_url_accepts_only_exact_https_hosts_without_userinfo_or_ports() {
    for url in [
        "https://auth.openai.com/oauth/authorize",
        "https://chatgpt.com/auth/login?continue=%2F",
        "https://openai.com/login",
    ] {
        assert!(
            validate_auth_url(url).is_ok(),
            "expected official URL: {url}"
        );
    }

    for url in [
        "http://auth.openai.com/oauth/authorize",
        "https://auth.openai.com.evil.example/oauth/authorize",
        "https://evil.example/?next=https://auth.openai.com/",
        "https://auth.openai.com@evil.example/oauth/authorize",
        "https://user@auth.openai.com/oauth/authorize",
        "https://auth.openai.com:443/oauth/authorize",
        "https://chatgpt.com.evil.example/",
        "file:///C:/secrets.txt",
    ] {
        assert!(validate_auth_url(url).is_err(), "expected rejection: {url}");
    }
}

#[test]
fn login_start_uses_only_the_official_chatgpt_flow() {
    assert_eq!(
        login_start_params(),
        json!({
            "type": "chatgpt",
            "codexStreamlinedLogin": true,
            "useHostedLoginSuccessPage": true,
            "appBrand": "codex"
        })
    );
}

#[test]
fn login_result_exposes_only_login_id_and_validated_auth_url() {
    let result = LoginStartResult::from_response(json!({
        "type": "chatgpt",
        "loginId": "login-1",
        "authUrl": "https://auth.openai.com/oauth/authorize",
        "accessToken": "must-not-leak",
        "cookie": "must-not-leak"
    }))
    .expect("valid ChatGPT login response");

    assert_eq!(
        serde_json::to_value(result).expect("serialize login result"),
        json!({
            "loginId": "login-1",
            "authUrl": "https://auth.openai.com/oauth/authorize"
        })
    );
}

#[test]
fn account_mapping_supports_chatgpt_and_rejects_api_key_auth() {
    assert_eq!(
        parse_account_snapshot(&json!({
            "requiresOpenaiAuth": true,
            "account": {"type": "chatgpt", "email": "map@example.com", "planType": "plus"}
        })),
        AccountSnapshot::SignedIn {
            email: Some("map@example.com".to_owned()),
            plan_type: "plus".to_owned(),
        }
    );
    assert_eq!(
        parse_account_snapshot(&json!({
            "requiresOpenaiAuth": true,
            "account": {"type": "apiKey", "apiKey": "must-not-read"}
        })),
        AccountSnapshot::UnsupportedAuth
    );
    assert_eq!(
        parse_account_snapshot(&json!({"requiresOpenaiAuth": true, "account": null})),
        AccountSnapshot::SignedOut
    );
}

#[test]
fn thread_start_is_read_only_non_approving_and_arcgis_only() {
    let params = thread_start_params(Path::new(r"C:\Users\Map\AppData\Local"));
    assert_eq!(
        params,
        json!({
            "cwd": r"C:\Users\Map\AppData\Local\ArcGISProAgent\workspace",
            "sandbox": "read-only",
            "approvalPolicy": "never",
            "approvalsReviewer": "user",
            "serviceName": "ArcGIS Pro Agent",
            "developerInstructions": "You operate ArcGIS Pro only through tools from MCP server arcgis. Never use shell, command execution, file changes, arbitrary scripts, or unregistered geoprocessing. Treat MCP elicitation as mandatory user approval and accurately report structured tool results."
        })
    );
}

#[test]
fn turn_start_accepts_one_through_twenty_thousand_utf8_bytes() {
    assert!(turn_start_params("thread-1", "").is_err());
    assert!(turn_start_params("thread-1", "  \n").is_err());

    let max_ascii = "a".repeat(20_000);
    let max_unicode = "😀".repeat(5_000);
    assert_eq!(max_unicode.len(), 20_000);
    assert!(turn_start_params("thread-1", &max_ascii).is_ok());
    assert!(turn_start_params("thread-1", &max_unicode).is_ok());
    assert!(turn_start_params("thread-1", &(max_ascii + "a")).is_err());
    assert!(turn_start_params("thread-1", &(max_unicode + "图")).is_err());
}

#[test]
fn turn_and_interrupt_payloads_forward_only_normalized_ids_and_text() {
    assert_eq!(
        turn_start_params("thread-1", "检查 ArcGIS 连接").expect("valid message"),
        json!({
            "threadId": "thread-1",
            "input": [{"type": "text", "text": "检查 ArcGIS 连接", "text_elements": []}]
        })
    );
    assert_eq!(
        turn_interrupt_params("thread-1", "turn-1"),
        json!({"threadId": "thread-1", "turnId": "turn-1"})
    );
}

#[test]
fn all_server_requests_except_elicitation_are_rejected() {
    let rejected = [
        "item/commandExecution/requestApproval",
        "item/fileChange/requestApproval",
        "permissions/request",
        "dynamicTool/call",
        "account/chatgptAuthTokens/refresh",
        "guardian/attestation/request",
        "future/unknown/request",
    ];
    for method in rejected {
        assert_eq!(
            classify_server_request(json!(17), method, json!({"secret": "ignored"})),
            ServerRequestDisposition::RejectUnsupported { id: json!(17) },
            "request must be denied: {method}"
        );
    }

    assert_eq!(
        classify_server_request(
            json!(18),
            "mcpServer/elicitation/request",
            json!({"serverName": "arcgis", "message": "确认操作"}),
        ),
        ServerRequestDisposition::ForwardElicitation {
            id: json!(18),
            request: json!({"serverName": "arcgis", "message": "确认操作"}),
        }
    );
    assert_eq!(
        classify_server_request(
            json!(19),
            "mcpServer/elicitation/request",
            json!({"serverName": "unregistered", "message": "确认操作"}),
        ),
        ServerRequestDisposition::RejectUnsupported { id: json!(19) }
    );
    assert_eq!(
        unsupported_server_response(json!(17)),
        json!({
            "id": 17,
            "error": {"code": -32601, "message": "Unsupported server request"}
        })
    );
}

#[test]
fn health_payload_is_scoped_to_the_active_arcgis_thread() {
    assert_eq!(
        health_call_params("thread-1"),
        json!({
            "threadId": "thread-1",
            "server": "arcgis",
            "tool": "arcgis_connection_status",
            "arguments": {}
        })
    );
}

fn expected_health() -> BridgeSnapshot {
    BridgeSnapshot {
        status: BridgeStatus::Connected,
        context_is_live: false,
        protocol_version: Some("1.0".to_owned()),
        add_in_version: Some("0.1.0".to_owned()),
        arc_gis_pro_version: Some("3.5.2".to_owned()),
        project_name: Some("城市规划".to_owned()),
        project_has_unsaved_changes: None,
        active_map_name: Some("中心城区".to_owned()),
        active_view: None,
        layers: vec![],
        last_updated: Some("2026-07-19T12:00:00Z".to_owned()),
        error: None,
    }
}

fn health_json() -> Value {
    json!({
        "connected": true,
        "protocolVersion": "1.0",
        "addInVersion": "0.1.0",
        "arcGisProVersion": "3.5.2",
        "projectName": "城市规划",
        "activeMapName": "中心城区",
        "capabilities": []
    })
}

#[test]
fn health_parses_structured_content_or_the_first_json_text_content() {
    assert_eq!(
        parse_health_response(
            &json!({"structuredContent": health_json(), "content": []}),
            "2026-07-19T12:00:00Z",
        )
        .expect("structured health"),
        expected_health()
    );
    assert_eq!(
        parse_health_response(
            &json!({
                "content": [
                    {"type": "text", "text": serde_json::to_string(&health_json()).unwrap()}
                ]
            }),
            "2026-07-19T12:00:00Z",
        )
        .expect("text health"),
        expected_health()
    );
}

#[test]
fn failed_health_refresh_retains_last_success_as_non_live_and_redacts_details() {
    let stale = retain_stale_snapshot(
        &expected_health(),
        r"Bearer secret-token at C:\Users\Alice\private\runtime.json",
    );
    assert_eq!(stale.status, BridgeStatus::Disconnected);
    assert_eq!(stale.project_name.as_deref(), Some("城市规划"));
    assert_eq!(stale.active_map_name.as_deref(), Some("中心城区"));
    assert_eq!(stale.last_updated.as_deref(), Some("2026-07-19T12:00:00Z"));
    assert_eq!(stale.error.as_deref(), Some("ArcGIS 连接检查失败"));
    let serialized = serde_json::to_string(&stale).expect("serialize stale snapshot");
    assert!(!serialized.contains("secret-token"));
    assert!(!serialized.contains("Alice"));
}

#[test]
fn health_timestamp_is_rfc3339_utc() {
    let timestamp = current_timestamp();
    let parsed =
        chrono::DateTime::parse_from_rfc3339(&timestamp).expect("timestamp must be RFC3339");
    assert_eq!(parsed.offset().local_minus_utc(), 0);
    assert!(timestamp.ends_with('Z'));
}

#[test]
fn polling_requires_visibility_arcgis_readiness_and_an_active_thread_then_cancels() {
    let gate = PollGate::new();
    assert!(!gate.should_poll(false, true, Some("thread-1")));
    assert!(!gate.should_poll(true, false, Some("thread-1")));
    assert!(!gate.should_poll(true, true, None));
    assert!(gate.should_poll(true, true, Some("thread-1")));

    gate.cancel();
    assert!(gate.is_cancelled());
    assert!(!gate.should_poll(true, true, Some("thread-1")));
}

#[test]
fn context_payloads_are_exact_and_request_nested_layers_only() {
    assert_eq!(
        context_call_params("thread-ctx"),
        json!({
            "threadId": "thread-ctx",
            "server": "arcgis",
            "tool": "arcgis_describe_context",
            "arguments": {}
        })
    );
    assert_eq!(
        list_layers_call_params("thread-ctx"),
        json!({
            "threadId": "thread-ctx",
            "server": "arcgis",
            "tool": "arcgis_list_layers",
            "arguments": {"includeNested": true}
        })
    );
}

fn safe_context_json() -> Value {
    json!({
        "project": {
            "name": "城市规划",
            "path": r"C:\Users\Alice\secret\城市规划.aprx",
            "hasUnsavedChanges": true,
            "items": [
                {"uri": "map://main", "name": "中心城区", "kind": "map", "isActive": true}
            ],
            "apiToken": "sk-proj-secret",
            "email": "leak@example.com"
        },
        "activeView": {
            "uri": "map://main",
            "name": "中心城区",
            "kind": "map",
            "extent": {"xMin": 1.25, "yMin": 2.5, "xMax": 9.75, "yMax": 10.0, "wkid": 4326},
            "connectionString": "Server=private;Password=secret"
        },
        "rawResultText": "feature value MUST_NOT_LEAK",
        "sql": "OWNER = 'Alice'"
    })
}

fn expected_active_view() -> ActiveViewSnapshot {
    ActiveViewSnapshot {
        uri: "map://main".to_owned(),
        name: "中心城区".to_owned(),
        kind: "map".to_owned(),
        extent: Some(ContextExtentSnapshot {
            x_min: 1.25,
            y_min: 2.5,
            x_max: 9.75,
            y_max: 10.0,
            wkid: Some(4326),
        }),
    }
}

#[test]
fn context_parses_structured_before_text_and_drops_project_path_and_unknown_fields() {
    let structured = parse_context_response(&json!({
        "structuredContent": safe_context_json(),
        "content": [{"type": "text", "text": "{\"project\":null}"}]
    }))
    .expect("structured context");
    let text = parse_context_response(&json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string(&safe_context_json()).unwrap()
        }]
    }))
    .expect("text context");

    for parsed in [structured, text] {
        assert_eq!(parsed.project_name.as_deref(), Some("城市规划"));
        assert_eq!(parsed.project_has_unsaved_changes, Some(true));
        assert_eq!(parsed.active_view, Some(expected_active_view()));
        let serialized = serde_json::to_string(&parsed).expect("serialize redacted context");
        for forbidden in [
            "Alice",
            "城市规划.aprx",
            "sk-proj-secret",
            "leak@example.com",
            "Server=private",
            "MUST_NOT_LEAK",
            "OWNER =",
            "path",
        ] {
            assert!(
                !serialized.contains(forbidden),
                "leaked {forbidden}: {serialized}"
            );
        }
    }
}

#[test]
fn nested_layers_parse_only_the_bounded_allowlist_from_structured_or_text() {
    let layers = json!({
        "layers": [
            {
                "uri": "layer://group",
                "name": "基础设施",
                "longName": "基础设施",
                "layerType": "GroupLayer",
                "parentUri": null,
                "depth": 0,
                "visible": true,
                "isFeatureLayer": false,
                "sourcePath": r"\\server\share\secret.gdb",
                "connectionString": "Password=hunter2"
            },
            {
                "uri": "layer://roads",
                "name": "道路",
                "longName": "基础设施\\道路",
                "layerType": "FeatureLayer",
                "parentUri": "layer://group",
                "depth": 1,
                "visible": false,
                "isFeatureLayer": true,
                "featureValues": [{"OWNER": "Alice"}],
                "matchedObjectIds": [7, 8, 9]
            }
        ],
        "rawResultText": "SELECT * FROM private"
    });
    let expected = vec![
        LayerSnapshot {
            uri: "layer://group".to_owned(),
            name: "基础设施".to_owned(),
            long_name: "基础设施".to_owned(),
            layer_type: "GroupLayer".to_owned(),
            parent_uri: None,
            depth: 0,
            visible: true,
            is_feature_layer: false,
        },
        LayerSnapshot {
            uri: "layer://roads".to_owned(),
            name: "道路".to_owned(),
            long_name: "基础设施\\道路".to_owned(),
            layer_type: "FeatureLayer".to_owned(),
            parent_uri: Some("layer://group".to_owned()),
            depth: 1,
            visible: false,
            is_feature_layer: true,
        },
    ];

    for response in [
        json!({"structuredContent": layers.clone()}),
        json!({"content": [{"type": "text", "text": serde_json::to_string(&layers).unwrap()}]}),
    ] {
        let parsed = parse_layers_response(&response).expect("bounded layer summary");
        assert_eq!(parsed, expected);
        let serialized = serde_json::to_string(&parsed).expect("serialize layers");
        for forbidden in ["server", "hunter2", "Alice", "7", "SELECT"] {
            assert!(
                !serialized.contains(forbidden),
                "leaked {forbidden}: {serialized}"
            );
        }
    }
}

#[test]
fn malformed_or_oversized_context_and_layer_fields_fail_closed() {
    let long_name = "界".repeat(257);
    let long_uri = format!("map://{}", "u".repeat(2_001));
    let too_many_items: Vec<Value> = (0..101)
        .map(|index| json!({"uri": format!("map://{index}"), "name": "map", "kind": "map", "isActive": false}))
        .collect();
    let too_many_layers: Vec<Value> = (0..201)
        .map(|index| {
            json!({
                "uri": format!("layer://{index}"), "name": "layer", "longName": "layer",
                "layerType": "FeatureLayer", "parentUri": null, "depth": 0,
                "visible": true, "isFeatureLayer": true
            })
        })
        .collect();

    for response in [
        json!({"structuredContent": {"project": {"name": long_name, "path": null, "hasUnsavedChanges": false, "items": []}, "activeView": null}}),
        json!({"structuredContent": {"project": {"name": "project", "path": null, "hasUnsavedChanges": false, "items": []}, "activeView": {"uri": long_uri, "name": "map", "kind": "map", "extent": null}}}),
        json!({"structuredContent": {"project": {"name": "project", "path": null, "hasUnsavedChanges": false, "items": too_many_items}, "activeView": null}}),
        json!({"structuredContent": {"project": {"name": "project", "path": null, "hasUnsavedChanges": false, "items": []}, "activeView": {"uri": "map://main", "name": "map", "kind": "map", "extent": {"xMin": "NaN", "yMin": 0, "xMax": 1, "yMax": 1, "wkid": 4326}}}}),
    ] {
        assert!(parse_context_response(&response).is_err());
    }
    assert!(
        parse_layers_response(&json!({"structuredContent": {"layers": too_many_layers}})).is_err()
    );
    assert!(
        parse_context_response(&json!({"structuredContent": {
            "project": null,
            "activeView": null,
            "unknownPadding": "x".repeat(600_000)
        }}))
        .is_err(),
        "oversized unknown structured fields must not bypass the response bound"
    );
}

#[test]
fn structured_payload_budget_is_early_stopping_and_borrowed_locally() {
    let commands_source = include_str!("../src/commands.rs");
    let missing_contracts = [
        (
            commands_source.contains("Cow<'a, Value>"),
            "structured MCP payloads must be borrowed",
        ),
        (
            commands_source.contains("serde_json::to_writer"),
            "structured JSON must stream into a limited writer",
        ),
        (
            !commands_source.contains("serde_json::to_vec(structured)"),
            "the local budget must not allocate a second full JSON buffer",
        ),
        (
            commands_source.contains("already allocated upstream"),
            "the unavoidable upstream Value allocation must be documented",
        ),
    ]
    .into_iter()
    .filter_map(|(present, message)| (!present).then_some(message))
    .collect::<Vec<_>>();

    assert!(
        missing_contracts.is_empty(),
        "missing limited structured payload contracts: {missing_contracts:?}"
    );

    assert!(
        parse_health_response(
            &json!({
                "structuredContent": {
                    "connected": true,
                    "unknownPadding": "x".repeat(600_000)
                }
            }),
            "2026-07-29T01:02:03Z",
        )
        .is_err(),
        "health structured content must use the same local byte budget"
    );

    let oversized_tool = tool_completion_event(&json!({
        "item": {
            "type": "mcpToolCall",
            "server": "arcgis",
            "tool": "arcgis_count_features",
            "status": "completed",
            "structuredContent": {
                "count": 7,
                "unknownPadding": "x".repeat(600_000)
            }
        }
    }))
    .expect("bounded tool event");
    assert_eq!(oversized_tool["item"]["outcome"], "succeeded");
    assert!(
        oversized_tool["item"].get("summary").is_none(),
        "oversized structured tool content must not be traversed"
    );

    let mut deeply_nested = json!(true);
    for _ in 0..66 {
        deeply_nested = json!([deeply_nested]);
    }
    assert!(
        parse_health_response(
            &json!({
                "structuredContent": {
                    "connected": true,
                    "unknownDeepTree": deeply_nested
                }
            }),
            "2026-07-29T01:02:03Z",
        )
        .is_err(),
        "deep structured values must be rejected before recursive serialization"
    );
}

#[test]
fn structured_payload_rejects_high_node_values_before_serialization() {
    assert!(
        parse_health_response(
            &json!({
                "structuredContent": {
                    "connected": true,
                    "unknownWideTree": vec![Value::Null; 20_001]
                }
            }),
            "2026-07-29T01:02:03Z",
        )
        .is_err(),
        "high-node structured values must be rejected before full traversal"
    );
}

#[test]
fn completed_tool_events_are_new_allowlisted_redacted_values() {
    let event = tool_completion_event(&json!({
        "item": {
            "type": "mcpToolCall",
            "server": "arcgis",
            "tool": "arcgis_query_features",
            "status": "completed",
            "durationMs": 37,
            "arguments": {"where": "OWNER = 'Alice'", "apiToken": "sk-proj-secret"},
            "result": {
                "content": [{"type": "text", "text": "raw feature Secret Bakery"}],
                "structuredContent": {
                    "count": 2,
                    "hasMore": false,
                    "records": [{"OID": 7, "NAME": "Secret Bakery"}],
                    "sourcePath": r"\\server\share\private.gdb",
                    "email": "leak@example.com",
                    "connectionString": "Password=hunter2"
                }
            }
        }
    }))
    .expect("safe ArcGIS event");
    assert_eq!(
        event,
        json!({
            "type": "item/completed",
            "item": {
                "type": "mcpToolCall",
                "server": "arcgis",
                "tool": "arcgis_query_features",
                "risk": "R0",
                "outcome": "succeeded",
                "durationMs": 37,
                "summary": "count=2, hasMore=false"
            }
        })
    );
    let serialized = serde_json::to_string(&event).expect("serialize safe event");
    for forbidden in [
        "arguments",
        "result",
        "content",
        "structuredContent",
        "Alice",
        "sk-proj",
        "Secret Bakery",
        "server\\share",
        "leak@example.com",
        "hunter2",
        "OID",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "leaked {forbidden}: {serialized}"
        );
    }

    let failed = tool_completion_event(&json!({
        "item": {
            "type": "mcpToolCall", "server": "arcgis", "tool": "arcgis_zoom_to_layer",
            "status": "failed", "durationMs": 9,
            "result": {"structuredContent": {"error": {"code": "no_active_view", "message": r"C:\private\map.aprx"}}}
        }
    }))
    .expect("safe failure event");
    assert_eq!(failed["item"]["risk"], "R1");
    assert_eq!(failed["item"]["outcome"], "failed");
    assert_eq!(failed["item"]["errorCode"], "no_active_view");
    assert!(failed["item"].get("summary").is_none());

    let unknown = tool_completion_event(&json!({
        "item": {"type": "mcpToolCall", "server": "arcgis", "tool": "arcgis_future_tool", "status": "pending"}
    }))
    .expect("bounded unknown tool event");
    assert_eq!(unknown["item"]["risk"], "unknown");
    assert_eq!(unknown["item"]["outcome"], "unknown");

    let invalid_code = tool_completion_event(&json!({
        "item": {"type": "mcpToolCall", "server": "arcgis", "tool": "arcgis_count_features", "status": "failed", "result": {"structuredContent": {"errorCode": "SECRET/path"}}}
    }))
    .expect("event without invalid code");
    assert!(invalid_code["item"].get("errorCode").is_none());
}

#[test]
fn top_level_structured_tool_errors_drive_one_canonical_safe_view() {
    let event = tool_completion_event(&json!({
        "item": {
            "type": "mcpToolCall",
            "server": "arcgis",
            "tool": "arcgis_count_features",
            "status": "completed",
            "structuredContent": {
                "count": 4,
                "error": {
                    "code": "query_failed",
                    "message": r"C:\private\secret.gdb"
                }
            }
        }
    }))
    .expect("safe top-level structured failure");

    assert_eq!(event["item"]["outcome"], "failed");
    assert_eq!(event["item"]["summary"], "count=4");
    assert_eq!(event["item"]["errorCode"], "query_failed");
    assert!(
        !serde_json::to_string(&event)
            .unwrap()
            .contains("secret.gdb")
    );

    let top_level_is_error = tool_completion_event(&json!({
        "item": {
            "type": "mcpToolCall",
            "server": "arcgis",
            "tool": "arcgis_count_features",
            "status": "completed",
            "isError": true,
            "structuredContent": {"count": 0}
        }
    }))
    .expect("safe top-level isError failure");
    assert_eq!(top_level_is_error["item"]["outcome"], "failed");
}

#[test]
fn null_result_structured_content_does_not_shadow_a_safe_top_level_error() {
    let event = tool_completion_event(&json!({
        "item": {
            "type": "mcpToolCall",
            "server": "arcgis",
            "tool": "arcgis_count_features",
            "status": "completed",
            "result": {"structuredContent": null},
            "structuredContent": {
                "error": {"code": "query_failed", "message": "private detail"}
            }
        }
    }))
    .expect("safe top-level fallback after null result content");

    assert_eq!(event["item"]["outcome"], "failed");
    assert_eq!(event["item"]["errorCode"], "query_failed");
    assert!(
        !serde_json::to_string(&event)
            .unwrap()
            .contains("private detail")
    );
}

#[test]
fn oversized_result_structured_content_does_not_shadow_a_safe_top_level_error() {
    let event = tool_completion_event(&json!({
        "item": {
            "type": "mcpToolCall",
            "server": "arcgis",
            "tool": "arcgis_count_features",
            "status": "completed",
            "result": {
                "structuredContent": {"unknownPadding": "x".repeat(600_000)}
            },
            "structuredContent": {
                "error": {"code": "query_failed", "message": "private detail"}
            }
        }
    }))
    .expect("safe top-level fallback after oversized result content");

    assert_eq!(event["item"]["outcome"], "failed");
    assert_eq!(event["item"]["errorCode"], "query_failed");
    assert!(
        !serde_json::to_string(&event)
            .unwrap()
            .contains("private detail")
    );
}

#[tokio::test]
async fn desktop_state_tracks_only_app_owned_conversation_and_snapshot_state() {
    let state = DesktopState::new(Path::new(r"C:\Users\Map\AppData\Local").to_owned());
    let initial = state.snapshot().await;
    assert_eq!(initial.account, AccountSnapshot::Checking);
    assert_eq!(initial.codex, CodexSnapshot::Starting);
    assert_eq!(state.active_ids().await, (None, None));

    state.set_active_thread("thread-1".to_owned()).await;
    state.set_active_turn("turn-1".to_owned()).await;
    assert_eq!(
        state.active_ids().await,
        (Some("thread-1".to_owned()), Some("turn-1".to_owned()))
    );

    state.clear_conversation().await;
    assert_eq!(state.active_ids().await, (None, None));
    assert_eq!(
        state.local_app_data(),
        Path::new(r"C:\Users\Map\AppData\Local")
    );
}

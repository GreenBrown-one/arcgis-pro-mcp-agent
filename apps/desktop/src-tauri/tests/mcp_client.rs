use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use arcgis_pro_agent_desktop_lib::{
    arcgis_tool_client::ArcGisToolClient,
    mcp::{McpRuntime, resolve_private_mcp_command},
};
use serde_json::{Value, json};
use tokio::time::timeout;

const FAKE_SCENARIO_ENV: &str = "ARCGIS_MCP_FAKE_SCENARIO";
const FAKE_CAPTURE_ENV: &str = "ARCGIS_MCP_FAKE_CAPTURE";
const FAKE_SECRET_ENV: &str = "ARCGIS_MCP_FAKE_SECRET";
const WATCHDOG: Duration = Duration::from_secs(10);

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn direct_mcp_initializes_lists_baseline_routes_out_of_order_times_out_and_redacts_stderr() {
    let directory = TestDir::new();
    let capture = directory.path.join("received.jsonl");
    let runtime_token = "runtime-token-must-not-leak";
    let runtime_file = directory.path.join("bridge.json");
    std::fs::write(&runtime_file, json!({"token": runtime_token}).to_string())
        .expect("write fake runtime credential");
    let mut command = fake_command("contract", &capture);
    command.env(FAKE_SECRET_ENV, runtime_token);
    command.env("ARCGIS_AGENT_RUNTIME_FILE", &runtime_file);
    let runtime = timeout(WATCHDOG, McpRuntime::start_with_command(command))
        .await
        .expect("starting fake MCP server must not hang")
        .expect("MCP initialization and inventory validation");

    let tools = runtime.list_tools().await.expect("list baseline tools");
    assert_eq!(
        tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>(),
        BASELINE_TOOLS
    );

    let (first, second) = timeout(WATCHDOG, async {
        tokio::join!(
            runtime.call_tool("arcgis_connection_status", json!({"request": 1})),
            runtime.call_tool("arcgis_describe_context", json!({"request": 2}))
        )
    })
    .await
    .expect("out-of-order MCP replies must not hang");
    assert_eq!(
        first.expect("first tool reply").structured_content,
        json!({"request": 1})
    );
    assert_eq!(
        second.expect("second tool reply").structured_content,
        json!({"request": 2})
    );

    let hanging_started = std::time::Instant::now();
    let hanging = timeout(
        Duration::from_secs(6),
        runtime.call_tool("arcgis_list_layers", json!({"hang": true})),
    )
    .await
    .expect("MCP call timeout must be bounded");
    assert!(
        hanging.is_err(),
        "hanging MCP call must fail after five seconds"
    );
    let hanging_elapsed = hanging_started.elapsed();
    assert!(
        (Duration::from_millis(4_500)..Duration::from_secs(6)).contains(&hanging_elapsed),
        "hanging MCP call must time out near five seconds, got {hanging_elapsed:?}"
    );

    assert!(
        runtime
            .stderr_lines()
            .await
            .iter()
            .all(|line| !line.contains(runtime_token)),
        "stderr diagnostics must redact the runtime token"
    );
    runtime.shutdown().await.expect("shutdown fake MCP server");

    let received = read_json_lines(&capture);
    assert_eq!(
        received.iter().take(3).cloned().collect::<Vec<_>>(),
        vec![
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"arcgis-pro-agent-desktop","version":"0.2.0-preview.1"}}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn protocol_frames_fail_pending_requests_before_the_call_timeout_and_terminate_the_child() {
    let directory = TestDir::new();
    let capture = directory.path.join("received.jsonl");
    let runtime = start_fake_runtime("malformed", &capture).await;
    let started = std::time::Instant::now();
    let error = runtime
        .call_tool("arcgis_connection_status", json!({"protocol": true}))
        .await
        .expect_err("malformed stdout must fail the pending request");
    assert_eq!(error.to_string(), "ArcGIS MCP protocol failed");
    assert!(started.elapsed() < Duration::from_secs(2));
    runtime
        .shutdown()
        .await
        .expect("termination acknowledgement");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_utf8_and_oversized_stdout_frames_fail_closed() {
    for scenario in ["invalid-utf8", "oversized"] {
        let directory = TestDir::new();
        let runtime = start_fake_runtime(scenario, &directory.path.join("received.jsonl")).await;
        let started = std::time::Instant::now();
        let error = runtime
            .call_tool("arcgis_connection_status", json!({"protocol": scenario}))
            .await
            .expect_err("invalid protocol frame");
        assert_eq!(error.to_string(), "ArcGIS MCP protocol failed");
        assert!(started.elapsed() < Duration::from_secs(2));
        runtime
            .shutdown()
            .await
            .expect("termination acknowledgement");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_is_bounded_when_a_non_reading_child_blocks_the_writer() {
    let directory = TestDir::new();
    let capture = directory.path.join("received.jsonl");
    let runtime = std::sync::Arc::new(start_fake_runtime("blocked-shutdown", &capture).await);
    for _ in 0..(64 + 4) {
        let runtime = runtime.clone();
        tokio::spawn(async move {
            let _ = runtime
                .call_tool(
                    "arcgis_connection_status",
                    json!({"payload": "x".repeat(256 * 1024)}),
                )
                .await;
        });
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    timeout(Duration::from_secs(2), runtime.shutdown())
        .await
        .expect("shutdown must remain bounded")
        .expect("shutdown observes child termination");
    let pid = std::fs::read_to_string(capture.with_extension("pid"))
        .expect("fake child PID")
        .trim()
        .parse::<u32>()
        .expect("fake child PID is numeric");
    assert!(
        !process_is_running(pid),
        "shutdown must wait for child exit"
    );
}

async fn start_fake_runtime(scenario: &str, capture: &Path) -> McpRuntime {
    let mut command = fake_command(scenario, capture);
    command.env(FAKE_SECRET_ENV, "test-runtime-token");
    timeout(WATCHDOG, McpRuntime::start_with_command(command))
        .await
        .expect("fake start must not hang")
        .expect("fake start")
}

#[test]
fn production_resolver_accepts_only_the_packaged_private_mcp_executable() {
    let desktop = Path::new(r"C:\Program Files\ArcGISProAgent\0.2.0-preview.1\desktop\app.exe");
    let expected =
        Path::new(r"C:\Program Files\ArcGISProAgent\0.2.0-preview.1\mcp\ArcGISProAgent.Mcp.exe");
    assert_eq!(
        resolve_private_mcp_command(Some(desktop), |path| path == expected)
            .expect("packaged private MCP executable"),
        expected
    );
    assert!(resolve_private_mcp_command(Some(desktop), |_| false).is_err());
    assert!(
        resolve_private_mcp_command(Some(Path::new(r"C:\repo\target\debug\app.exe")), |_| true,)
            .is_err()
    );
    let release_source = include_str!("../src/mcp/client.rs");
    assert!(!release_source.contains("pub mcp_command"));
    assert!(!release_source.contains("pub mcp_args"));
}

#[test]
#[ignore]
fn fake_mcp_server_process() {
    let Some(scenario) = std::env::var_os(FAKE_SCENARIO_ENV) else {
        return;
    };
    let capture = PathBuf::from(std::env::var_os(FAKE_CAPTURE_ENV).expect("fake capture path"));
    run_fake_server(&scenario.to_string_lossy(), &capture);
}

fn fake_command(scenario: &str, capture: &Path) -> Command {
    let mut command = Command::new(std::env::current_exe().expect("integration test executable"));
    command
        .arg("--ignored")
        .arg("--exact")
        .arg("fake_mcp_server_process")
        .arg("--quiet")
        .env(FAKE_SCENARIO_ENV, scenario)
        .env(FAKE_CAPTURE_ENV, capture)
        .env("ARCGIS_MCP_TEST_HARNESS", "1");
    command
}

fn run_fake_server(scenario: &str, capture: &Path) {
    assert!(matches!(
        scenario,
        "contract" | "malformed" | "invalid-utf8" | "oversized" | "blocked-shutdown"
    ));
    std::fs::write(
        capture.with_extension("pid"),
        std::process::id().to_string(),
    )
    .expect("write fake child PID");
    let mut capture = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(capture)
        .expect("capture");
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    let mut delayed = Vec::new();
    for line in BufReader::new(stdin.lock()).lines() {
        let line = line.expect("read JSONL frame");
        writeln!(capture, "{line}").expect("capture JSONL frame");
        capture.flush().expect("flush capture");
        let message: Value = serde_json::from_str(&line).expect("client JSON frame");
        match message["method"].as_str() {
            Some("initialize") => write_json_line(
                &mut stdout,
                json!({"jsonrpc":"2.0","id":message["id"],"result":{"protocolVersion":"2025-11-25","capabilities":{},"serverInfo":{"name":"fake"}}}),
            ),
            Some("tools/list") => write_json_line(
                &mut stdout,
                json!({"jsonrpc":"2.0","id":message["id"],"result":{"tools":BASELINE_TOOLS.iter().map(|name| json!({"name":name,"description":"fake","inputSchema":{"type":"object"}})).collect::<Vec<_>>()}}),
            ),
            Some("tools/call") if scenario == "malformed" => {
                stdout.write_all(b"{malformed\n").expect("malformed stdout");
                stdout.flush().expect("flush malformed stdout");
            }
            Some("tools/call") if scenario == "invalid-utf8" => {
                stdout
                    .write_all(&[0xff, b'\n'])
                    .expect("invalid UTF-8 stdout");
                stdout.flush().expect("flush invalid UTF-8 stdout");
            }
            Some("tools/call") if scenario == "oversized" => {
                stdout
                    .write_all(&vec![b'x'; 1_048_577])
                    .expect("oversized stdout");
                stdout.write_all(b"\n").expect("oversized terminator");
                stdout.flush().expect("flush oversized stdout");
            }
            Some("tools/call") => {
                let arguments = message["params"]["arguments"].clone();
                if arguments.get("hang").and_then(Value::as_bool) == Some(true) {
                    continue;
                }
                delayed.push((message["id"].clone(), arguments));
                if delayed.len() == 2 {
                    for (id, arguments) in delayed.drain(..).rev() {
                        write_json_line(
                            &mut stdout,
                            json!({"jsonrpc":"2.0","id":id,"result":{"structuredContent":arguments,"content":[]}}),
                        );
                    }
                }
            }
            Some("notifications/initialized") => {
                let secret = std::env::var(FAKE_SECRET_ENV).expect("fake secret");
                writeln!(stderr, "runtime={secret}").expect("stderr");
                stderr.flush().expect("flush stderr");
            }
            other => panic!("unexpected method: {other:?}"),
        }
        if scenario == "blocked-shutdown" && message["method"] == "tools/list" {
            std::thread::sleep(Duration::from_secs(30));
        }
    }
}

fn process_is_running(pid: u32) -> bool {
    let output = Command::new("tasklist.exe")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .expect("query fake child process");
    String::from_utf8_lossy(&output.stdout).contains(&pid.to_string())
}

fn write_json_line(stdout: &mut dyn Write, value: Value) {
    writeln!(
        stdout,
        "{}",
        serde_json::to_string(&value).expect("serialize fake JSON")
    )
    .expect("stdout JSONL");
    stdout.flush().expect("flush stdout JSONL");
}

fn read_json_lines(path: &Path) -> Vec<Value> {
    BufReader::new(File::open(path).expect("open capture"))
        .lines()
        .map(|line| serde_json::from_str(&line.expect("capture line")).expect("captured JSON"))
        .collect()
}

struct TestDir {
    path: PathBuf,
}
impl TestDir {
    fn new() -> Self {
        Self {
            path: tempfile::tempdir().expect("test directory").keep(),
        }
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

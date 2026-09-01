# Phase 2 Live Compatibility Repair Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the installed ArcGIS Pro Agent discover its bundled MCP server, track current and legacy Codex MCP startup events through one fail-closed state machine, and remain usable at 150% Windows display scaling.

**Architecture:** Add a pure `mcp_status` module that parses allowlisted App Server data and combines lifecycle plus tool-inventory evidence. `DesktopState` owns that readiness accumulator behind the existing generation/runtime/thread leases, while `commands` only routes notifications and issues the bounded `mcpServerStatus/list` request. Keep executable discovery in `paths.rs` and fix constrained-height behavior in CSS without changing GIS tools or Tauri window dimensions.

**Tech Stack:** Rust 2024, Tokio, serde_json, Tauri 2, React 19, TypeScript 7, Vitest 4, PowerShell, .NET 8/.NET 10, ArcGIS Pro 3.7.

## Global Constraints

- `mcpServer/startupStatus/updated` is authoritative; `mcpServer/status/updated` is a compatibility alias into the same parser and state transition.
- Never expose or retain raw App Server `error` text; only allowlisted lifecycle values and safe local failure categories may be stored.
- ArcGIS readiness requires both a `ready` lifecycle and a validated `arcgis` tool inventory for the current runtime epoch and active thread.
- `ARCGIS_AGENT_MCP_COMMAND` is the highest-priority override; otherwise prefer `<version>/mcp/ArcGISProAgent.Mcp.exe`, then fall back to the existing bare command.
- Do not modify global `PATH`, global MCP configuration, API-key handling, Add-In/MCP tool contracts, R0/R1 behavior, or any R2/R3 surface.
- Do not change `tauri.conf.json` window dimensions unless the 150% live verification still fails after the CSS repair.
- Tests and live verification must not save an ArcGIS project or modify GIS data.
- Use TDD for every behavior change and commit each independently reviewable task.

---

## File Structure

- Create `apps/desktop/src-tauri/src/mcp_status.rs`: protocol method constants, allowlisted status parser, safe failure categories, inventory parser, list parameters, and the pure readiness accumulator.
- Modify `apps/desktop/src-tauri/src/lib.rs`: export the new module.
- Modify `apps/desktop/src-tauri/src/app_state.rs`: replace the standalone readiness boolean with the accumulator and enforce runtime/thread leases.
- Modify `apps/desktop/src-tauri/src/commands.rs`: route both notification names, start the event loop before account-driven conversation work, request status after `thread/start`, and use portable MCP command resolution.
- Modify `apps/desktop/src-tauri/src/paths.rs`: add deterministic installed-layout MCP executable resolution.
- Modify `apps/desktop/src-tauri/tests/desktop_commands.rs`: pure protocol/status/path assertions.
- Modify `apps/desktop/src-tauri/tests/command_lifecycle.rs`: asynchronous conversation, epoch, ordering, and failure-closed assertions.
- Create `apps/desktop/tests/layoutCss.test.ts`: constrained-height and drawer CSS regression assertions.
- Modify `apps/desktop/src/app.css`: remove fixed root height assumptions and make the conversation body scrollable.
- Modify `docs/development/phase-2-user-guide.md`: document automatic startup, compatibility, installed path, DPI behavior, and diagnosis.
- Modify `docs/development/phase-2-smoke.md`: record the focused live compatibility acceptance steps and observations.
- Modify ignored `.superpowers/sdd/progress.md` and `.superpowers/sdd/phase2-task-5-report.md`: keep local SDD tracking current without force-adding ignored files.

---

### Task 1: Single MCP readiness state machine

**Files:**
- Create: `apps/desktop/src-tauri/src/mcp_status.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/src/app_state.rs`
- Modify: `apps/desktop/src-tauri/tests/desktop_commands.rs`
- Modify: `apps/desktop/src-tauri/tests/command_lifecycle.rs`

**Interfaces:**
- Produces: `parse_arcgis_status_notification(method: &str, params: &Value) -> Option<ArcGisStatusUpdate>`.
- Produces: `mcp_status_list_params(thread_id: &str) -> Value` and `arcgis_inventory_is_valid(response: &Value) -> bool`.
- Produces: `ArcGisMcpReadiness::{apply_status, apply_inventory, is_ready, failure}`.
- Produces: `McpDiscoveryLease { generation, runtime_epoch, thread_id }`, `DesktopState::apply_arcgis_status(runtime_epoch, update)`, and `apply_arcgis_inventory(&lease, valid)`.
- Consumes: existing `Coordinator` generation, runtime epoch, active thread, and readiness epoch.

- [ ] **Step 1: Write failing pure status tests**

Add imports and tests in `apps/desktop/src-tauri/tests/desktop_commands.rs` that define the exact allowlist and precedence:

```rust
use arcgis_pro_agent_desktop_lib::mcp_status::{
    ArcGisMcpReadiness, FailureCategory, Lifecycle, StatusSource,
    arcgis_inventory_is_valid, mcp_status_list_params,
    parse_arcgis_status_notification,
};

#[test]
fn current_and_legacy_mcp_events_share_one_allowlisted_parser() {
    let params = json!({
        "name": "arcgis",
        "status": "failed",
        "threadId": "thread-1",
        "failureReason": "reauthenticationRequired",
        "error": "must-not-be-retained"
    });
    let current = parse_arcgis_status_notification(
        "mcpServer/startupStatus/updated",
        &params,
    ).expect("current event");
    let legacy = parse_arcgis_status_notification(
        "mcpServer/status/updated",
        &params,
    ).expect("legacy event");
    assert_eq!(current.source, StatusSource::Current);
    assert_eq!(current.lifecycle, Lifecycle::Failed);
    assert_eq!(current.failure, Some(FailureCategory::ReauthenticationRequired));
    assert_eq!(legacy.source, StatusSource::Legacy);
    assert_eq!(legacy.failure, Some(FailureCategory::StartupFailed));
    assert!(!format!("{current:?}{legacy:?}").contains("must-not-be-retained"));
    assert!(parse_arcgis_status_notification("unknown", &params).is_none());
    assert!(parse_arcgis_status_notification(
        "mcpServer/startupStatus/updated",
        &json!({"name": "other", "status": "ready"}),
    ).is_none());
    let unknown = parse_arcgis_status_notification(
        "mcpServer/startupStatus/updated",
        &json!({"name": "arcgis", "status": "future-state", "threadId": "thread-1"}),
    ).expect("recognized arcgis event must fail closed");
    assert_eq!(unknown.lifecycle, Lifecycle::Unknown);
    assert_eq!(unknown.failure, Some(FailureCategory::Unknown));
}

#[test]
fn readiness_requires_lifecycle_and_baseline_inventory_in_either_order() {
    let ready = parse_arcgis_status_notification(
        "mcpServer/startupStatus/updated",
        &json!({"name": "arcgis", "status": "ready", "threadId": "thread-1"}),
    ).unwrap();
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
    ).unwrap();
    assert!(readiness.apply_status(current.clone()));
    assert!(!readiness.apply_status(current));
    let legacy = parse_arcgis_status_notification(
        "mcpServer/status/updated",
        &json!({"name": "arcgis", "status": "ready"}),
    ).unwrap();
    assert!(!readiness.apply_status(legacy));
    assert_eq!(readiness.failure(), Some(FailureCategory::ReauthenticationRequired));
    assert!(!readiness.is_ready());
}

#[test]
fn list_params_and_inventory_require_only_the_phase_2_baseline() {
    assert_eq!(mcp_status_list_params("thread-1"), json!({
        "threadId": "thread-1",
        "detail": "toolsAndAuthOnly"
    }));
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
```

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test desktop_commands mcp -- --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test desktop_commands readiness_requires -- --nocapture
```

Expected: compilation fails because `mcp_status` and its public types/functions do not exist.

- [ ] **Step 3: Implement the pure module**

Create `apps/desktop/src-tauri/src/mcp_status.rs` with these concrete types and rules:

```rust
use serde_json::{Value, json};

pub const CURRENT_STATUS_METHOD: &str = "mcpServer/startupStatus/updated";
pub const LEGACY_STATUS_METHOD: &str = "mcpServer/status/updated";
const REQUIRED_TOOLS: [&str; 3] = [
    "arcgis_connection_status",
    "arcgis_describe_context",
    "arcgis_list_layers",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusSource { Current, Legacy }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle { Starting, Ready, Failed, Cancelled, Unknown }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCategory {
    StartupFailed,
    ReauthenticationRequired,
    Cancelled,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArcGisStatusUpdate {
    pub source: StatusSource,
    pub thread_id: Option<String>,
    pub lifecycle: Lifecycle,
    pub failure: Option<FailureCategory>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArcGisMcpReadiness {
    lifecycle: Option<Lifecycle>,
    inventory_valid: bool,
    current_seen: bool,
    failure: Option<FailureCategory>,
}

impl ArcGisMcpReadiness {
    pub fn apply_status(&mut self, update: ArcGisStatusUpdate) -> bool {
        if self.current_seen && update.source == StatusSource::Legacy {
            return false;
        }
        let before = self.clone();
        if update.source == StatusSource::Current {
            self.current_seen = true;
        }
        self.lifecycle = Some(update.lifecycle);
        self.failure = update.failure;
        *self != before
    }

    pub fn apply_inventory(&mut self, valid: bool) -> bool {
        let changed = self.inventory_valid != valid;
        self.inventory_valid = valid;
        changed
    }

    pub fn is_ready(&self) -> bool {
        self.inventory_valid && self.lifecycle == Some(Lifecycle::Ready)
    }

    pub fn failure(&self) -> Option<FailureCategory> { self.failure }
}

pub fn parse_arcgis_status_notification(method: &str, params: &Value)
    -> Option<ArcGisStatusUpdate>
{
    let source = match method {
        CURRENT_STATUS_METHOD => StatusSource::Current,
        LEGACY_STATUS_METHOD => StatusSource::Legacy,
        _ => return None,
    };
    if params.get("name").and_then(Value::as_str) != Some("arcgis") {
        return None;
    }
    let lifecycle = match params.get("status").and_then(Value::as_str) {
        Some("starting") => Lifecycle::Starting,
        Some("ready") => Lifecycle::Ready,
        Some("failed") => Lifecycle::Failed,
        Some("cancelled") => Lifecycle::Cancelled,
        _ => Lifecycle::Unknown,
    };
    let failure = match lifecycle {
        Lifecycle::Failed if source == StatusSource::Current
            && params.get("failureReason").and_then(Value::as_str)
                == Some("reauthenticationRequired") =>
            Some(FailureCategory::ReauthenticationRequired),
        Lifecycle::Failed => Some(FailureCategory::StartupFailed),
        Lifecycle::Cancelled => Some(FailureCategory::Cancelled),
        Lifecycle::Unknown => Some(FailureCategory::Unknown),
        Lifecycle::Starting | Lifecycle::Ready => None,
    };
    Some(ArcGisStatusUpdate {
        source,
        thread_id: match params.get("threadId") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
            _ => return Some(ArcGisStatusUpdate {
                source,
                thread_id: None,
                lifecycle: Lifecycle::Unknown,
                failure: Some(FailureCategory::Unknown),
            }),
        },
        lifecycle,
        failure,
    })
}

pub fn mcp_status_list_params(thread_id: &str) -> Value {
    json!({"threadId": thread_id, "detail": "toolsAndAuthOnly"})
}

pub fn arcgis_inventory_is_valid(response: &Value) -> bool {
    response.get("data").and_then(Value::as_array).into_iter().flatten()
        .find(|server| server.get("name").and_then(Value::as_str) == Some("arcgis"))
        .and_then(|server| server.get("tools").and_then(Value::as_object))
        .is_some_and(|tools| REQUIRED_TOOLS.iter().all(|name| tools.contains_key(*name)))
}
```

Export it from `lib.rs` with `pub mod mcp_status;`.

- [ ] **Step 4: Write failing state lease tests**

In `apps/desktop/src-tauri/tests/command_lifecycle.rs`, add a helper that installs a client, records the returned runtime epoch, activates `thread-context`, and applies lifecycle plus inventory. Assert that a wrong epoch and wrong thread are rejected, either arrival order becomes ready, duplicate application does not change `ready_epoch` behavior, and `mark_runtime_stopped`, account change, and conversation replacement reset readiness.

Use this exact state-facing call shape:

```rust
let runtime_epoch = state.install_client(client).await;
let (conversation, _) = state.begin_conversation().await.unwrap();
let discovery = state.commit_conversation(
    &conversation,
    "thread-context".to_owned(),
).await.unwrap();
let update = parse_arcgis_status_notification(
    CURRENT_STATUS_METHOD,
    &json!({"name": "arcgis", "status": "ready", "threadId": "thread-context"}),
).unwrap();
assert!(state.apply_arcgis_status(runtime_epoch, update).await);
assert!(state.apply_arcgis_inventory(&discovery, true).await);
assert!(state.arcgis_ready().await);
state.mark_runtime_stopped().await;
assert!(!state.apply_arcgis_inventory(&discovery, true).await);
```

- [ ] **Step 5: Run the state tests and verify RED**

Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test command_lifecycle arcgis_readiness -- --nocapture
```

Expected: compilation fails because `install_client` returns `()` and the leased apply methods do not exist.

- [ ] **Step 6: Integrate the accumulator into `DesktopState`**

Replace `Coordinator.arcgis_ready: bool` with `arcgis_mcp: ArcGisMcpReadiness`. Make `install_client` return the incremented `runtime_epoch`. Extend `ConversationLease` with the runtime epoch captured by `begin_conversation`, and make `commit_conversation` verify both generation and runtime epoch before returning:

```rust
#[derive(Clone)]
pub struct McpDiscoveryLease {
    generation: u64,
    runtime_epoch: u64,
    thread_id: String,
}

impl McpDiscoveryLease {
    pub fn runtime_epoch(&self) -> u64 { self.runtime_epoch }
    pub fn thread_id(&self) -> &str { &self.thread_id }
}
```

Add the two leased apply methods. Inventory submission must check all three discovery-lease fields. Status submission must check the captured event-loop runtime epoch and an exact non-empty `threadId` before it may establish `ready`. A global `threadId: null` `starting`/`failed`/`cancelled`/`unknown` notification may revoke readiness for the current runtime, but a global `ready` notification must be rejected. Each accepted apply method must preserve the readiness epoch rule:

```rust
let before = coordinator.arcgis_mcp.is_ready();
if coordinator.runtime_epoch != runtime_epoch
    || coordinator.generation != expected_generation
    || coordinator.active.thread_id.as_deref() != expected_thread
{
    return false;
}
// Apply the current status or inventory evidence.
let after = coordinator.arcgis_mcp.is_ready();
if before != after {
    coordinator.ready_epoch = coordinator.ready_epoch.wrapping_add(1);
}
true
```

Unknown/missing lifecycle values and malformed `threadId` values enter the same state machine as `Lifecycle::Unknown` and revoke readiness instead of being ignored. Each apply method returns `true` only when a current lease was accepted and internal state changed; rejected and duplicate updates return `false`. Replace all reset sites with `coordinator.arcgis_mcp = ArcGisMcpReadiness::default()`. Make `arcgis_ready()`, `health_lease()`, and `health_lease_is_current()` derive readiness from `arcgis_mcp.is_ready()`. Remove production use of `set_arcgis_ready_for`; update tests that used `set_arcgis_ready(true)` to obtain a discovery lease via `begin_conversation`/`commit_conversation`, then install valid lifecycle and inventory evidence instead of bypassing the invariant.

- [ ] **Step 7: Run focused and complete Rust tests**

Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test desktop_commands -- --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test command_lifecycle -- --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --nocapture
```

Expected: all active Rust tests pass; the existing explicitly ignored Windows runtime probe remains ignored unless manually selected.

- [ ] **Step 8: Commit Task 1**

```powershell
git add apps/desktop/src-tauri/src/mcp_status.rs apps/desktop/src-tauri/src/lib.rs apps/desktop/src-tauri/src/app_state.rs apps/desktop/src-tauri/tests/desktop_commands.rs apps/desktop/src-tauri/tests/command_lifecycle.rs
git commit -m "fix(desktop): unify MCP readiness state"
```

---

### Task 2: Current App Server startup discovery

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/tests/command_lifecycle.rs`
- Modify: `apps/desktop/src-tauri/tests/desktop_commands.rs`

**Interfaces:**
- Consumes: Task 1 `parse_arcgis_status_notification`, `mcp_status_list_params`, `arcgis_inventory_is_valid`, and leased `DesktopState` methods.
- Produces: `handle_mcp_status_notification_with(state, runtime_epoch, method, params) -> bool`.
- Produces: `refresh_mcp_status_with(state, client, &McpDiscoveryLease) -> bool`.

- [ ] **Step 1: Write failing routing and conversation tests**

Add tests for both method names through one exported seam and update the existing conversation test to script two responses:

```rust
let client = Arc::new(ScriptedClient::new([
    Ok(json!({"thread": {"id": "thread-command"}})),
    Ok(json!({"data": [{"name": "arcgis", "tools": {
        "arcgis_connection_status": {},
        "arcgis_describe_context": {},
        "arcgis_list_layers": {}
    }}]})),
]));
// ... conversation_start_with(...)
assert_eq!(calls[1], (
    "mcpServerStatus/list".to_owned(),
    json!({"threadId": "thread-command", "detail": "toolsAndAuthOnly"})
));
```

Add a failure case where the second response is `Err("status unavailable")`; `conversation_start_with` must still return `thread-command`, preserve the active thread, and leave `arcgis_ready == false`.

For routing, call `handle_mcp_status_notification_with` once with each supported method and assert an unrelated server, unknown status, wrong thread, wrong epoch, and unknown method are rejected. Apply a valid inventory response before asserting that a current or legacy `ready` event makes the state ready.

- [ ] **Step 2: Run focused tests and verify RED**

Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test command_lifecycle conversation_command -- --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test command_lifecycle mcp_status -- --nocapture
```

Expected: the conversation call count is still one and the exported routing seam does not exist.

- [ ] **Step 3: Implement bounded discovery and shared event routing**

In `commands.rs`, add a separate bounded timeout and these seams:

```rust
const MCP_STATUS_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn refresh_mcp_status_with(
    state: &DesktopState,
    client: &dyn AppServerClient,
    lease: &McpDiscoveryLease,
) -> bool {
    let response = tokio::time::timeout(
        MCP_STATUS_TIMEOUT,
        client.request(
            "mcpServerStatus/list",
            mcp_status_list_params(lease.thread_id()),
        ),
    ).await;
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
```

Use the `McpDiscoveryLease` returned by `commit_conversation` to call `refresh_mcp_status_with`, ignore its boolean for the conversation result, and always return the valid conversation ID. Do not send a turn. If the runtime changes between `begin_conversation` and `commit_conversation`, the commit fails; if it changes while the status list is in flight, inventory submission is rejected by the lease.

Make `handle_notification` route both constants to `handle_mcp_status_notification_with`; remove direct field access and the old `set_arcgis_ready_for` branch. Pass the epoch captured from `install_client` into `run_event_loop` and all notification handling.

Move event-loop spawning to immediately after client installation and before `refresh_account` can publish a signed-in snapshot. Keep the health poller startup after account refresh.

- [ ] **Step 4: Run protocol, lifecycle, and full Rust tests**

Run:

```powershell
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test desktop_commands -- --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test command_lifecycle -- --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --nocapture
```

Expected: formatting check and all active Rust tests pass; no request named `turn/start` is generated by conversation startup.

- [ ] **Step 5: Commit Task 2**

```powershell
git add apps/desktop/src-tauri/src/commands.rs apps/desktop/src-tauri/tests/command_lifecycle.rs apps/desktop/src-tauri/tests/desktop_commands.rs
git commit -m "fix(desktop): discover current MCP startup status"
```

---

### Task 3: Portable installed MCP command resolution

**Files:**
- Modify: `apps/desktop/src-tauri/src/paths.rs`
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/tests/desktop_commands.rs`

**Interfaces:**
- Produces: `resolve_mcp_command(explicit: Option<&OsStr>, current_exe: Option<&Path>, is_file: impl Fn(&Path) -> bool) -> PathBuf`.
- Consumes: `std::env::var_os("ARCGIS_AGENT_MCP_COMMAND")`, `std::env::current_exe()`, and `Path::is_file`.

- [ ] **Step 1: Write failing path-priority tests**

Add these cases to `desktop_commands.rs` without changing process-global environment variables:

```rust
#[test]
fn mcp_command_resolution_prefers_override_then_installed_sibling_then_bare_name() {
    let exe = Path::new(r"C:\Program Files\ArcGISProAgent\0.1.0\desktop\arcgis-pro-agent-desktop.exe");
    let sibling = PathBuf::from(r"C:\Program Files\ArcGISProAgent\0.1.0\mcp\ArcGISProAgent.Mcp.exe");
    assert_eq!(
        resolve_mcp_command(Some(OsStr::new(r"E:\custom path\mcp.exe")), Some(exe), |_| true),
        PathBuf::from(r"E:\custom path\mcp.exe")
    );
    assert_eq!(resolve_mcp_command(None, Some(exe), |path| path == sibling), sibling);
    assert_eq!(
        resolve_mcp_command(None, Some(exe), |_| false),
        PathBuf::from("ArcGISProAgent.Mcp.exe")
    );
    assert_eq!(
        resolve_mcp_command(None, Some(Path::new(r"C:\repo\target\debug\desktop.exe")), |_| true),
        PathBuf::from("ArcGISProAgent.Mcp.exe")
    );
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test desktop_commands mcp_command_resolution -- --nocapture
```

Expected: compilation fails because `resolve_mcp_command` does not exist.

- [ ] **Step 3: Implement deterministic resolution in `paths.rs`**

Add:

```rust
use std::ffi::OsStr;

pub const MCP_EXECUTABLE_NAME: &str = "ArcGISProAgent.Mcp.exe";

pub fn resolve_mcp_command(
    explicit: Option<&OsStr>,
    current_exe: Option<&Path>,
    is_file: impl Fn(&Path) -> bool,
) -> PathBuf {
    if let Some(explicit) = explicit.filter(|value| !value.is_empty()) {
        return PathBuf::from(explicit);
    }
    let candidate = current_exe
        .and_then(Path::parent)
        .filter(|parent| parent.file_name().is_some_and(|name|
            name.to_string_lossy().eq_ignore_ascii_case("desktop")))
        .and_then(Path::parent)
        .map(|version| version.join("mcp").join(MCP_EXECUTABLE_NAME));
    candidate.filter(|path| is_file(path))
        .unwrap_or_else(|| PathBuf::from(MCP_EXECUTABLE_NAME))
}
```

In `initialize_runtime`, replace the MCP `environment_path` call with:

```rust
let explicit_mcp = std::env::var_os("ARCGIS_AGENT_MCP_COMMAND");
let current_exe = std::env::current_exe().ok();
let mcp_command = resolve_mcp_command(
    explicit_mcp.as_deref(),
    current_exe.as_deref(),
    Path::is_file,
);
```

Pass this `PathBuf` directly to `CodexStartOptions`; do not quote it manually or use a shell.

- [ ] **Step 4: Run path, command-serialization, and full Rust tests**

Run:

```powershell
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test desktop_commands mcp_command_resolution -- --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test codex_client production_command_registers_only_arcgis_without_api_key_or_token -- --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --nocapture
```

Expected: all commands pass; the existing command serialization test proves paths containing spaces and quotes remain TOML data rather than shell syntax.

- [ ] **Step 5: Commit Task 3**

```powershell
git add apps/desktop/src-tauri/src/paths.rs apps/desktop/src-tauri/src/commands.rs apps/desktop/src-tauri/tests/desktop_commands.rs
git commit -m "fix(desktop): resolve bundled MCP executable"
```

---

### Task 4: High-DPI constrained-height layout

**Files:**
- Create: `apps/desktop/tests/layoutCss.test.ts`
- Modify: `apps/desktop/src/app.css`

**Interfaces:**
- Consumes: current `app-shell`, `conversation-pane`, `conversation-body`, `composer-wrap`, and context drawer class names.
- Produces: CSS that fits an approximately `853 × 533` CSS viewport while preserving the existing `<980px` drawer behavior.

- [ ] **Step 1: Write the failing CSS regression test**

Create `apps/desktop/tests/layoutCss.test.ts`:

```ts
import css from "../src/app.css?raw";

function rule(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = css.match(new RegExp(`${escaped}\\s*\\{([^}]*)\\}`, "m"));
  expect(match, `missing CSS rule for ${selector}`).not.toBeNull();
  return match![1];
}

test("root and conversation can shrink without hiding the composer", () => {
  const rootMatch = css.match(/html,\s*body,\s*#root\s*\{([^}]*)\}/m);
  expect(rootMatch).not.toBeNull();
  const root = rootMatch![1];
  expect(root).toContain("height: 100%");
  expect(root).toContain("min-height: 0");
  expect(root).not.toContain("min-height: 720px");
  expect(rule(".app-shell")).toContain("min-height: 0");
  expect(rule(".conversation-body")).toContain("overflow-y: auto");
});

test("narrow effective CSS width retains the context drawer", () => {
  expect(css).toContain("@media (max-width: 979px)");
  expect(css).toMatch(/@media \(max-width: 979px\)[\s\S]*\.context-toggle\s*\{[\s\S]*display:\s*flex/);
  expect(css).toMatch(/@media \(max-width: 979px\)[\s\S]*\.context-pane--open\s*\{[\s\S]*visibility:\s*visible/);
});
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```powershell
npm test -- tests/layoutCss.test.ts
```

Working directory: `apps/desktop`.

Expected: the first test fails because the root still has `min-height: 720px`, `.app-shell` lacks `min-height: 0`, and `.conversation-body` lacks vertical overflow.

- [ ] **Step 3: Apply the minimal CSS repair**

Change the relevant rules to include exactly these constraints while preserving existing colors, dimensions, grid rows, and breakpoints:

```css
html,
body,
#root {
  width: 100%;
  min-width: 0;
  height: 100%;
  min-height: 0;
  margin: 0;
}

.app-shell {
  /* existing declarations remain */
  min-height: 0;
}

.conversation-body {
  /* existing declarations remain */
  overflow-y: auto;
}
```

Do not change `tauri.conf.json` during this step.

- [ ] **Step 4: Run frontend tests and production build**

Run from `apps/desktop`:

```powershell
npm test
npm run build
```

Expected: all Vitest tests pass and Vite completes a production build with exit code 0.

- [ ] **Step 5: Commit Task 4**

```powershell
git add apps/desktop/tests/layoutCss.test.ts apps/desktop/src/app.css
git commit -m "fix(desktop): keep composer visible at high DPI"
```

---

### Task 5: Documentation, complete gates, reinstall, and read-only live smoke

**Files:**
- Modify: `docs/development/phase-2-user-guide.md`
- Modify: `docs/development/phase-2-smoke.md`
- Modify: `.superpowers/sdd/progress.md` (ignored local tracking only)
- Modify: `.superpowers/sdd/phase2-task-5-report.md` (ignored local tracking only)

**Interfaces:**
- Consumes: all Task 1–4 behavior and existing transactional installer.
- Produces: user-facing startup/diagnostic instructions and evidence that the installed product works without GIS writes.

- [ ] **Step 1: Update user and smoke documentation**

In `phase-2-user-guide.md`, document this exact startup contract:

```markdown
- 新建对话后，桌面端会主动调用 `mcpServerStatus/list` 发现 ArcGIS MCP；无需先发送一条模型消息。
- 当前 Codex 使用 `mcpServer/startupStatus/updated`；旧事件名只作为内部兼容别名，不代表存在第二套 MCP 状态。
- 显式 `ARCGIS_AGENT_MCP_COMMAND` 优先；开发安装默认自动使用同版本 `mcp\ArcGISProAgent.Mcp.exe`，无需修改全局 PATH。
- 150% 缩放时右侧上下文自动改为抽屉；输入区应始终可见。
```

In `phase-2-smoke.md`, add a dated “实机兼容性修复验收” section with checkboxes for: no first turn, installed sibling command, connected state, R0 health/context, 150% composer visibility, drawer open/close, no API key, no project save, and no GIS data mutation. Do not mark an item complete until directly observed.

- [ ] **Step 2: Run source redaction and placeholder checks**

Run:

```powershell
rg -n "must-not-leak|sk-proj-|map@example.com|C:\\Users\\Administrator" apps/desktop/src apps/desktop/src-tauri/src docs/development
git diff --check
```

Expected: no production credential/path leakage and no whitespace errors. Test-only sentinel strings outside the searched production paths are allowed.

- [ ] **Step 3: Run complete non-GUI verification**

With ArcGIS Pro closed, run from the worktree root:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\Test-Foundation.ps1 -ArcGISProInstallDir D:\arcgis_pro
```

Expected: installer tests, .NET tests, frontend tests/build, Rust tests, source allowlists, and Tauri debug no-bundle build all exit 0. The guard prevents Add-In registration and no GUI or GIS operation occurs.

- [ ] **Step 4: Commit tracked documentation**

```powershell
git add docs/development/phase-2-user-guide.md docs/development/phase-2-smoke.md
git commit -m "docs: document live compatibility workflow"
```

- [ ] **Step 5: Transactionally reinstall the development build**

Resolve the exact install destinations through the existing installer, ensure ArcGIS Pro is closed for replacement, then run:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\Install-Dev.ps1 -ArcGISProInstallDir D:\arcgis_pro
```

Expected: installation exits 0, reports the desktop/MCP/Add-In destinations, and the ownership manifest validates every installed file. Do not manually copy or delete install files.

- [ ] **Step 6: Run the read-only live smoke at 150% scaling**

Start the installed desktop program and ArcGIS Pro 3.7 under the same Windows user and privilege level. Use the existing ChatGPT subscription session. Create a new conversation but do not send a turn. Observe and record:

1. ArcGIS becomes ready from status discovery.
2. `arcgis_connection_status` and the existing context refresh return only safe R0 summaries.
3. The input composer is fully visible at DPI 144 / 150%.
4. The context button opens the drawer and its close button/scrim closes it.
5. No API key is requested, no project is saved, and no GIS data is edited.

Use a non-sensitive window screenshot or `PrintWindow` capture for layout evidence. Do not include account email, access tokens, raw tool errors, project paths, or data-source paths in reports.

If the composer or drawer still fails at 150%, do not mark the task complete. Invoke `superpowers:systematic-debugging`, capture the physical client size/DPI/effective CSS viewport again, add a failing regression assertion for the observed constraint, and make only the smallest evidence-backed `tauri.conf.json` or CSS adjustment before rerunning Steps 3–6.

- [ ] **Step 7: Update local SDD tracking and run final verification-before-completion**

Update the ignored SDD files with commit IDs, exact test counts, install manifest result, DPI, and observed smoke outcome. Then run:

```powershell
git status --short
git log -7 --oneline
git diff HEAD~5..HEAD --check
```

Expected: the tracked worktree is clean, the five task commits are present after the plan commit, and the final diff has no whitespace errors. Invoke `superpowers:verification-before-completion` before claiming completion.

# ArcGIS Pro 智能助手最小可分发预览版 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在不修改已验收 ArcGIS 核心语义的前提下，交付一个 Windows x64 预览安装包，让试用者安装后自动发现 ArcGIS Pro 3.7，并在 DeepSeek API 与 ChatGPT 订阅登录之间二选一使用现有 17 个 MCP 工具。

**Architecture:** 保留 .NET 10 Add-In、.NET 8 MCP Server、Bridge 和现有 Codex App Server 链路，在 Tauri 后端新增一个小型、源码级可替换的提供商边界。DeepSeek 适配器通过应用私有 MCP stdio 客户端调用同一工具集；安装、发现、凭据和诊断均限制在应用专用目录及精确所有权清单中。

**Tech Stack:** Windows 10/11 x64、ArcGIS Pro 3.7、Tauri 2.11、Rust 2024、React 19、TypeScript 7、Vitest 4、.NET 8 MCP/Bridge、.NET 10 Add-In、NSIS、Codex CLI `0.144.5`、DeepSeek OpenAI-compatible Chat Completions API。

## Global Constraints

- 只在 `feature/distributable-preview-deepseek` 和 `.worktrees/distributable-preview-deepseek` 中实施；不得修改或合并 `master` 与 `feature/arcgis-pro-agent-foundation`。
- 预览版本固定为 `0.2.0-preview.1`，目标为 Windows 10/11 x64 与 ArcGIS Pro 3.7。
- 不新增或改变现有 17 个 R0/R1 ArcGIS 工具语义；不增加 R2/R3、保存、导出、地理处理或源数据编辑。
- 提供商是二选一；不做自动路由、自动故障切换、多用户、计费、自动更新、插件市场或遥测后台。
- DeepSeek Key 只存 Windows Credential Manager；ChatGPT 身份只存应用私有 `CODEX_HOME`；任何秘密不得进入设置、命令行、日志、诊断或前端持久状态。
- 安装和卸载只管理安装清单拥有的文件；不得递归删除 ArcGIS AddIns 根目录、ArcGIS 工程、地理数据库或其他 Codex 数据。
- 所有实现先写失败测试，再做最小实现；每个任务独立提交并通过其局部回归。
- 开发命令在 PowerShell 中使用 `npm.cmd`，避免本机执行策略阻止 `npm.ps1`。

---

## File Structure

新增或调整的文件按单一责任组织：

- `apps/desktop/src-tauri/src/providers/mod.rs`：统一提供商类型、状态、事件与注册入口。
- `apps/desktop/src-tauri/src/providers/codex.rs`：把现有 Codex 生命周期映射为统一提供商契约。
- `apps/desktop/src-tauri/src/providers/deepseek.rs`：DeepSeek 会话与有界工具循环。
- `apps/desktop/src-tauri/src/providers/deepseek_api.rs`：DeepSeek HTTP/SSE 协议与安全错误映射。
- `apps/desktop/src-tauri/src/settings.rs`：无秘密的版本化应用设置。
- `apps/desktop/src-tauri/src/credential_store.rs`：Windows Credential Manager 的窄接口。
- `apps/desktop/src-tauri/src/arcgis_install.rs`：ArcGIS Pro 发现、验证和启动。
- `apps/desktop/src-tauri/src/addin_install.rs`：Add-In 精确安装、哈希所有权、修复与清理。
- `apps/desktop/src-tauri/src/mcp/mod.rs`：应用私有 MCP 客户端公开接口。
- `apps/desktop/src-tauri/src/mcp/client.rs`：MCP stdio JSON-RPC 生命周期。
- `apps/desktop/src-tauri/src/arcgis_tool_client.rs`：Codex 和直接 MCP 共用的工具调用窄接口。
- `apps/desktop/src-tauri/src/diagnostics.rs`：只生成脱敏版本/状态摘要。
- `apps/desktop/src/components/SetupView.tsx`：首次启动和提供商二选一流程。
- `apps/desktop/src/components/ProviderSwitcher.tsx`：登录后最小提供商切换入口。
- `apps/desktop/src-tauri/tauri.preview.conf.json`：只用于预览 NSIS bundle 的配置覆盖层。
- `apps/desktop/src-tauri/windows/hooks.nsh`：精确的桌面快捷方式和卸载清理钩子。
- `scripts/Build-Preview.ps1`：可重复构建、暂存、打包和计算 SHA-256。
- `scripts/Test-PreviewPackaging.ps1`：不安装即可验证 bundle 输入与所有权规则。
- `Open-Project.ps1`：从仓库根目录打开 `McpServer.sln`。

---

### Task 1: 建立提供商中立契约并保持 Codex 行为不变

**Files:**
- Create: `apps/desktop/src-tauri/src/providers/mod.rs`
- Create: `apps/desktop/src-tauri/src/providers/codex.rs`
- Create: `apps/desktop/src-tauri/tests/provider_contract.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/src/app_state.rs`
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/codex/client.rs`
- Modify: `apps/desktop/src-tauri/src/paths.rs`
- Modify: `apps/desktop/src/domain.ts`
- Modify: `apps/desktop/src/desktopApi.ts`
- Modify: `apps/desktop/src/appStore.ts`
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/components/LoginView.tsx`
- Modify: `apps/desktop/src/components/Sidebar.tsx`
- Modify: `apps/desktop/tests/App.test.tsx`
- Modify: `apps/desktop/tests/appStore.test.ts`
- Modify: `apps/desktop/tests/loginFlow.test.tsx`

**Interfaces:**
- Consumes: existing `AccountSnapshot`, `CodexSnapshot`, `CodexRuntime`, `conversation_start_with`, `turn_start_with` and `desktop://event` stream.
- Produces: `ProviderKind`, `ProviderAuthSnapshot`, `ProviderRuntimeSnapshot`, `ProviderSnapshot`, `ProviderEvent`; `DesktopSnapshot.provider`; app-private `CodexStartOptions.codex_home`.

- [ ] **Step 1: Write the failing provider-contract tests**

Create `apps/desktop/src-tauri/tests/provider_contract.rs` with the exact public shape and Codex isolation expectations:

```rust
use std::path::PathBuf;

use arcgis_pro_agent_desktop_lib::{
    app_state::DesktopSnapshot,
    codex::{build_codex_command, CodexStartOptions},
    providers::{ProviderAuthSnapshot, ProviderKind, ProviderRuntimeSnapshot},
    runtime_secret::create_runtime_file,
};

#[test]
fn default_snapshot_is_provider_neutral_and_codex_selected() {
    let snapshot = DesktopSnapshot::default();
    assert_eq!(snapshot.provider.kind, ProviderKind::Codex);
    assert_eq!(snapshot.provider.auth, ProviderAuthSnapshot::Checking);
    assert_eq!(snapshot.provider.runtime, ProviderRuntimeSnapshot::Starting);
}

#[test]
fn codex_home_is_injected_only_into_the_child() {
    let base = std::env::temp_dir().join("arcgis-provider-contract");
    let runtime = create_runtime_file(&base).unwrap();
    let options = CodexStartOptions {
        codex_command: PathBuf::from("codex.exe"),
        codex_home: base.join("codex-home"),
        mcp_command: PathBuf::from("ArcGISProAgent.Mcp.exe"),
        mcp_args: vec![],
        local_app_data: base,
    };
    let command = build_codex_command(&options, &runtime);
    assert_eq!(command.get_envs().find(|(k, _)| *k == "CODEX_HOME").unwrap().1,
               Some(options.codex_home.as_os_str()));
}
```

Update frontend fixtures so `DesktopSnapshot` requires:

```ts
provider: {
  kind: "codex",
  auth: { status: "needsSetup" },
  runtime: { status: "ready", version: "0.144.5" },
}
```

- [ ] **Step 2: Run the focused tests and verify the new contract is absent**

Run:

```powershell
Set-Location apps\desktop\src-tauri
cargo test --test provider_contract
Set-Location ..
npm.cmd test -- tests/appStore.test.ts tests/loginFlow.test.tsx
```

Expected: Rust fails because `providers` and `DesktopSnapshot.provider` do not exist; TypeScript fails because fixtures and state still use `account`/`codex` as the public contract.

- [ ] **Step 3: Add the minimal provider types and Codex adapter**

Define the stable types in `providers/mod.rs` and serialize them with camelCase:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind { Codex, DeepSeek }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ProviderAuthSnapshot {
    Checking,
    NeedsSetup,
    LoginPending { #[serde(rename = "loginId")] login_id: String },
    Ready { label: Option<String>, plan: Option<String> },
    Error { code: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ProviderRuntimeSnapshot {
    Stopped,
    Starting,
    Ready { version: Option<String> },
    Error { code: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSnapshot {
    pub kind: ProviderKind,
    pub auth: ProviderAuthSnapshot,
    pub runtime: ProviderRuntimeSnapshot,
}

pub type BoxFuture<'a, T> =
    std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;
```

Move Codex-specific snapshot mapping into `providers/codex.rs`; keep the existing App Server calls unchanged. Replace frontend `account` and `codex` reads with `snapshot.provider.auth` and `snapshot.provider.runtime`. Emit neutral event names:

```rust
pub enum ProviderEvent {
    TextDelta { item_id: String, text: String },
    ToolCompleted { item: serde_json::Value },
    TurnCompleted { turn_id: String },
}
```

Add `codex_home` to `CodexStartOptions` and call `.env("CODEX_HOME", &options.codex_home)` on the child command only. `resolve_codex_command` must use this order: explicit environment override, adjacent bundled `codex.exe`, then development fallback `codex.cmd`.

- [ ] **Step 4: Run Codex and frontend regression tests**

Run:

```powershell
Set-Location apps\desktop\src-tauri
cargo test --test provider_contract
cargo test --test codex_client
cargo test --test command_lifecycle
cargo test --test desktop_commands
Set-Location ..
npm.cmd test
npm.cmd run build
```

Expected: all tests pass; existing ChatGPT login, conversation, tool events and ArcGIS readiness retain their prior behavior through the neutral snapshot.

- [ ] **Step 5: Commit the provider seam**

```powershell
git add apps/desktop/src-tauri/src apps/desktop/src-tauri/tests/provider_contract.rs apps/desktop/src apps/desktop/tests
git commit -m "refactor: isolate model provider contract"
```

---

### Task 2: Persist minimal settings and protect the DeepSeek Key

**Files:**
- Create: `apps/desktop/src-tauri/src/settings.rs`
- Create: `apps/desktop/src-tauri/src/credential_store.rs`
- Create: `apps/desktop/src-tauri/tests/settings_credentials.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/Cargo.lock`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/src/app_state.rs`
- Modify: `apps/desktop/src-tauri/src/commands.rs`

**Interfaces:**
- Consumes: `ProviderKind` from Task 1 and `DesktopState.local_app_data`.
- Produces: `AppSettings`, `SettingsStore`, `SecretStore`, `WindowsCredentialStore`, `provider_select`, `deepseek_configure`, `deepseek_clear`.

- [ ] **Step 1: Write failing settings and secret tests**

Create tests that serialize settings and use an in-memory secret fake:

```rust
#[test]
fn settings_never_serialize_a_deepseek_key() {
    let settings = AppSettings::default();
    let json = serde_json::to_string(&settings).unwrap();
    assert!(!json.to_ascii_lowercase().contains("api_key"));
    assert!(!json.to_ascii_lowercase().contains("apikey"));
    assert_eq!(settings.deepseek.base_url, "https://api.deepseek.com");
    assert_eq!(settings.deepseek.model, "deepseek-v4-flash");
}

#[tokio::test]
async fn configuring_deepseek_stores_only_the_secret_reference() {
    let secrets = MemorySecretStore::default();
    configure_deepseek(&secrets, "sk-test-not-a-real-key").await.unwrap();
    assert_eq!(secrets.get(DEEPSEEK_CREDENTIAL_TARGET).await.unwrap().as_deref(),
               Some("sk-test-not-a-real-key"));
}
```

Add a Windows-only round trip test with a unique dummy value and a guard that always deletes the credential target before returning.

- [ ] **Step 2: Verify the credential abstraction is missing**

Run:

```powershell
Set-Location apps\desktop\src-tauri
cargo test --test settings_credentials
```

Expected: compile failure for `AppSettings`, `SecretStore` and `configure_deepseek`.

- [ ] **Step 3: Implement versioned settings and Credential Manager storage**

Use this exact non-secret settings schema:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub schema_version: u32,
    pub active_provider: ProviderKind,
    pub arcgis_pro_root: Option<PathBuf>,
    pub deepseek: DeepSeekSettings,
    pub onboarding_complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepSeekSettings {
    pub base_url: String,
    pub model: String,
}
```

`SettingsStore::save` writes atomically to `%LOCALAPPDATA%\ArcGISProAgent\preview\settings.json`. `credential_store.rs` wraps `CredWriteW`, `CredReadW` and `CredDeleteW` behind:

```rust
pub trait SecretStore: Send + Sync {
    fn get<'a>(&'a self, target: &'a str) -> BoxFuture<'a, Result<Option<String>, SecretError>>;
    fn set<'a>(&'a self, target: &'a str, secret: &'a str) -> BoxFuture<'a, Result<(), SecretError>>;
    fn delete<'a>(&'a self, target: &'a str) -> BoxFuture<'a, Result<(), SecretError>>;
}

pub const DEEPSEEK_CREDENTIAL_TARGET: &str = "ArcGISProAgent.Preview.DeepSeek";
```

Reject empty, control-character, newline, shorter-than-16 or longer-than-512 Key values before calling Credential Manager. Add Windows crate feature `Win32_Security_Credentials`; do not add a plaintext fallback on Windows.

- [ ] **Step 4: Run security-focused tests and search for accidental persistence**

Run:

```powershell
Set-Location apps\desktop\src-tauri
cargo test --test settings_credentials
cargo test
Set-Location ..\..\..
rg -n "apiKey|api_key|DEEPSEEK_API_KEY" apps/desktop/src-tauri/src apps/desktop/src
```

Expected: tests pass; source hits are limited to transient command parameters, validation and redaction tests, with no settings field or logging statement containing the Key.

- [ ] **Step 5: Commit settings and secure credentials**

```powershell
git add apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.lock apps/desktop/src-tauri/src apps/desktop/src-tauri/tests/settings_credentials.rs
git commit -m "feat: store provider settings and DeepSeek credentials"
```

---

### Task 3: Discover, validate and launch ArcGIS Pro; own only this Add-In

**Files:**
- Create: `apps/desktop/src-tauri/src/arcgis_install.rs`
- Create: `apps/desktop/src-tauri/src/addin_install.rs`
- Create: `apps/desktop/src-tauri/tests/arcgis_install.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/Cargo.lock`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/src/app_state.rs`
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/settings.rs`
- Modify: `apps/desktop/src/domain.ts`
- Modify: `apps/desktop/src/desktopApi.ts`

**Interfaces:**
- Consumes: saved `AppSettings.arcgis_pro_root`, `%USERPROFILE%`, Tauri resource directory, existing Add-In GUID `{1A0481EA-3F43-4C98-B4B5-A58C727CD115}`.
- Produces: `ArcGisInstallation`, `ArcGisInstallSnapshot`, `discover_arcgis`, `choose_arcgis_executable`, `repair_addin`, `launch_arcgis`, `cleanup_owned_addin`.

- [ ] **Step 1: Write failing pure discovery and ownership tests**

Cover exact priority and deletion safety:

```rust
#[test]
fn discovery_prefers_valid_saved_then_registry_then_standard_candidates() {
    let chosen = choose_installation(
        [saved(), registry(), standard(), PathBuf::from(r"D:\arcgis_pro")],
        |root| root == &registry(),
    ).unwrap();
    assert_eq!(chosen.root, registry());
}

#[test]
fn a_directory_name_without_required_files_is_rejected() {
    let root = TestDir::new();
    std::fs::create_dir_all(root.path().join("bin")).unwrap();
    assert!(validate_installation(root.path()).is_err());
}

#[test]
fn cleanup_refuses_any_path_outside_the_exact_addin_guid_root() {
    let manifest = owned_manifest_for(r"C:\Users\test\Documents\ArcGIS\AddIns\other.esriAddinX");
    assert_eq!(cleanup_plan(&manifest, expected_addin_root()), CleanupPlan::Refuse);
}
```

Also test that a changed Add-In hash is preserved instead of blindly deleted.

- [ ] **Step 2: Run tests and verify the product discovery module is absent**

Run:

```powershell
Set-Location apps\desktop\src-tauri
cargo test --test arcgis_install
```

Expected: compile failure for the new discovery, Add-In ownership and cleanup functions.

- [ ] **Step 3: Implement discovery, Add-In repair and safe launch**

Use this validated result type:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArcGisInstallation {
    pub root: PathBuf,
    pub executable: PathBuf,
    pub version: Option<String>,
    pub source: ArcGisInstallSource,
}
```

`validate_installation` requires both `bin\ArcGISPro.exe` and `bin\ArcGIS.Core.dll`, canonicalizes the root and rejects non-files. Production candidate collection reads `HKLM\SOFTWARE\ESRI\ArcGISPro\InstallDir`, the standard `C:\Program Files\ArcGIS\Pro`, and `D:\arcgis_pro`; tests inject candidates and file predicates instead of touching the registry.

Install the packaged `ArcGISProAgent.AddIn.esriAddinX` atomically at:

```text
%USERPROFILE%\Documents\ArcGIS\AddIns\ArcGISPro\{1A0481EA-3F43-4C98-B4B5-A58C727CD115}\ArcGISProAgent.AddIn.esriAddinX
```

Write an ownership record with destination and SHA-256 under the app data directory. If ArcGIS Pro is already running and the Add-In hash must change, return `requiresRestart: true`; do not terminate the process. `launch_arcgis` uses `std::process::Command::new(validated.executable)` without shell concatenation.

- [ ] **Step 4: Run discovery tests plus existing installer ownership regression**

Run:

```powershell
Set-Location apps\desktop\src-tauri
cargo test --test arcgis_install
cargo test
Set-Location ..\..\..
powershell -ExecutionPolicy Bypass -File scripts\Test-InstallDev.ps1
```

Expected: all tests pass; existing development installer ownership behavior remains unchanged.

- [ ] **Step 5: Commit ArcGIS discovery and Add-In ownership**

```powershell
git add apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.lock apps/desktop/src-tauri/src apps/desktop/src-tauri/tests/arcgis_install.rs apps/desktop/src apps/desktop/src/desktopApi.ts
git commit -m "feat: discover ArcGIS Pro and manage the preview add-in"
```

---

### Task 4: Add a direct, bounded MCP stdio client for non-Codex providers

**Files:**
- Create: `apps/desktop/src-tauri/src/mcp/mod.rs`
- Create: `apps/desktop/src-tauri/src/mcp/client.rs`
- Create: `apps/desktop/src-tauri/src/arcgis_tool_client.rs`
- Create: `apps/desktop/src-tauri/tests/mcp_client.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/src/app_state.rs`
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/mcp_status.rs`
- Modify: `apps/desktop/src-tauri/src/paths.rs`
- Modify: `apps/desktop/src-tauri/src/runtime_secret.rs`

**Interfaces:**
- Consumes: `ArcGISProAgent.Mcp.exe`, runtime credential file, existing MCP status parser and exact 17-tool inventory.
- Produces: `McpRuntime`, `McpStartOptions`, `McpTool`, `McpToolResult`, `ArcGisToolClient`; Codex and direct-MCP implementations of the same health/tool seam.

- [ ] **Step 1: Write a fake-stdio contract test**

The fake child must capture and respond to these exact frames:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"arcgis-pro-agent-desktop","version":"0.2.0-preview.1"}}}
{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
```

Test that `McpRuntime::start_with_command` completes initialization, lists only the baseline 17 tool names, routes an out-of-order response by id, times out a hanging call in five seconds, redacts the runtime token from stderr and kills the child on shutdown.

- [ ] **Step 2: Run the MCP test and verify it fails before implementation**

```powershell
Set-Location apps\desktop\src-tauri
cargo test --test mcp_client -- --nocapture
```

Expected: compile failure because `mcp::McpRuntime` and `ArcGisToolClient` do not exist.

- [ ] **Step 3: Implement the smallest reusable JSON-RPC runtime**

Expose this narrow contract:

```rust
pub trait ArcGisToolClient: Send + Sync {
    fn list_tools<'a>(&'a self) -> BoxFuture<'a, Result<Vec<McpTool>, ToolClientError>>;
    fn call_tool<'a>(
        &'a self,
        name: &'a str,
        arguments: Value,
    ) -> BoxFuture<'a, Result<McpToolResult, ToolClientError>>;
}
```

`McpRuntime` may reuse the proven request-id, bounded writer, pending-response and shutdown patterns from `codex/client.rs`, but it remains a separate module so it can later be replaced without modifying Codex. Spawn the MCP executable with only `ARCGIS_AGENT_RUNTIME_FILE` injected into the child. After `tools/list`, reject missing required baseline tools, duplicate names, names outside the 17-tool allowlist, oversized schemas and non-object input schemas.

Adapt health refresh to call `ArcGisToolClient::call_tool("arcgis_connection_status", {})`, `arcgis_describe_context` and `arcgis_list_layers`. The Codex adapter implements this trait by translating to its existing `mcpServer/tool/call`; the direct MCP adapter uses `tools/call`.

- [ ] **Step 4: Run direct MCP and Codex state-machine regressions**

```powershell
Set-Location apps\desktop\src-tauri
cargo test --test mcp_client
cargo test --test command_lifecycle
cargo test --test desktop_commands
cargo test --test codex_client
```

Expected: all pass; health polling no longer depends on a Codex-only type, while Codex behavior remains identical.

- [ ] **Step 5: Commit the direct MCP client**

```powershell
git add apps/desktop/src-tauri/src apps/desktop/src-tauri/tests/mcp_client.rs
git commit -m "feat: add provider-neutral ArcGIS MCP client"
```

---

### Task 5: Implement the bounded DeepSeek provider and tool loop

**Files:**
- Create: `apps/desktop/src-tauri/src/providers/deepseek_api.rs`
- Create: `apps/desktop/src-tauri/src/providers/deepseek.rs`
- Create: `apps/desktop/src-tauri/tests/deepseek_provider.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/Cargo.lock`
- Modify: `apps/desktop/src-tauri/src/providers/mod.rs`
- Modify: `apps/desktop/src-tauri/src/app_state.rs`
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `SecretStore`, `AppSettings.deepseek`, `ArcGisToolClient`, provider-neutral events from Task 1.
- Produces: `DeepSeekApi`, `DeepSeekProvider`, `DeepSeekSession`, `DeepSeekErrorCode`; working DeepSeek branches for `conversation_start`, `turn_start`, `turn_interrupt`.

- [ ] **Step 1: Write failing DeepSeek protocol and agent-loop tests**

Use a fake `DeepSeekTransport` and fake `ArcGisToolClient` so tests never use a real Key or network:

```rust
#[tokio::test]
async fn one_tool_call_is_executed_and_returned_to_the_model() {
    let transport = ScriptedTransport::new([
        model_tool_call("call-1", "arcgis_list_layers", json!({})),
        model_text("当前地图包含 2 个图层。"),
    ]);
    let tools = Arc::new(RecordingToolClient::with_result(
        "arcgis_list_layers",
        json!({"count": 2}),
    ));
    let events = DeepSeekProvider::new(transport, tools.clone())
        .run_turn("列出图层")
        .await
        .unwrap();
    assert_eq!(tools.calls(), vec![("arcgis_list_layers", json!({}))]);
    assert!(events.iter().any(|event| matches!(event, ProviderEvent::TextDelta { .. })));
}

#[tokio::test]
async fn ninth_tool_round_is_rejected_without_calling_arcgis() {
    let provider = provider_with_repeated_tool_calls(9);
    assert_eq!(provider.run_turn("循环").await.unwrap_err().code(), "tool_loop_limit");
}
```

Also cover fragmented SSE lines, fragmented `tool_calls[*].function.arguments`, 401/403, 429, 5xx retry limit, unknown tool, invalid JSON arguments, cancellation, 20,000-byte input limit and secret redaction.

- [ ] **Step 2: Run tests and verify DeepSeek is not implemented**

```powershell
Set-Location apps\desktop\src-tauri
cargo test --test deepseek_provider -- --nocapture
```

Expected: compile failure for `DeepSeekProvider`, transport types and provider dispatch.

- [ ] **Step 3: Implement HTTP/SSE parsing and an eight-round agent loop**

Add:

```toml
futures-util = "0.3"
reqwest = { version = "0.12", default-features = false, features = ["json", "stream", "rustls-tls"] }
zeroize = { version = "1", features = ["derive"] }
```

Use the fixed endpoint `${base_url}/chat/completions`, bearer auth, configured model, `stream: true`, current conversation messages and MCP-derived tool definitions. The provider algorithm is exactly:

```rust
for round in 0..MAX_TOOL_ROUNDS {
    let response = api.complete(&messages, &tools, cancel.clone()).await?;
    emit_text_deltas(&response, &events)?;
    if response.tool_calls.is_empty() { return Ok(response.final_text); }
    for call in validate_tool_calls(response.tool_calls, &tools)? {
        let result = arcgis.call_tool(&call.name, call.arguments).await?;
        messages.push(tool_result_message(call.id, result));
    }
}
Err(DeepSeekError::ToolLoopLimit)
```

Set `MAX_TOOL_ROUNDS = 8`, request timeout to 90 seconds and MCP tool timeout to 30 seconds. Retry only 429 and 5xx at 250 ms, 1 s and 3 s; never retry 400, 401, 403 or invalid tool calls. Retain messages only in the active local session.

- [ ] **Step 4: Run DeepSeek, provider and full Rust tests**

```powershell
Set-Location apps\desktop\src-tauri
cargo test --test deepseek_provider
cargo test --test provider_contract
cargo test
```

Expected: all tests pass without a real API Key or outbound network request.

- [ ] **Step 5: Commit DeepSeek support**

```powershell
git add apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.lock apps/desktop/src-tauri/src apps/desktop/src-tauri/tests/deepseek_provider.rs
git commit -m "feat: add bounded DeepSeek ArcGIS provider"
```

---

### Task 6: Build the shortest first-run and provider-switching UI

**Files:**
- Create: `apps/desktop/src/components/SetupView.tsx`
- Create: `apps/desktop/src/components/ProviderSwitcher.tsx`
- Create: `apps/desktop/tests/setupFlow.test.tsx`
- Modify: `apps/desktop/src/domain.ts`
- Modify: `apps/desktop/src/desktopApi.ts`
- Modify: `apps/desktop/src/appStore.ts`
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/components/LoginView.tsx`
- Modify: `apps/desktop/src/components/Sidebar.tsx`
- Modify: `apps/desktop/src/components/ConversationPane.tsx`
- Modify: `apps/desktop/src/app.css`
- Modify: `apps/desktop/package.json`
- Modify: `apps/desktop/package-lock.json`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/Cargo.lock`
- Modify: `apps/desktop/src-tauri/capabilities/default.json`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/tests/App.test.tsx`
- Modify: `apps/desktop/tests/conversationFlow.test.tsx`
- Modify: `apps/desktop/tests/loginFlow.test.tsx`

**Interfaces:**
- Consumes: `DesktopSnapshot.provider`, ArcGIS installation snapshot, `provider_select`, `deepseek_configure`, ChatGPT login commands, `repair_addin`, `launch_arcgis`.
- Produces: one-page setup flow, one-click ArcGIS launch, minimal model switch entry, actionable connection labels.

- [ ] **Step 1: Write failing first-run UI tests**

Create tests for the only required user paths:

```tsx
it("lets a new user choose DeepSeek without starting Codex", async () => {
  render(<App initialSnapshot={newUserWithDetectedArcGis} />);
  fireEvent.click(screen.getByRole("button", { name: "DeepSeek API" }));
  fireEvent.change(screen.getByLabelText("DeepSeek API Key"), {
    target: { value: "sk-preview-not-a-real-key" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存并检查" }));
  await waitFor(() => expect(api.configureDeepSeek).toHaveBeenCalledOnce());
  expect(api.startChatGptLogin).not.toHaveBeenCalled();
});

it("offers manual selection when ArcGIS Pro is not found", async () => {
  render(<App initialSnapshot={newUserWithoutArcGis} />);
  expect(screen.getByText("未找到 ArcGIS Pro 3.7")).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "选择 ArcGISPro.exe" }));
  expect(api.chooseArcGisExecutable).toHaveBeenCalledOnce();
});
```

Cover ChatGPT choice, unsafe auth URL rejection, Add-In restart warning, one-click launch, provider switching after login and the labels “正在重连”“MCP 未就绪”“需要登录”；assert the old generic “ArcGIS 连接已过期” text is absent.

- [ ] **Step 2: Run the focused frontend tests and verify the setup UI is absent**

```powershell
Set-Location apps\desktop
npm.cmd test -- tests/setupFlow.test.tsx tests/loginFlow.test.tsx tests/conversationFlow.test.tsx
```

Expected: tests fail because `SetupView`, provider commands and the new status labels do not exist.

- [ ] **Step 3: Implement a single-page setup flow without extra settings screens**

`SetupView` renders four compact sections in order: ArcGIS detection, Add-In state, model choice, launch/self-check. Keep the API Key in component state only until `configureDeepSeek` resolves, then immediately clear it:

```tsx
const submitDeepSeek = async () => {
  try {
    await configureDeepSeek(apiKey);
  } finally {
    setApiKey("");
  }
};
```

Add `@tauri-apps/plugin-dialog` `2.7.2`, Rust `tauri-plugin-dialog = "2"`, initialize the plugin in `lib.rs`, and grant only `dialog:allow-open`. `chooseArcGisExecutable` opens a single-file dialog filtered to `.exe`, then passes the returned absolute path to the backend validator; cancellation is not an error.

Do not claim JavaScript strings can be securely zeroized; JavaScript strings are immutable, so the frontend guarantee is limited to clearing component state and never persisting or logging the value. The actual persistence guarantee remains in the Rust backend and Credential Manager. `ProviderSwitcher` is a small button in the sidebar that returns to provider selection; it is not a general settings product. `ConversationPane` derives labels from structured connection states and never displays raw backend errors.

- [ ] **Step 4: Run all frontend tests and production frontend build**

```powershell
Set-Location apps\desktop
npm.cmd test
npm.cmd run build
```

Expected: all tests and TypeScript/Vite build pass; both provider flows reach the same conversation shell.

- [ ] **Step 5: Commit the minimal setup UI**

```powershell
git add apps/desktop/package.json apps/desktop/package-lock.json apps/desktop/src apps/desktop/tests apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.lock apps/desktop/src-tauri/capabilities/default.json apps/desktop/src-tauri/src/lib.rs
git commit -m "feat: add minimal ArcGIS and provider setup flow"
```

---

### Task 7: Produce a repeatable NSIS installer, shortcut and safe uninstaller

**Files:**
- Create: `apps/desktop/src-tauri/tauri.preview.conf.json`
- Create: `apps/desktop/src-tauri/windows/hooks.nsh`
- Create: `scripts/Build-Preview.ps1`
- Create: `scripts/Test-PreviewPackaging.ps1`
- Create: `Open-Project.ps1`
- Create: `THIRD_PARTY_NOTICES.md`
- Create: `licenses/codex-cli/LICENSE`
- Create: `licenses/nicogis-mcp-arcgis-pro-addin/LICENSE`
- Modify: `.gitignore`
- Modify: `apps/desktop/package.json`
- Modify: `apps/desktop/package-lock.json`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/Cargo.lock`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/src/main.rs`
- Modify: `apps/desktop/src-tauri/src/paths.rs`
- Modify: `apps/desktop/src-tauri/src/addin_install.rs`
- Modify: `apps/desktop/src-tauri/tauri.conf.json`

**Interfaces:**
- Consumes: built Add-In, self-contained MCP executable, Codex CLI `0.144.5`, application cleanup command and Tauri 2 NSIS bundler.
- Produces: `Build-Preview.ps1`, `--uninstall-cleanup`, `ArcGISPro智能助手-0.2.0-preview.1-x64-setup.exe`, SHA-256 file, desktop/start-menu entry.

- [ ] **Step 1: Write packaging assertions before enabling bundle output**

`scripts/Test-PreviewPackaging.ps1` must assert:

```powershell
$config = Get-Content $PreviewConfig -Raw | ConvertFrom-Json
Assert-Equal $config.version '0.2.0-preview.1' 'preview version'
Assert-True $config.bundle.active 'bundle enabled'
Assert-True ($config.bundle.targets -contains 'nsis') 'NSIS target'
Assert-Equal $config.bundle.windows.nsis.installMode 'currentUser' 'per-user install'
Assert-True ($config.bundle.externalBin -contains 'generated/preview/codex') 'Codex sidecar'
Assert-True ($config.bundle.externalBin -contains 'generated/preview/ArcGISProAgent.Mcp') 'MCP sidecar'
Assert-NoTextMatch -Paths @($PreviewConfig, $BuildScript) -Pattern 'C:\\Program Files\\ArcGIS|D:\\arcgis_pro'
```

Also assert the NSIS uninstall hook invokes only `$INSTDIR\arcgis-pro-agent-desktop.exe --uninstall-cleanup` and deletes only the named desktop shortcut.

- [ ] **Step 2: Run packaging tests and verify preview config/scripts are absent**

```powershell
powershell -ExecutionPolicy Bypass -File scripts\Test-PreviewPackaging.ps1
```

Expected: failure because the preview config, build script and hook do not exist.

- [ ] **Step 3: Implement deterministic staging and Tauri NSIS configuration**

Use this overlay shape in `tauri.preview.conf.json`:

```json
{
  "productName": "ArcGIS Pro 智能助手（预览版）",
  "version": "0.2.0-preview.1",
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "externalBin": [
      "generated/preview/codex",
      "generated/preview/ArcGISProAgent.Mcp"
    ],
    "resources": [
      "generated/preview/ArcGISProAgent.AddIn.esriAddinX",
      "../../../THIRD_PARTY_NOTICES.md",
      "../../../licenses/codex-cli/LICENSE",
      "../../../licenses/nicogis-mcp-arcgis-pro-addin/LICENSE"
    ],
    "windows": {
      "nsis": {
        "installMode": "currentUser",
        "languages": ["SimpChinese", "English"],
        "startMenuFolder": "ArcGIS Pro 智能助手",
        "installerHooks": "windows/hooks.nsh"
      }
    }
  }
}
```

`Build-Preview.ps1` must:

1. resolve ArcGIS Pro with the existing development resolver only for compiling the Add-In;
2. verify `codex.exe --version` equals `codex-cli 0.144.5`;
3. run `Test-Foundation.ps1`;
4. publish MCP as `win-x64`, self-contained, single-file and untrimmed;
5. build the Add-In in Release;
6. inspect the `.esriAddinX` archive and fail if it contains any Esri `ArcGIS.*.dll` runtime assembly;
7. stage target-triple-suffixed sidecars and Add-In under ignored `generated/preview`;
8. run `npm.cmd ci` and `npm.cmd run tauri -- build --config src-tauri/tauri.preview.conf.json --bundles nsis --target x86_64-pc-windows-msvc`;
9. copy the installer to `artifacts/preview`, rename it exactly, and write a lowercase SHA-256 plus filename.

`main.rs` recognizes `--uninstall-cleanup` before Tauri startup and calls only ownership-aware cleanup functions. `hooks.nsh` creates/removes the exact desktop shortcut and invokes cleanup before the main binary is removed.

`Open-Project.ps1` resolves `McpServer.sln` relative to `$PSScriptRoot` and opens it with `devenv.exe` when available, otherwise with the Windows file association; it contains no repository or ArcGIS hard-coded path.

`licenses/codex-cli/LICENSE` and `licenses/nicogis-mcp-arcgis-pro-addin/LICENSE` are verbatim copies from the corresponding pinned upstream source revisions. `THIRD_PARTY_NOTICES.md` names both components, their revisions, licenses and source URLs; it also states that no Esri ArcGIS runtime assembly is redistributed.

- [ ] **Step 4: Validate config, build the installer and inspect its inputs**

```powershell
powershell -ExecutionPolicy Bypass -File scripts\Test-PreviewPackaging.ps1
$codexPackage = Join-Path (Split-Path (Get-Command codex.cmd -ErrorAction Stop).Source) 'node_modules\@openai\codex'
$previewCodexExe = Get-ChildItem -LiteralPath $codexPackage -Recurse -Filter codex.exe -File | Select-Object -First 1 -ExpandProperty FullName
powershell -ExecutionPolicy Bypass -File scripts\Build-Preview.ps1 -ArcGISProInstallDir D:\arcgis_pro -CodexExe $previewCodexExe
Get-FileHash artifacts\preview\ArcGISPro智能助手-0.2.0-preview.1-x64-setup.exe -Algorithm SHA256
git status --short
```

Expected: packaging tests pass; installer and recorded SHA-256 match; staged binaries and `artifacts/preview` are ignored; only intended source/config files appear in Git status.

- [ ] **Step 5: Commit production packaging sources, not generated binaries**

```powershell
git add .gitignore Open-Project.ps1 THIRD_PARTY_NOTICES.md licenses/codex-cli/LICENSE licenses/nicogis-mcp-arcgis-pro-addin/LICENSE scripts/Build-Preview.ps1 scripts/Test-PreviewPackaging.ps1 apps/desktop/package.json apps/desktop/package-lock.json apps/desktop/src-tauri
git commit -m "build: package the minimal Windows preview"
```

---

### Task 8: Add minimum diagnostics, user documentation and final acceptance evidence

**Files:**
- Create: `apps/desktop/src-tauri/src/diagnostics.rs`
- Create: `apps/desktop/src-tauri/tests/diagnostics.rs`
- Create: `docs/development/preview-user-guide.md`
- Create: `docs/development/preview-smoke.md`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src/domain.ts`
- Modify: `apps/desktop/src/desktopApi.ts`
- Modify: `apps/desktop/src/components/Sidebar.tsx`
- Modify: `apps/desktop/tests/App.test.tsx`
- Modify: `README.md`
- Modify: `scripts/Test-Foundation.ps1`

**Interfaces:**
- Consumes: provider, ArcGIS, Add-In, Bridge and MCP structured snapshots; preview build script and installer artifact.
- Produces: `diagnostic_summary`, `export_diagnostics`, “复制诊断信息”, local sanitized JSON export, user/open-project manual and signed-off smoke record.

- [ ] **Step 1: Write failing diagnostics redaction tests**

Create a snapshot containing deliberate canaries and assert none survive:

```rust
#[test]
fn diagnostic_export_contains_status_but_no_secrets_or_user_paths() {
    let output = build_diagnostics(&fixture_with_canaries(
        "sk-secret-canary",
        r"C:\Users\Alice\SecretProject\map.aprx",
        "alice@example.com",
    ));
    let json = serde_json::to_string(&output).unwrap();
    assert!(json.contains("0.2.0-preview.1"));
    assert!(json.contains("bridge_reconnecting"));
    assert!(!json.contains("sk-secret-canary"));
    assert!(!json.contains("Alice"));
    assert!(!json.contains("alice@example.com"));
    assert!(!json.contains("map.aprx"));
}
```

Frontend test clicks “复制诊断信息” and asserts the clipboard receives only the backend-produced summary, never locally cached messages or API input.

- [ ] **Step 2: Run diagnostics tests and verify commands are absent**

```powershell
Set-Location apps\desktop\src-tauri
cargo test --test diagnostics
Set-Location ..
npm.cmd test -- tests/App.test.tsx
```

Expected: failure for missing diagnostics functions and UI controls.

- [ ] **Step 3: Implement only local, opt-in diagnostics and concise manuals**

`DiagnosticBundle` contains only:

```rust
pub struct DiagnosticBundle {
    pub app_version: String,
    pub provider_kind: ProviderKind,
    pub provider_status: String,
    pub arcgis_install_status: String,
    pub arcgis_pro_version: Option<String>,
    pub add_in_version: Option<String>,
    pub bridge_status: String,
    pub mcp_status: String,
    pub public_error_codes: Vec<String>,
    pub generated_at: String,
}
```

`diagnostic_summary` returns text for explicit clipboard copy. `export_diagnostics` writes the same structure as JSON under `%USERPROFILE%\Documents\ArcGISProAgent\diagnostics\` and returns the path; it does not export raw runtime logs, messages, tool payloads or full paths.

The user guide begins with exactly three end-user actions: run setup, open the desktop shortcut, choose DeepSeek or ChatGPT and launch ArcGIS Pro. It documents switching, reconnecting and uninstalling. The developer section points to `Open-Project.ps1` and `McpServer.sln`. The smoke document records each acceptance item as `PASS` or `FAIL` with date and version; it never records credentials or GIS content.

- [ ] **Step 4: Run the full automated and live preview acceptance suite**

Run automated verification:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\Test-Foundation.ps1 -ArcGISProInstallDir D:\arcgis_pro
powershell -ExecutionPolicy Bypass -File scripts\Test-PreviewPackaging.ps1
Set-Location apps\desktop
npm.cmd test
npm.cmd run build
Set-Location src-tauri
cargo test
Set-Location ..\..\..
git diff --check
```

Then install `artifacts\preview\ArcGISPro智能助手-0.2.0-preview.1-x64-setup.exe` on the test machine and perform these two user-controlled smoke paths:

1. choose DeepSeek, enter a real Key through the UI, launch ArcGIS Pro 3.7 and run “列出当前地图图层”；
2. clear/switch provider, sign in through the official ChatGPT browser flow, launch or reconnect ArcGIS Pro and run the same read-only prompt.

Finally restart ArcGIS Pro once, confirm “正在重连” recovers, export diagnostics and inspect for secrets, uninstall from Windows Settings, and confirm `.aprx`, geodatabases and unrelated Add-Ins remain.

Expected: all automated commands exit 0; every required smoke row is `PASS`; the installer SHA-256 matches the delivered checksum.

- [ ] **Step 5: Commit diagnostics, manuals and acceptance record**

```powershell
git add apps/desktop/src-tauri/src apps/desktop/src-tauri/tests/diagnostics.rs apps/desktop/src apps/desktop/tests/App.test.tsx docs/development README.md scripts/Test-Foundation.ps1
git commit -m "docs: finish preview diagnostics and acceptance guide"
```

---

## Final Review Gate

Before declaring the preview complete, verify the branch contains only the eight intentional task commits after design/plan documentation, compare `git diff 07f8dfb...HEAD`, and confirm no generated installer, API Key, auth state, user email, absolute user path or live GIS data is tracked. Do not merge, push or publish until the user reviews the installed preview and explicitly authorizes the next action.

# ChatGPT-only External Codex Preview Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 交付一个安装、启动和卸载都足够简单的 Windows x64 预览版，自动发现 ArcGIS Pro 3.7 与用户已安装的 Codex CLI，通过应用私有 ChatGPT 登录连接现有 17 个 ArcGIS 工具。

**Architecture:** 保留已验收的 Add-In、Bridge、MCP 与 Codex App Server 链路，在 Rust 后端增加外部 Codex CLI 发现器与可重试运行时协调器。React 只消费结构化状态并呈现单页首次使用流程；NSIS 只打包应用私有 MCP 和 Add-In，不打包、不安装也不删除外部 Codex。

**Tech Stack:** Windows 10/11 x64、ArcGIS Pro 3.7、Tauri 2.11、Rust 2024、Tokio、React 19、TypeScript 7、Vitest 4、`@tauri-apps/plugin-dialog 2.7.2`、.NET 8 MCP/Bridge、.NET 10 Add-In、NSIS、外部 Codex CLI（起始验收版本 `0.149.0`）。

## Global Constraints

- 只在 `feature/distributable-preview-deepseek` 和 `.worktrees/distributable-preview-deepseek` 中实施；不得修改或合并 `master` 与 `feature/arcgis-pro-agent-foundation`。
- 预览版本固定为 `0.2.0-preview.1`，目标为 Windows 10/11 x64 与 ArcGIS Pro 3.7。
- DeepSeek API、HTTP、工具循环、API Key UI、提供商选择和切换全部延期；发布命令表不得暴露 DeepSeek 配置或切换命令。
- 安装包不得包含 `codex.exe`、`codex.cmd`、Codex npm 包或 Codex 桌面应用内部文件。
- 外部 Codex 只从当前 `PATH` 和当前用户 npm 全局命令目录发现；不得扫描磁盘、接受前端任意路径或使用 WindowsApps 私有路径。
- 非 `0.149.0` 版本只产生非阻断警告；必须通过真实 App Server 初始化和 `account/read` 自检才能进入可用状态。
- Codex 子进程只使用 `%LOCALAPPDATA%\ArcGISProAgent\codex`，不得读写用户全局 Codex 身份或修改父进程/系统环境。
- MCP 可执行文件和 Add-In 只能来自应用私有安装资源；保留 Task 4 的有界生命周期、精确 17 工具清单和 fail-closed 行为。
- Add-In 继续采用 Option A：通过 Esri 官方文件关联打开固定包；安装器和卸载器不得直接写入或递归删除 ArcGIS AddIns 根目录。
- 不新增或改变现有 17 个 R0/R1 工具语义；不增加保存、导出、地理处理或源数据编辑。
- 所有功能和修复先观察失败测试，再做最小实现；每个任务独立提交并经过规格审查和代码质量审查。
- 不提交生成的安装包、sidecar、`target`、`dist`、身份数据、用户路径、真实 GIS 数据或烟雾测试中的账号信息。
- nicogis 上游来源固定记录为提交 `e3383804d1682ed56b8e9dffda3e639064fb5230`；分发包必须包含该提交的 MIT `LICENSE` 原文和来源说明。
- 不推送、不合并、不创建公开 Release；完成本地预览安装和验收后等待用户决定。

## File Responsibility Map

- `apps/desktop/src-tauri/src/codex/discovery.rs`：外部 Codex 候选枚举、绝对路径验证、版本探测和版本置信度。
- `apps/desktop/src-tauri/src/codex/client.rs`：只负责已验证命令的 App Server JSONL 生命周期，不负责发现。
- `apps/desktop/src-tauri/src/app_state.rs`：ChatGPT-only 状态、运行时重启串行化、世代隔离。
- `apps/desktop/src-tauri/src/commands.rs`：Tauri 命令编排、Codex 重检、ArcGIS 发现和现有会话命令。
- `apps/desktop/src/components/SetupView.tsx`：唯一首次使用页面，不包含通用设置或提供商切换。
- `apps/desktop/src/desktopApi.ts`：前端可调用的最小 Tauri 命令表和文件选择边界。
- `apps/desktop/src-tauri/src/cleanup.rs`：卸载时只清理经过验证的应用私有数据根。
- `scripts/Build-Preview.ps1`：测试、.NET 发布、Add-In 构建、资源暂存、NSIS 构建和校验和。
- `scripts/Test-PreviewPackaging.ps1`：不安装即可验证打包输入、所有权和无 Codex/DeepSeek 约束。
- `docs/development/preview-user-guide.md`：面向试用者的最短安装、登录、启动、重连和卸载说明。
- `docs/development/preview-smoke.md`：只记录版本、日期和 PASS/FAIL 的实机验收证据。

---

### Task 5: Discover and validate an external Codex CLI

**Files:**
- Create: `apps/desktop/src-tauri/src/codex/discovery.rs`
- Create: `apps/desktop/src-tauri/tests/codex_discovery.rs`
- Modify: `apps/desktop/src-tauri/src/codex/mod.rs`

**Interfaces:**
- Consumes: `PATH`, `%APPDATA%\npm` and a bounded process runner.
- Produces:

```rust
pub const TESTED_CODEX_VERSION: &str = "0.149.0";
pub const CODEX_INSTALL_URL: &str = "https://learn.chatgpt.com/docs/codex/cli";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexVersionConfidence {
    Tested,
    Unverified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexInstallation {
    pub command: PathBuf,
    pub version: String,
    pub confidence: CodexVersionConfidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexDiscoveryError {
    NotFound,
    Invalid,
}

pub trait CodexVersionProbe: Send + Sync {
    fn probe<'a>(&'a self, command: &'a Path)
        -> BoxFuture<'a, Result<String, CodexDiscoveryError>>;
}

pub fn codex_candidates(
    path: Option<&OsStr>,
    roaming_app_data: Option<&Path>,
) -> Vec<PathBuf>;
pub async fn discover_codex_with(
    path: Option<&OsStr>,
    roaming_app_data: Option<&Path>,
    probe: &dyn CodexVersionProbe,
) -> Result<CodexInstallation, CodexDiscoveryError>;
pub async fn discover_codex()
    -> Result<CodexInstallation, CodexDiscoveryError>;
```

- [ ] **Step 1: Write candidate, validation and bounded-process tests**

Create a `RecordingProbe` in `codex_discovery.rs` whose reply map is keyed by canonical absolute path. Add these named tests:

```rust
#[tokio::test]
async fn path_candidate_wins_before_the_user_npm_shim() {
    let fixture = CodexFixture::new();
    let path_command = fixture.file("path/codex.exe");
    let npm_command = fixture.file("appdata/npm/codex.cmd");
    let path = std::env::join_paths([path_command.parent().unwrap()]).unwrap();
    let probe = RecordingProbe::from([
        (path_command.clone(), Ok("codex-cli 0.149.0")),
        (npm_command, Ok("codex-cli 0.149.0")),
    ]);

    let found = discover_codex_with(Some(&path), Some(&fixture.app_data()), &probe)
        .await
        .unwrap();

    assert_eq!(found.command, std::fs::canonicalize(path_command).unwrap());
    assert_eq!(found.version, "0.149.0");
    assert_eq!(found.confidence, CodexVersionConfidence::Tested);
}

#[tokio::test]
async fn a_different_well_formed_version_is_unverified_not_rejected() {
    let fixture = CodexFixture::new();
    let command = fixture.file("path/codex.cmd");
    let probe = RecordingProbe::one(command.clone(), Ok("codex-cli 0.150.1"));
    let path = std::env::join_paths([command.parent().unwrap()]).unwrap();

    let found = discover_codex_with(Some(&path), Some(&fixture.app_data()), &probe)
        .await
        .unwrap();

    assert_eq!(found.version, "0.150.1");
    assert_eq!(found.confidence, CodexVersionConfidence::Unverified);
}

#[tokio::test]
async fn invalid_candidates_are_skipped_and_never_executed_by_bare_name() {
    let fixture = CodexFixture::new();
    fixture.directory("path/codex.exe");
    let path = std::env::join_paths([fixture.path("path")]).unwrap();
    let probe = RecordingProbe::default();

    let error = discover_codex_with(Some(&path), Some(&fixture.app_data()), &probe)
        .await
        .unwrap_err();

    assert_eq!(error, CodexDiscoveryError::NotFound);
    assert!(probe.calls().is_empty());
}
```

`BoxFuture` reuses `crate::providers::BoxFuture`; Task 5 adds no dependency.

Add a real helper-process test that emits more than 4096 bytes or waits forever. Assert `ProcessCodexVersionProbe` returns `Invalid` within six seconds and the captured PID no longer exists. Add source invariants that reject `WindowsApps`, recursive filesystem enumeration, `ARCGIS_AGENT_CODEX_COMMAND` and a bare `Command::new("codex")` in production discovery.

- [ ] **Step 2: Run the focused test and observe RED**

Run:

```powershell
Set-Location apps\desktop\src-tauri
cargo test --test codex_discovery -- --nocapture
```

Expected: compile failure because `codex::discovery`, `CodexVersionProbe` and `discover_codex_with` do not exist.

- [ ] **Step 3: Implement deterministic candidate enumeration and parsing**

In `codex/discovery.rs`, enumerate `codex.exe` before `codex.cmd` in each `PATH` directory, then append `%APPDATA%\npm\codex.cmd`. Deduplicate after canonicalization and accept only ordinary files whose leaf name is exactly `codex.exe` or `codex.cmd`, case-insensitively.

Parse only this bounded grammar:

```rust
fn parse_version(value: &str) -> Option<String> {
    let version = value.trim().strip_prefix("codex-cli ")?;
    let mut parts = version.split('.');
    let valid = parts.clone().count() == 3
        && parts.all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    valid.then(|| version.to_owned())
}
```

Return the canonical absolute command path. Do not persist the path and do not fall back to a bare command name.

The production `discover_codex()` wrapper reads the current process `PATH` and optional `APPDATA`, then delegates to `discover_codex_with` with `ProcessCodexVersionProbe`. A missing `APPDATA` only disables the npm fallback; valid `PATH` candidates remain usable.

- [ ] **Step 4: Implement the bounded real process probe**

`ProcessCodexVersionProbe` must spawn the exact validated path with only `--version`, remove `OPENAI_API_KEY`, `AZURE_OPENAI_API_KEY` and `CODEX_API_KEY` from the child, and set `kill_on_drop(true)`. Read stdout and stderr concurrently through a 4097-byte limiter; reject either stream when it exceeds 4096 bytes. Apply a five-second total timeout, then kill and wait for the child before returning `Invalid`.

Use this exact limiter:

```rust
async fn read_limited(
    reader: impl tokio::io::AsyncRead + Unpin,
) -> std::io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut bytes = Vec::new();
    reader.take(4097).read_to_end(&mut bytes).await?;
    if bytes.len() > 4096 {
        return Err(std::io::Error::other("version output too large"));
    }
    Ok(bytes)
}
```

Only UTF-8 stdout with exit code 0 and the exact version grammar succeeds. Never surface captured stderr.

- [ ] **Step 5: Run focused and regression tests**

Run:

```powershell
cargo test --test codex_discovery -- --nocapture
cargo test --test codex_client --test provider_contract
cargo fmt -- --check
git diff --check
```

Expected: all active tests pass; the helper-process test is ignored only when it is being launched as the fake child.

- [ ] **Step 6: Commit external discovery**

```powershell
git add apps/desktop/src-tauri/src/codex/discovery.rs apps/desktop/src-tauri/src/codex/mod.rs apps/desktop/src-tauri/tests/codex_discovery.rs
git commit -m "feat: detect external Codex CLI"
```

---

### Task 6: Make runtime startup retryable and ChatGPT-only

**Files:**
- Modify: `apps/desktop/src-tauri/src/app_state.rs`
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/src/providers/codex.rs`
- Modify: `apps/desktop/src-tauri/src/providers/mod.rs`
- Modify: `apps/desktop/src-tauri/src/settings.rs`
- Modify: `apps/desktop/src-tauri/src/paths.rs`
- Modify: `apps/desktop/src-tauri/tests/command_lifecycle.rs`
- Modify: `apps/desktop/src-tauri/tests/desktop_commands.rs`
- Modify: `apps/desktop/src-tauri/tests/provider_contract.rs`
- Modify: `apps/desktop/src-tauri/tests/settings_credentials.rs`

**Interfaces:**
- Consumes: `CodexInstallation` from Task 5 and existing `CodexRuntime`.
- Produces:

```rust
pub enum CodexSnapshot {
    Starting,
    Ready {
        version: String,
        version_verified: bool,
    },
    Error {
        code: String,
    },
}

pub enum ProviderRuntimeSnapshot {
    Stopped,
    Starting,
    Ready {
        version: Option<String>,
        version_verified: bool,
    },
    Error { code: String },
}

#[tauri::command]
pub async fn rediscover_codex(
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String>;
```

- [ ] **Step 1: Write failing ChatGPT-only and retry lifecycle tests**

Add these contract tests:

```rust
#[tokio::test]
async fn persisted_deepseek_selection_is_normalized_to_codex_without_reading_a_secret() {
    let fixture = StateFixture::with_provider(ProviderKind::DeepSeek);
    let secrets = Arc::new(CountingSecretStore::default());
    let state = DesktopState::with_secret_store(fixture.local_app_data(), secrets.clone()).await;

    assert_eq!(state.snapshot().await.provider.kind, ProviderKind::Codex);
    assert_eq!(state.settings_store().load().unwrap().active_provider, ProviderKind::Codex);
    assert_eq!(secrets.read_count(), 0);
}

#[test]
fn production_handler_exposes_no_deepseek_or_provider_switch_command() {
    let source = std::fs::read_to_string("src/lib.rs").unwrap();
    for forbidden in [
        "commands::provider_select",
        "commands::deepseek_configure",
        "commands::deepseek_clear",
    ] {
        assert!(!source.contains(forbidden), "{forbidden} must not be registered");
    }
    assert!(source.contains("commands::rediscover_codex"));
}

#[test]
fn unverified_compatible_codex_is_serialized_as_ready_with_a_warning_flag() {
    let snapshot = codex_provider_snapshot(
        &AccountSnapshot::SignedOut,
        &CodexSnapshot::Ready {
            version: "0.150.1".to_owned(),
            version_verified: false,
        },
    );
    assert_eq!(
        serde_json::to_value(snapshot.runtime).unwrap(),
        json!({"status":"ready","version":"0.150.1","versionVerified":false})
    );
}
```

Add async lifecycle tests proving two concurrent rediscovery calls start one replacement runtime, the old runtime is shut down before publication, an old runtime's exit cannot overwrite the new epoch, and `account/read` failure maps to `codex_incompatible` rather than signed-out.

- [ ] **Step 2: Run the focused tests and observe RED**

Run:

```powershell
cargo test --test provider_contract --test command_lifecycle --test settings_credentials
```

Expected: failures for the old DeepSeek selection, old runtime snapshot shape, missing rediscovery command and stale runtime lifecycle.

- [ ] **Step 3: Normalize persisted settings to Codex**

Add this atomic operation to `SettingsStore`:

```rust
pub fn load_chatgpt_only(&self) -> Result<AppSettings, SettingsError> {
    let mut settings = self.load()?;
    if settings.active_provider != ProviderKind::Codex {
        settings.active_provider = ProviderKind::Codex;
        self.save(&settings)?;
    }
    Ok(settings)
}
```

Both `DesktopState::new` and `with_secret_store` must use it and must not inspect the DeepSeek credential. Keep the isolated credential module and its direct unit tests intact; only remove its production reachability.

- [ ] **Step 4: Add serialized restart coordination and epoch checks**

Add `runtime_restart: tokio::sync::Mutex<()>` to `DesktopState`. The restart mutex intentionally spans the entire discover/replace/self-test operation so only one replacement can exist. Capture the runtime epoch before waiting for the restart mutex; after acquiring it, skip work only when the epoch changed and the newer snapshot is already healthy `Ready`. This coalesces concurrent calls without preventing a later explicit recheck. Add methods that publish `Starting`, take and shut down the old runtime, and check whether a runtime epoch is still current.

Change the health poller signature to `run_health_poller(app, runtime_epoch)`. It exits when the epoch changes, the runtime disappears or the window lifecycle gate is cancelled. A Codex child exit must call `mark_runtime_stopped_if_epoch(runtime_epoch)`; it must not cancel the window lifecycle gate or overwrite a replacement runtime.

No coordinator/runtime `RwLock` or state guard may be held across `CodexRuntime::shutdown`, App Server startup, `account/read` or event-task joins. The dedicated `runtime_restart` mutex is the sole intentional guard across those awaits.

- [ ] **Step 5: Integrate discovery, compatibility self-test and rediscovery**

Refactor `initialize_runtime` into:

```rust
pub async fn initialize_runtime(app: AppHandle) {
    let state = app.state::<DesktopState>();
    discover_arcgis_with(&state).await;
    emit_snapshot(&app, state.snapshot().await);
    let _ = restart_codex_runtime(&app, &state).await;
}

#[tauri::command]
pub async fn rediscover_codex(
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<DesktopSnapshot, String> {
    restart_codex_runtime(&app, &state).await;
    Ok(state.snapshot().await)
}
```

`restart_codex_runtime` must:

1. discover an absolute external Codex command through Task 5;
2. map no candidate to `codex_not_found` and invalid probes to `codex_invalid`;
3. start `CodexRuntime` with the application-private `CODEX_HOME` and application-private MCP executable;
4. start the event consumer before issuing `account/read`;
5. require `account/read` to succeed inside the existing bounded request path;
6. publish `Ready` with `version_verified = (confidence == Tested)`;
7. map startup, protocol or account self-test failure to `codex_incompatible`;
8. kill and join failed children before returning.

Keep development MCP overrides behind `cfg(debug_assertions)`. In release, resolve only the installed sibling `ArcGISProAgent.Mcp.exe` and reject a missing or redirected file.

- [ ] **Step 6: Remove DeepSeek commands from the production Tauri surface**

In `lib.rs`, the handler list must contain only:

```rust
tauri::generate_handler![
    commands::desktop_snapshot,
    commands::rediscover_codex,
    commands::discover_arcgis,
    commands::choose_arcgis_executable,
    commands::open_addin_installer,
    commands::launch_arcgis,
    commands::addin_uninstall_guidance,
    commands::chatgpt_login_start,
    commands::chatgpt_login_cancel,
    commands::chatgpt_logout,
    commands::conversation_start,
    commands::turn_start,
    commands::turn_interrupt,
]
```

Delete the three Tauri wrapper functions for provider selection and DeepSeek configuration. Retain only isolated backend helpers needed by existing credential tests, without `#[tauri::command]`.

- [ ] **Step 7: Run all Rust tests and commit**

Run:

```powershell
Set-Location apps\desktop\src-tauri
cargo test
cargo check --release
cargo fmt -- --check
git diff --check
```

Expected: every active Rust test passes; Windows Credential Manager tests are rerun in the interactive Administrator logon if the sandbox identity lacks a logon session.

Commit:

```powershell
git add apps/desktop/src-tauri/src apps/desktop/src-tauri/tests apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.lock
git commit -m "feat: add ChatGPT-only Codex runtime recovery"
```

---

### Task 7: Add the one-page convenient setup flow

**Files:**
- Create: `apps/desktop/src/components/SetupView.tsx`
- Create: `apps/desktop/tests/setupFlow.test.tsx`
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/app.css`
- Modify: `apps/desktop/src/appStore.ts`
- Modify: `apps/desktop/src/components/LoginView.tsx`
- Modify: `apps/desktop/src/components/Sidebar.tsx`
- Modify: `apps/desktop/src/desktopApi.ts`
- Modify: `apps/desktop/src/domain.ts`
- Modify: `apps/desktop/tests/App.test.tsx`
- Modify: `apps/desktop/tests/loginFlow.test.tsx`
- Modify: `apps/desktop/tests/conversationFlow.test.tsx`
- Modify: `apps/desktop/package.json`
- Modify: `apps/desktop/package-lock.json`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/Cargo.lock`
- Modify: `apps/desktop/src-tauri/capabilities/default.json`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: structured provider/runtime, ArcGIS installation and Bridge snapshots from Task 6.
- Produces:

```ts
export async function rediscoverCodex(): Promise<DesktopSnapshot>;
export async function selectArcGisExecutable(): Promise<ArcGisInstallSnapshot | null>;

export type SetupViewProps = {
  snapshot: DesktopSnapshot;
  loginError?: string;
  actionError?: string;
  onRediscoverCodex: () => void;
  onDiscoverArcGis: () => void;
  onSelectArcGis: () => void;
  onOpenAddIn: () => void;
  onLogin: () => void;
  onLaunchArcGis: () => void;
};
```

- [ ] **Step 1: Write failing end-user path tests**

Mock the exact API surface and add these representative tests:

```tsx
it("shows only ChatGPT and guides a missing Codex user", async () => {
  render(<App initialSnapshot={missingCodexSnapshot} />);

  expect(screen.getByText("未找到 Codex CLI")).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "查看官方安装说明" }));
  expect(api.openExternalUrl).toHaveBeenCalledWith(
    "https://learn.chatgpt.com/docs/codex/cli",
  );
  expect(screen.queryByText(/DeepSeek/i)).not.toBeInTheDocument();
  expect(screen.queryByLabelText(/API Key/i)).not.toBeInTheDocument();
});

it("redetects Codex when the window regains focus", async () => {
  render(<App initialSnapshot={missingCodexSnapshot} />);
  fireEvent.focus(window);
  await waitFor(() => expect(api.rediscoverCodex).toHaveBeenCalledOnce());
});

it("allows an unverified compatible version and continues to ChatGPT login", async () => {
  render(<App initialSnapshot={unverifiedSignedOutSnapshot} />);
  expect(screen.getByText("Codex 0.150.1 未经本版验证")).toBeVisible();
  expect(
    screen.getByRole("button", { name: "使用 ChatGPT 账号登录" }),
  ).toBeEnabled();
});

it("offers a validated ArcGIS executable picker after automatic discovery fails", async () => {
  vi.mocked(api.selectArcGisExecutable).mockResolvedValue(readyArcGisInstall);
  render(<App initialSnapshot={arcGisMissingSnapshot} />);
  fireEvent.click(screen.getByRole("button", { name: "选择 ArcGISPro.exe" }));
  await waitFor(() => expect(api.selectArcGisExecutable).toHaveBeenCalledOnce());
});

it("launches ArcGIS from the single primary action and enters chat after connection", async () => {
  render(<App initialSnapshot={readyExceptBridgeSnapshot} />);
  fireEvent.click(screen.getByRole("button", { name: "启动 ArcGIS Pro 并连接" }));
  expect(api.launchArcGis).toHaveBeenCalledOnce();
  emitSnapshot(fullyReadySnapshot);
  expect(await screen.findByRole("main", { name: "对话" })).toBeVisible();
});
```

Also assert Add-In restart guidance, dialog cancellation, failed actions remaining retryable, completed cards collapsing, and absence of the generic text “ArcGIS 连接已过期”.

- [ ] **Step 2: Run focused frontend tests and observe RED**

Run:

```powershell
Set-Location apps\desktop
npm test -- tests/setupFlow.test.tsx tests/loginFlow.test.tsx tests/conversationFlow.test.tsx
```

Expected: failures because `SetupView`, `rediscoverCodex` and the file-dialog path do not exist.

- [ ] **Step 3: Add the minimum file-dialog capability**

Pin `@tauri-apps/plugin-dialog` to `2.7.2` and add `tauri-plugin-dialog = "2"`. Initialize it in `lib.rs` and grant only `dialog:allow-open`. Extend the existing `opener:allow-open-url` scope with exactly `https://learn.chatgpt.com/*` so the official Codex installation guide can open; do not grant a wildcard URL scope.

Implement:

```ts
export async function selectArcGisExecutable() {
  const executable = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "ArcGIS Pro", extensions: ["exe"] }],
  });
  if (typeof executable !== "string") return null;
  return chooseArcGisExecutable(executable);
}
```

The Rust validator remains authoritative; selecting another executable or a non-3.7 installation fails closed.

- [ ] **Step 4: Implement SetupView and structured labels**

Render five compact cards in this order: Codex CLI, ArcGIS Pro 3.7, ArcGIS Add-In, ChatGPT, launch/self-check. Derive all copy from structured states:

```ts
function codexLabel(runtime: ProviderRuntimeSnapshot) {
  if (runtime.status === "starting") return "正在检测 Codex CLI";
  if (runtime.status === "error" && runtime.code === "codex_not_found") {
    return "未找到 Codex CLI";
  }
  if (runtime.status === "error" && runtime.code === "codex_incompatible") {
    return "Codex 与当前版本不兼容";
  }
  if (runtime.status === "ready" && !runtime.versionVerified) {
    return "Codex " + (runtime.version ?? "未知版本") + " 未经本版验证";
  }
  return "Codex CLI 已就绪";
}
```

Do not add a settings screen. Keep Add-In action local state only: after opening the package, show the returned restart requirement until Bridge becomes connected. Conversation creation may run in the background after provider readiness so MCP discovery can complete; the conversation shell is shown only after ChatGPT, ArcGIS installation, conversation and live Bridge are ready.

- [ ] **Step 5: Add automatic recheck and convenient daily startup**

Register one window `focus` listener only while Codex is missing/invalid/incompatible. Debounce it with the same in-flight ref used by the “重新检测” button. On a fully ready snapshot, bypass `SetupView` and render the existing conversation shell immediately.

The sidebar model label is always “ChatGPT / Codex”; remove the DeepSeek conditional. Preserve official auth URL validation and all existing cancellation behavior.

- [ ] **Step 6: Run full frontend and Rust command-surface regression**

Run:

```powershell
Set-Location apps\desktop
npm test
npm run build
Set-Location src-tauri
cargo test --test desktop_commands --test command_lifecycle --test provider_contract
cargo fmt -- --check
git diff --check
```

Expected: all frontend tests and the production Vite build pass; Rust command and provider contracts pass.

- [ ] **Step 7: Commit the one-page setup**

```powershell
git add apps/desktop/package.json apps/desktop/package-lock.json apps/desktop/src apps/desktop/tests apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.lock apps/desktop/src-tauri/capabilities/default.json apps/desktop/src-tauri/src/lib.rs
git commit -m "feat: add convenient ChatGPT-only setup"
```

---

### Task 8: Build the NSIS preview and safe uninstaller

**Files:**
- Create: `apps/desktop/src-tauri/src/cleanup.rs`
- Create: `apps/desktop/src-tauri/tests/cleanup.rs`
- Create: `apps/desktop/src-tauri/tauri.preview.conf.json`
- Create: `apps/desktop/src-tauri/windows/hooks.nsh`
- Create: `scripts/Build-Preview.ps1`
- Create: `scripts/Test-PreviewPackaging.ps1`
- Create: `Open-Project.ps1`
- Create: `THIRD_PARTY_NOTICES.md`
- Create: `licenses/nicogis-mcp-arcgis-pro-addin/LICENSE`
- Modify: `.gitignore`
- Modify: `apps/desktop/package.json`
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/src/main.rs`
- Modify: `apps/desktop/src-tauri/src/paths.rs`
- Modify: `apps/desktop/src-tauri/src/credential_store.rs`
- Modify: `apps/desktop/src-tauri/tauri.conf.json`
- Modify: `src/ArcGISProAgent.AddIn/ArcGISProAgent.AddIn.csproj`
- Modify: `src/ArcGISProAgent.AddIn/Config.daml`
- Modify: `src/ArcGISProAgent.AddIn/AgentModule.cs`
- Modify: `src/ArcGISProAgent.Mcp/ArcGISProAgent.Mcp.csproj`
- Modify: `src/ArcGISProAgent.Mcp/Program.cs`

**Interfaces:**
- Consumes: self-contained .NET 8 MCP publish, .NET 10 Add-In build and Tauri NSIS bundler.
- Produces: `ArcGISPro智能助手-0.2.0-preview.1-x64-setup.exe`, lowercase SHA-256 file, shortcuts, `--uninstall-cleanup` and `Open-Project.ps1`.

- [ ] **Step 1: Write cleanup and packaging assertions**

The cleanup test must prove a reparse-point application root is rejected and its outside target remains unchanged:

```rust
#[test]
fn cleanup_rejects_a_redirected_application_data_root() {
    let fixture = WindowsJunctionFixture::new();
    let outside = fixture.outside_file("keep.txt");
    let result = cleanup_owned_data(fixture.local_app_data());
    assert_eq!(result, Err(CleanupError::UnsafeRoot));
    assert_eq!(std::fs::read_to_string(outside).unwrap(), "keep");
}
```

`Test-PreviewPackaging.ps1` must assert:

```powershell
$config = Get-Content -LiteralPath $PreviewConfig -Raw | ConvertFrom-Json
Assert-Equal $config.version '0.2.0-preview.1' 'preview version'
Assert-True $config.bundle.active 'bundle enabled'
Assert-True ($config.bundle.targets -contains 'nsis') 'NSIS target'
Assert-Equal $config.bundle.windows.nsis.installMode 'currentUser' 'per-user install'
Assert-True ($config.bundle.externalBin -contains 'generated/preview/ArcGISProAgent.Mcp') 'MCP sidecar'
Assert-False (($config.bundle.externalBin -join '|') -match '(?i)codex') 'Codex must stay external'
Assert-NoTextMatch -Paths @($PreviewConfig, $BuildScript) -Pattern 'CodexExe|WindowsApps|D:\\arcgis_pro'
Assert-NoTextMatch -Paths @($PreviewConfig, $BuildScript) -Pattern 'deepseek'
```

Also assert the hook creates/removes only the named desktop shortcut and invokes only `$INSTDIR\arcgis-pro-agent-desktop.exe --uninstall-cleanup` before binary removal.

- [ ] **Step 2: Run RED tests**

Run:

```powershell
Set-Location apps\desktop\src-tauri
cargo test --test cleanup
Set-Location ..\..\..
powershell -ExecutionPolicy Bypass -File scripts\Test-PreviewPackaging.ps1
```

Expected: the Rust test fails to compile and the PowerShell test fails because cleanup/config/build files do not exist.

- [ ] **Step 3: Implement exact owned-data cleanup**

`cleanup_owned_data(local_app_data)` may target only `local_app_data.join("ArcGISProAgent")`. Before `remove_dir_all`, inspect lexical metadata and reject symlinks or Windows `FILE_ATTRIBUTE_REPARSE_POINT`. Never canonicalize a redirected root into trust. Remove only the exact app root; treat a missing root as success.

Expose a narrow `clear_owned_credential_for_uninstall()` helper from `credential_store.rs` that deletes only the existing `DEEPSEEK_CREDENTIAL_TARGET` and treats a missing credential as success. `cleanup_for_uninstall_with(local_app_data, delete_owned_credential)` injects this operation for tests; production `cleanup_for_uninstall()` passes the exact helper. If either root validation or credential deletion fails, return nonzero without widening the cleanup target.

`main.rs` handles `--uninstall-cleanup` before Tauri startup:

```rust
fn main() {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--uninstall-cleanup")) {
        std::process::exit(arcgis_pro_agent_desktop_lib::cleanup_for_uninstall());
    }
    arcgis_pro_agent_desktop_lib::run();
}
```

Return nonzero on unsafe roots and leave them untouched for manual inspection.

- [ ] **Step 4: Add deterministic preview configuration and staging**

Use this overlay:

```json
{
  "productName": "ArcGIS Pro 智能助手（预览版）",
  "version": "0.2.0-preview.1",
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "externalBin": ["generated/preview/ArcGISProAgent.Mcp"],
    "resources": [
      "generated/preview/ArcGISProAgent.AddIn.esriAddInX",
      "../../../THIRD_PARTY_NOTICES.md",
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

Stage the MCP as `generated/preview/ArcGISProAgent.Mcp-x86_64-pc-windows-msvc.exe`, as required by Tauri `externalBin`. At runtime resolve only the installed sibling `ArcGISProAgent.Mcp.exe`. Keep generated staging and `artifacts/preview` ignored.

Update product metadata to `0.2.0-preview.1`; use numeric `0.2.0` where ArcGIS DAML requires a numeric version. MCP server info uses the full preview string.

- [ ] **Step 5: Implement Build-Preview.ps1**

The script accepts only optional `-ArcGISProInstallDir` and `-SkipTests`. It must:

```powershell
$arcgis = & (Join-Path $PSScriptRoot 'Resolve-ArcGISProInstall.ps1') -Candidate $ArcGISProInstallDir
if (-not $SkipTests) {
    & (Join-Path $PSScriptRoot 'Test-Foundation.ps1') -ArcGISProInstallDir $arcgis
    if ($LASTEXITCODE -ne 0) { throw 'Foundation verification failed.' }
}

dotnet publish $McpProject -c Release -r win-x64 --self-contained true `
    -p:PublishSingleFile=true -p:PublishTrimmed=false -p:Version=0.2.0-preview.1 `
    -o $McpPublish
if ($LASTEXITCODE -ne 0) { throw 'MCP publish failed.' }

dotnet build $AddInProject -c Release -p:ArcGISProInstallDir=$arcgis `
    -p:Version=0.2.0-preview.1
if ($LASTEXITCODE -ne 0) { throw 'Add-In build failed.' }
```

Require exactly one MCP executable and one `.esriAddInX`. Inspect the Add-In as a zip and fail if any entry matches `(^|/)ArcGIS\..*\.dll$`. Copy only the target-triple MCP and fixed Add-In package into `generated/preview`.

Run `npm ci`, then:

```powershell
npm run tauri -- build --config src-tauri/tauri.preview.conf.json `
    --bundles nsis --target x86_64-pc-windows-msvc
```

Copy exactly one produced installer to `artifacts/preview/ArcGISPro智能助手-0.2.0-preview.1-x64-setup.exe` and write `ArcGISPro智能助手-0.2.0-preview.1-x64-setup.exe.sha256` as lowercase hash, two spaces and filename. The script must not resolve, verify, download or stage Codex.

- [ ] **Step 6: Add shortcuts, project opener and third-party notice**

`hooks.nsh` uses `NSIS_HOOK_POSTINSTALL` to create one desktop shortcut and `NSIS_HOOK_PREUNINSTALL` to run exact cleanup and delete that shortcut. Do not enumerate or remove any AddIns directory.

`Open-Project.ps1` resolves `McpServer.sln` from `$PSScriptRoot` and opens it with `devenv.exe` when available, otherwise through the Windows file association. It contains no repository or ArcGIS absolute path.

`THIRD_PARTY_NOTICES.md` states that Codex CLI is an external prerequisite and is not redistributed. Copy `LICENSE` verbatim from nicogis upstream commit `e3383804d1682ed56b8e9dffda3e639064fb5230` into `licenses/nicogis-mcp-arcgis-pro-addin/LICENSE`; verify its Git blob hash is `f853e36a29acb2a37c05abee5886d39645f44433`, and record the source URL/revision in the notice. State explicitly that no Esri ArcGIS runtime assembly is redistributed.

- [ ] **Step 7: Validate sources, build the installer and commit**

Run:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\Test-PreviewPackaging.ps1
powershell -ExecutionPolicy Bypass -File scripts\Build-Preview.ps1
Get-FileHash -LiteralPath 'artifacts\preview\ArcGISPro智能助手-0.2.0-preview.1-x64-setup.exe' -Algorithm SHA256
git status --short
```

Expected: packaging assertions pass; installer hash matches the recorded file; staged binaries and installer artifacts do not appear in `git status`.

Commit only source/configuration:

```powershell
git add .gitignore Open-Project.ps1 THIRD_PARTY_NOTICES.md licenses/nicogis-mcp-arcgis-pro-addin/LICENSE scripts/Build-Preview.ps1 scripts/Test-PreviewPackaging.ps1 apps/desktop/package.json apps/desktop/package-lock.json apps/desktop/src-tauri src/ArcGISProAgent.AddIn src/ArcGISProAgent.Mcp
git commit -m "build: package ChatGPT-only Windows preview"
```

---

### Task 9: Finish user guidance and real-machine acceptance

**Files:**
- Create: `docs/development/preview-user-guide.md`
- Create: `docs/development/preview-smoke.md`
- Create: `scripts/Test-Preview.ps1`
- Modify: `README.md`
- Modify: `docs/development/foundation.md`

**Interfaces:**
- Consumes: the installer, checksum, external Codex detector, ChatGPT login and ArcGIS launch flow.
- Produces: a three-step quick start, one automated verification command and a dated PASS/FAIL acceptance record.

- [ ] **Step 1: Write the automated verification entrypoint first**

`Test-Preview.ps1` accepts optional `-ArcGISProInstallDir`, resolves it through `Resolve-ArcGISProInstall.ps1` and runs these commands with explicit exit checks:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\Test-Foundation.ps1 -ArcGISProInstallDir $arcgis
dotnet test McpServer.sln --no-restore
Push-Location apps\desktop
npm test
npm run build
Push-Location src-tauri
cargo test
cargo check --release
Pop-Location
Pop-Location
powershell -ExecutionPolicy Bypass -File scripts\Test-PreviewPackaging.ps1
git diff --check
```

Run it before the script exists and record the expected “file not found” RED. Then implement the exact command sequence and require all exits to be zero.

- [ ] **Step 2: Write the minimum user guide**

The guide starts with these three numbered actions:

1. install the official Codex CLI if the first screen says it is missing;
2. run `ArcGISPro智能助手-0.2.0-preview.1-x64-setup.exe` and choose “立即启动”;
3. complete the in-app ChatGPT login, ArcGIS/Add-In checks, then click “启动 ArcGIS Pro 并连接”.

Document the desktop/start-menu shortcuts, version-warning behavior, “重新检测”, manual `ArcGISPro.exe` selection, Add-In restart message, reconnection states, Windows uninstall and Esri Add-In Manager guidance. State that every user signs in with their own ChatGPT plan and that the software neither requests an OpenAI API key nor bundles Codex.

Add one developer paragraph linking `Open-Project.ps1` and `McpServer.sln`. README links the new guide and clearly labels DeepSeek as deferred.

- [ ] **Step 3: Run the full automated verification and rebuild**

Run:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\Test-Preview.ps1
powershell -ExecutionPolicy Bypass -File scripts\Build-Preview.ps1 -SkipTests
```

Expected: all automated suites pass and the installer/checksum are reproduced with the same source inputs. If Windows Credential Manager tests fail only under the sandbox identity, rerun `cargo test` in the interactive Administrator logon and record both outputs without weakening tests.

- [ ] **Step 4: Perform the real-machine smoke path**

Install the generated preview through Windows. Record only PASS/FAIL, date and product versions for:

1. installer offers immediate launch and both shortcuts work;
2. missing-Codex guidance is verified in an isolated test environment or controlled PATH fixture without uninstalling the user's Codex;
3. installed `codex-cli 0.149.0` is found, and a controlled nonmatching compatible version fixture shows the warning;
4. the application-private `CODEX_HOME` starts signed out, then official ChatGPT browser login completes;
5. ArcGIS Pro 3.7 is automatically found; manual selection rejects the wrong executable;
6. the fixed Add-In opens through Esri's handler and restart guidance is correct;
7. “启动 ArcGIS Pro 并连接” reaches live Bridge/MCP status;
8. tool inventory is exactly 17;
9. health check, project/context read and layer listing succeed without saving or editing GIS data;
10. restarting ArcGIS Pro recovers from “正在重新连接”;
11. Windows uninstall removes this app and its shortcuts but leaves external Codex, ArcGIS Pro, `.aprx` files, geodatabases and unrelated Add-Ins unchanged.

Do not record email, token, project path, project name, layer names or tool payloads in `preview-smoke.md`.

- [ ] **Step 5: Commit guides and acceptance evidence**

```powershell
git add README.md docs/development/preview-user-guide.md docs/development/preview-smoke.md docs/development/foundation.md scripts/Test-Preview.ps1
git commit -m "docs: finish ChatGPT-only preview acceptance"
```

---

## Final Review Gate

1. Run `superpowers:verification-before-completion` and retain fresh command output.
2. Generate a review package for `ad55341..HEAD` and request one final branch-level code/spec review.
3. Verify `git status --short` is clean.
4. Verify no generated installer, sidecar, Codex binary, credential, auth state, email, absolute user path or live GIS data is tracked.
5. Verify the final reviewer reports no Critical or Important findings; fix and re-review every such finding.
6. Present the local installer path, SHA-256 path, user guide, smoke record and exact unmerged branch to the user.
7. Do not merge, push, publish or create a GitHub Release until the user explicitly authorizes that separate action.

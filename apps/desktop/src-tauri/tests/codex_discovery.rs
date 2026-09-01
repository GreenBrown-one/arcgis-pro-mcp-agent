use std::{
    collections::HashMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    time::Duration,
};

use arcgis_pro_agent_desktop_lib::{
    codex::{
        CodexDiscoveryError, CodexVersionConfidence, CodexVersionProbe, ProcessCodexVersionProbe,
        discover_codex_with,
    },
    providers::BoxFuture,
};

struct RecordingProbe {
    replies: HashMap<PathBuf, Result<String, CodexDiscoveryError>>,
    calls: Mutex<Vec<PathBuf>>,
}

impl RecordingProbe {
    fn from<const N: usize>(replies: [(PathBuf, Result<&str, CodexDiscoveryError>); N]) -> Self {
        Self {
            replies: replies
                .into_iter()
                .map(|(path, reply)| {
                    (
                        fs::canonicalize(path).expect("canonical recording probe path"),
                        reply.map(str::to_owned),
                    )
                })
                .collect(),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn one(command: PathBuf, reply: Result<&str, CodexDiscoveryError>) -> Self {
        Self::from([(command, reply)])
    }

    fn calls(&self) -> Vec<PathBuf> {
        self.calls.lock().expect("recording probe mutex").clone()
    }
}

impl Default for RecordingProbe {
    fn default() -> Self {
        Self {
            replies: HashMap::new(),
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl CodexVersionProbe for RecordingProbe {
    fn probe<'a>(
        &'a self,
        command: &'a Path,
    ) -> BoxFuture<'a, Result<String, CodexDiscoveryError>> {
        Box::pin(async move {
            let command = fs::canonicalize(command).expect("canonical probe command");
            self.calls
                .lock()
                .expect("recording probe mutex")
                .push(command.clone());
            self.replies
                .get(&command)
                .cloned()
                .unwrap_or(Err(CodexDiscoveryError::Invalid))
        })
    }
}

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

    assert_eq!(found.command, fs::canonicalize(path_command).unwrap());
    assert_eq!(found.version, "0.149.0");
    assert_eq!(found.confidence, CodexVersionConfidence::Tested);
    assert_eq!(probe.calls().len(), 1);
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

#[tokio::test]
async fn real_version_output_is_discovered_without_reparsing() {
    let fixture = CodexFixture::new();
    let command = fixture.file_text("path/codex.cmd", "@echo off\r\necho codex-cli 0.149.0\r\n");
    let path = std::env::join_paths([command.parent().unwrap()]).unwrap();

    let probe = ProcessCodexVersionProbe::new(fixture.path("private/ArcGISProAgent/codex"));
    let found = discover_codex_with(Some(&path), Some(&fixture.app_data()), &probe)
        .await
        .expect("real version output is discovered");

    assert_eq!(found.version, "0.149.0");
    assert_eq!(found.confidence, CodexVersionConfidence::Tested);
}

#[tokio::test]
async fn process_probe_overrides_parent_codex_home_with_its_private_home() {
    let _environment = CODEX_HOME_LOCK.lock().expect("CODEX_HOME test mutex");
    let fixture = CodexFixture::new();
    let captured_home = fixture.path("helper/codex-home.txt");
    let command = fixture.file_text(
        "helper/version-probe.cmd",
        &format!(
            "@echo off\r\npowershell.exe -NoProfile -NonInteractive -Command \"$env:CODEX_HOME | Set-Content -LiteralPath '{}' ; [Console]::Out.Write('codex-cli 0.149.0')\"\r\n",
            powershell_single_quoted_path(&captured_home),
        ),
    );
    let private_home = fixture.path("private/ArcGISProAgent/codex");
    let _parent_home = EnvironmentOverride::set("CODEX_HOME", "parent-global-codex-home");
    let probe = ProcessCodexVersionProbe::new(private_home.clone());

    let output = probe
        .probe(&command)
        .await
        .expect("valid helper version output");

    assert_eq!(output, "codex-cli 0.149.0");
    assert_eq!(
        fs::read_to_string(captured_home)
            .expect("captured child CODEX_HOME")
            .trim(),
        private_home.to_string_lossy(),
    );
}

#[tokio::test]
async fn process_probe_rejects_exit_zero_stdout_outside_the_version_grammar() {
    let fixture = CodexFixture::new();
    let command = fixture.file_text(
        "helper/version-probe.cmd",
        "@echo off\r\necho not-a-codex-version\r\n",
    );
    let probe = ProcessCodexVersionProbe::new(fixture.path("private/ArcGISProAgent/codex"));

    let error = probe
        .probe(&command)
        .await
        .expect_err("exit-zero output outside the version grammar is invalid");

    assert_eq!(error, CodexDiscoveryError::Invalid);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn oversized_helper_output_is_rejected_and_the_helper_exits() {
    let fixture = CodexFixture::new();
    let pid_path = fixture.path("helper/pid.txt");
    let command = fixture.file_text(
        "helper/version-probe.cmd",
        &format!(
            "@echo off\r\npowershell.exe -NoProfile -NonInteractive -Command \"$PID | Set-Content -LiteralPath '{}' ; [Console]::Out.Write(('x' * 4097))\"\r\n",
            powershell_single_quoted_path(&pid_path),
        ),
    );
    let started = std::time::Instant::now();

    let probe = ProcessCodexVersionProbe::new(fixture.path("private/ArcGISProAgent/codex"));
    let error = probe
        .probe(&command)
        .await
        .expect_err("oversized version output is invalid");

    assert_eq!(error, CodexDiscoveryError::Invalid);
    assert!(started.elapsed() < Duration::from_secs(6));
    let pid = wait_for_pid(&pid_path);
    assert!(!process_is_running(pid), "helper process must be reaped");
}

#[test]
fn production_discovery_rejects_untrusted_search_and_override_paths() {
    let source = include_str!("../src/codex/discovery.rs");

    assert!(!source.contains("WindowsApps"));
    assert!(!source.contains("read_dir"));
    assert!(!source.contains("ARCGIS_AGENT_CODEX_COMMAND"));
    assert!(!source.contains("Command::new(\"codex\")"));
}

fn powershell_single_quoted_path(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

fn wait_for_pid(path: &Path) -> u32 {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        if let Ok(value) = fs::read_to_string(path) {
            return value.trim().parse().expect("fake helper PID is numeric");
        }
        assert!(
            std::time::Instant::now() < deadline,
            "fake helper did not record a PID"
        );
        std::thread::yield_now();
    }
}

fn process_is_running(pid: u32) -> bool {
    let output = Command::new("tasklist.exe")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .expect("query fake helper process");
    String::from_utf8_lossy(&output.stdout).contains(&pid.to_string())
}

static CODEX_HOME_LOCK: Mutex<()> = Mutex::new(());

struct EnvironmentOverride {
    name: &'static str,
    previous: Option<OsString>,
}

impl EnvironmentOverride {
    fn set(name: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(name);
        unsafe { std::env::set_var(name, value) };
        Self { name, previous }
    }
}

impl Drop for EnvironmentOverride {
    fn drop(&mut self) {
        if let Some(value) = &self.previous {
            unsafe { std::env::set_var(self.name, value) };
        } else {
            unsafe { std::env::remove_var(self.name) };
        }
    }
}

struct CodexFixture {
    root: PathBuf,
}

impl CodexFixture {
    fn new() -> Self {
        Self {
            root: tempfile::tempdir().expect("fixture directory").keep(),
        }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    fn file(&self, relative: &str) -> PathBuf {
        self.file_text(relative, "")
    }

    fn file_text(&self, relative: &str, value: &str) -> PathBuf {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().expect("fixture parent")).expect("create fixture parent");
        fs::write(&path, value).expect("write fixture file");
        path
    }

    fn directory(&self, relative: &str) {
        fs::create_dir_all(self.path(relative)).expect("create fixture directory");
    }

    fn app_data(&self) -> PathBuf {
        let path = self.path("appdata");
        fs::create_dir_all(&path).expect("create app data directory");
        path
    }
}

impl Drop for CodexFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

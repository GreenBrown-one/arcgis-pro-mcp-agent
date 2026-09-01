use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use arcgis_pro_agent_desktop_lib::{
    app_state::{AccountSnapshot, CodexSnapshot, DesktopSnapshot},
    codex::{CodexStartOptions, build_codex_command},
    paths::resolve_codex_command,
    providers::{
        ProviderAuthSnapshot, ProviderKind, ProviderRuntimeSnapshot,
        codex::snapshot as codex_provider_snapshot,
    },
    runtime_secret::create_runtime_file,
};
use serde_json::json;

#[test]
fn default_snapshot_is_provider_neutral_and_codex_selected() {
    let snapshot = DesktopSnapshot::default();
    assert_eq!(snapshot.provider.kind, ProviderKind::Codex);
    assert_eq!(snapshot.provider.auth, ProviderAuthSnapshot::Checking);
    assert_eq!(snapshot.provider.runtime, ProviderRuntimeSnapshot::Starting);
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

#[test]
fn codex_command_prefers_override_then_bundled_executable_then_development_fallback() {
    let executable = Path::new(r"C:\Program Files\ArcGISProAgent\desktop.exe");
    let bundled = PathBuf::from(r"C:\Program Files\ArcGISProAgent\codex.exe");

    assert_eq!(
        resolve_codex_command(
            Some(OsStr::new(r"E:\custom\codex.exe")),
            Some(executable),
            |_| true,
        ),
        PathBuf::from(r"E:\custom\codex.exe")
    );
    assert_eq!(
        resolve_codex_command(None, Some(executable), |path| path == bundled),
        bundled
    );
    assert_eq!(
        resolve_codex_command(None, Some(executable), |_| false),
        PathBuf::from("codex.cmd")
    );
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
    assert_eq!(
        command
            .get_envs()
            .find(|(key, _)| *key == "CODEX_HOME")
            .unwrap()
            .1,
        Some(options.codex_home.as_os_str())
    );
}

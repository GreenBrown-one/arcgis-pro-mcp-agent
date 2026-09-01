use std::{
    collections::HashSet,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use crate::providers::BoxFuture;

pub const TESTED_CODEX_VERSION: &str = "0.149.0";
pub const CODEX_INSTALL_URL: &str = "https://learn.chatgpt.com/docs/codex/cli";

const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

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

pub struct ProcessCodexVersionProbe {
    private_home: PathBuf,
}

impl ProcessCodexVersionProbe {
    pub fn new(private_home: PathBuf) -> Self {
        Self { private_home }
    }
}

impl CodexVersionProbe for ProcessCodexVersionProbe {
    fn probe<'a>(
        &'a self,
        command: &'a Path,
    ) -> BoxFuture<'a, Result<String, CodexDiscoveryError>> {
        Box::pin(async move {
            let mut process = tokio::process::Command::new(command);
            process
                .arg("--version")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .env("CODEX_HOME", &self.private_home)
                .env_remove("OPENAI_API_KEY")
                .env_remove("AZURE_OPENAI_API_KEY")
                .env_remove("CODEX_API_KEY")
                .kill_on_drop(true);
            let mut child = process.spawn().map_err(|_| CodexDiscoveryError::Invalid)?;
            let stdout = child.stdout.take().ok_or(CodexDiscoveryError::Invalid)?;
            let stderr = child.stderr.take().ok_or(CodexDiscoveryError::Invalid)?;
            let result = {
                let stdout = read_limited(stdout);
                let stderr = read_limited(stderr);
                tokio::time::timeout(VERSION_TIMEOUT, async {
                    let (stdout, _) = tokio::try_join!(stdout, stderr)
                        .map_err(|_| CodexDiscoveryError::Invalid)?;
                    let status = child
                        .wait()
                        .await
                        .map_err(|_| CodexDiscoveryError::Invalid)?;
                    Ok::<_, CodexDiscoveryError>((status, stdout))
                })
                .await
            };

            match result {
                Ok(Ok((status, stdout))) if status.success() => {
                    let output =
                        String::from_utf8(stdout).map_err(|_| CodexDiscoveryError::Invalid)?;
                    valid_version_output(&output)
                        .then_some(output)
                        .ok_or(CodexDiscoveryError::Invalid)
                }
                _ => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    Err(CodexDiscoveryError::Invalid)
                }
            }
        })
    }
}

pub fn codex_candidates(path: Option<&OsStr>, roaming_app_data: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();

    if let Some(path) = path {
        for directory in std::env::split_paths(path) {
            for filename in ["codex.exe", "codex.cmd"] {
                push_candidate(&mut candidates, &mut seen, directory.join(filename));
            }
        }
    }
    if let Some(roaming_app_data) = roaming_app_data {
        push_candidate(
            &mut candidates,
            &mut seen,
            roaming_app_data.join("npm").join("codex.cmd"),
        );
    }

    candidates
}

pub async fn discover_codex_with(
    path: Option<&OsStr>,
    roaming_app_data: Option<&Path>,
    probe: &dyn CodexVersionProbe,
) -> Result<CodexInstallation, CodexDiscoveryError> {
    for command in codex_candidates(path, roaming_app_data) {
        let Ok(output) = probe.probe(&command).await else {
            continue;
        };
        let Some(version) = parse_version(&output) else {
            continue;
        };
        return Ok(CodexInstallation {
            command,
            confidence: if version == TESTED_CODEX_VERSION {
                CodexVersionConfidence::Tested
            } else {
                CodexVersionConfidence::Unverified
            },
            version,
        });
    }
    Err(CodexDiscoveryError::NotFound)
}

pub async fn discover_codex() -> Result<CodexInstallation, CodexDiscoveryError> {
    let path = std::env::var_os("PATH");
    let app_data = std::env::var_os("APPDATA").map(PathBuf::from);
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or(CodexDiscoveryError::NotFound)?;
    let probe = ProcessCodexVersionProbe::new(local_app_data.join("ArcGISProAgent").join("codex"));
    discover_codex_with(path.as_deref(), app_data.as_deref(), &probe).await
}

fn push_candidate(candidates: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, path: PathBuf) {
    let Ok(path) = std::fs::canonicalize(path) else {
        return;
    };
    let Ok(metadata) = std::fs::metadata(&path) else {
        return;
    };
    let Some(filename) = path.file_name().and_then(OsStr::to_str) else {
        return;
    };
    if metadata.is_file()
        && matches!(
            filename.to_ascii_lowercase().as_str(),
            "codex.exe" | "codex.cmd"
        )
        && seen.insert(path.clone())
    {
        candidates.push(path);
    }
}

fn parse_version(value: &str) -> Option<String> {
    valid_version_output(value).then(|| {
        value
            .trim()
            .strip_prefix("codex-cli ")
            .expect("valid version output has the required prefix")
            .to_owned()
    })
}

fn valid_version_output(value: &str) -> bool {
    let Some(version) = value.trim().strip_prefix("codex-cli ") else {
        return false;
    };
    let mut parts = version.split('.');
    let valid = parts.clone().count() == 3
        && parts.all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    valid
}

async fn read_limited(reader: impl tokio::io::AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut bytes = Vec::new();
    reader.take(4097).read_to_end(&mut bytes).await?;
    if bytes.len() > 4096 {
        return Err(std::io::Error::other("version output too large"));
    }
    Ok(bytes)
}

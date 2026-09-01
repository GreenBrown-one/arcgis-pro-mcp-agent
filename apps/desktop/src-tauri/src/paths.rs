use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

pub const APPLICATION_DIRECTORY: &str = "ArcGISProAgent";
pub const RUNTIME_DIRECTORY: &str = "runtime";
pub const RUNTIME_FILE_NAME: &str = "bridge.json";
pub const MCP_EXECUTABLE_NAME: &str = "ArcGISProAgent.Mcp.exe";
pub const CODEX_DIRECTORY: &str = "codex";
pub const CODEX_EXECUTABLE_NAME: &str = "codex.exe";

pub fn resolve_codex_command(
    explicit: Option<&OsStr>,
    current_exe: Option<&Path>,
    is_file: impl Fn(&Path) -> bool,
) -> PathBuf {
    if let Some(explicit) = explicit.filter(|value| !value.is_empty()) {
        return PathBuf::from(explicit);
    }
    current_exe
        .and_then(Path::parent)
        .map(|parent| parent.join(CODEX_EXECUTABLE_NAME))
        .filter(|path| is_file(path))
        .unwrap_or_else(|| PathBuf::from("codex.cmd"))
}

#[cfg(debug_assertions)]
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
        .filter(|parent| {
            parent
                .file_name()
                .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("desktop"))
        })
        .and_then(Path::parent)
        .map(|version| version.join("mcp").join(MCP_EXECUTABLE_NAME));
    candidate
        .filter(|path| is_file(path))
        .unwrap_or_else(|| PathBuf::from(MCP_EXECUTABLE_NAME))
}

pub fn runtime_directory(local_app_data: &Path) -> PathBuf {
    local_app_data
        .join(APPLICATION_DIRECTORY)
        .join(RUNTIME_DIRECTORY)
}

#[cfg(not(debug_assertions))]
pub(crate) fn resolve_release_mcp_command(current_exe: Option<&Path>) -> Result<PathBuf, ()> {
    let install_directory = current_exe.and_then(Path::parent).ok_or(())?;
    let candidate = install_directory.join(MCP_EXECUTABLE_NAME);
    let metadata = std::fs::symlink_metadata(&candidate).map_err(|_| ())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        return Err(());
    }
    let canonical_root = std::fs::canonicalize(install_directory).map_err(|_| ())?;
    let expected = canonical_root.join(MCP_EXECUTABLE_NAME);
    let canonical_candidate = std::fs::canonicalize(&candidate).map_err(|_| ())?;
    paths_match(&canonical_candidate, &expected)
        .then_some(canonical_candidate)
        .ok_or(())
}

#[cfg(all(not(debug_assertions), windows))]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(all(not(debug_assertions), not(windows)))]
fn is_reparse_point(_metadata: &std::fs::Metadata) -> bool {
    false
}

#[cfg(all(not(debug_assertions), windows))]
fn paths_match(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

#[cfg(all(not(debug_assertions), not(windows)))]
fn paths_match(left: &Path, right: &Path) -> bool {
    left == right
}

pub fn codex_home(local_app_data: &Path) -> PathBuf {
    local_app_data
        .join(APPLICATION_DIRECTORY)
        .join(CODEX_DIRECTORY)
}

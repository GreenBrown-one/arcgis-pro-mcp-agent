use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

pub const ADDIN_PACKAGE_NAME: &str = "ArcGISProAgent.AddIn.esriAddInX";
pub const ADDIN_UNINSTALL_GUIDANCE: &str =
    "Open ArcGIS Pro, then go to Project/Settings > Add-In Manager and choose Delete this Add-In.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddInInstallerOpenResult {
    pub package_name: String,
    pub requires_restart: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArcGisProcessState {
    Running,
    NotRunning,
    Unknown,
}

impl ArcGisProcessState {
    pub fn requires_restart_for_change(self, changed: bool) -> bool {
        changed && !matches!(self, Self::NotRunning)
    }
}

pub fn arcgis_process_state_with<F, E>(enumerate: F) -> ArcGisProcessState
where
    F: FnOnce() -> Result<bool, E>,
{
    match enumerate() {
        Ok(true) => ArcGisProcessState::Running,
        Ok(false) => ArcGisProcessState::NotRunning,
        Err(_) => ArcGisProcessState::Unknown,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddInInstallError {
    Unavailable,
}

impl fmt::Display for AddInInstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Bundled ArcGIS Pro Add-In is unavailable")
    }
}

impl Error for AddInInstallError {}

pub fn packaged_addin_path(resource_dir: &Path) -> Result<PathBuf, AddInInstallError> {
    let trusted_root = trusted_resource_root(resource_dir)?;
    for relative_parent in [PathBuf::new(), PathBuf::from("generated").join("preview")] {
        let lexical_parent = resource_dir.join(&relative_parent);
        let expected_parent = trusted_root.join(&relative_parent);
        let canonical_parent = match fs::canonicalize(&lexical_parent) {
            Ok(parent) => parent,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => return Err(AddInInstallError::Unavailable),
        };
        if canonical_parent != expected_parent || !canonical_parent.starts_with(&trusted_root) {
            continue;
        }
        let candidate = lexical_parent.join(ADDIN_PACKAGE_NAME);
        let metadata = match fs::symlink_metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => return Err(AddInInstallError::Unavailable),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            continue;
        }
        let canonical_candidate =
            fs::canonicalize(&candidate).map_err(|_| AddInInstallError::Unavailable)?;
        if canonical_candidate.parent() == Some(canonical_parent.as_path())
            && canonical_candidate.starts_with(&trusted_root)
            && fs::metadata(&canonical_candidate)
                .map_err(|_| AddInInstallError::Unavailable)?
                .is_file()
        {
            return Ok(canonical_candidate);
        }
    }
    Err(AddInInstallError::Unavailable)
}

fn trusted_resource_root(resource_dir: &Path) -> Result<PathBuf, AddInInstallError> {
    let metadata =
        fs::symlink_metadata(resource_dir).map_err(|_| AddInInstallError::Unavailable)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(AddInInstallError::Unavailable);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
            return Err(AddInInstallError::Unavailable);
        }
    }
    fs::canonicalize(resource_dir).map_err(|_| AddInInstallError::Unavailable)
}

pub fn open_packaged_addin_with<F>(
    resource_dir: &Path,
    process_state: ArcGisProcessState,
    open_registered_association: F,
) -> Result<AddInInstallerOpenResult, AddInInstallError>
where
    F: FnOnce(&Path) -> Result<(), AddInInstallError>,
{
    let package = packaged_addin_path(resource_dir)?;
    open_registered_association(&package)?;
    Ok(AddInInstallerOpenResult {
        package_name: ADDIN_PACKAGE_NAME.to_owned(),
        requires_restart: process_state.requires_restart_for_change(true),
    })
}

pub fn uninstall_guidance() -> &'static str {
    ADDIN_UNINSTALL_GUIDANCE
}

#[cfg(windows)]
pub fn arcgis_pro_process_state() -> ArcGisProcessState {
    arcgis_process_state_with(enumerate_arcgis_processes)
}

#[cfg(windows)]
fn enumerate_arcgis_processes() -> Result<bool, ()> {
    use windows::Win32::{
        Foundation::{CloseHandle, ERROR_NO_MORE_FILES, GetLastError},
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
    };

    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return Err(());
    };
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let result = match unsafe { Process32FirstW(snapshot, &mut entry) } {
        Ok(()) => loop {
            let end = entry
                .szExeFile
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..end])
                .eq_ignore_ascii_case("ArcGISPro.exe")
            {
                break Ok(true);
            }
            if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
                break if unsafe { GetLastError() } == ERROR_NO_MORE_FILES {
                    Ok(false)
                } else {
                    Err(())
                };
            }
        },
        Err(_) if unsafe { GetLastError() } == ERROR_NO_MORE_FILES => Ok(false),
        Err(_) => Err(()),
    };
    let _ = unsafe { CloseHandle(snapshot) };
    result
}

#[cfg(not(windows))]
pub fn arcgis_pro_process_state() -> ArcGisProcessState {
    ArcGisProcessState::Unknown
}

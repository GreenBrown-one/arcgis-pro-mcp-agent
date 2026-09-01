use std::{
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArcGisInstallSource {
    Saved,
    Registry,
    Standard,
    Compatible,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArcGisInstallation {
    pub root: PathBuf,
    pub executable: PathBuf,
    pub version: Option<String>,
    pub source: ArcGisInstallSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ArcGisInstallSnapshot {
    Checking,
    Ready { installation: ArcGisInstallation },
    NotFound,
    Error { code: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArcGisInstallError {
    InvalidInstallation,
    UnsupportedVersion,
    NotFound,
}

impl fmt::Display for ArcGisInstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidInstallation => "ArcGIS Pro installation is invalid",
            Self::UnsupportedVersion => "ArcGIS Pro version is not supported",
            Self::NotFound => "ArcGIS Pro 3.7 was not found",
        })
    }
}

impl Error for ArcGisInstallError {}

pub fn validate_installation(root: &Path) -> Result<ArcGisInstallation, ArcGisInstallError> {
    validate_installation_with(root, file_version)
}

pub fn validate_installation_with<F>(
    root: &Path,
    read_version: F,
) -> Result<ArcGisInstallation, ArcGisInstallError>
where
    F: FnOnce(&Path) -> Option<String>,
{
    let root = fs::canonicalize(root).map_err(|_| ArcGisInstallError::InvalidInstallation)?;
    let executable = root.join("bin").join("ArcGISPro.exe");
    let core = root.join("bin").join("ArcGIS.Core.dll");
    if !executable.is_file() || !core.is_file() {
        return Err(ArcGisInstallError::InvalidInstallation);
    }
    let version = read_version(&executable).filter(|version| is_supported_version(version));
    if version.is_none() {
        return Err(ArcGisInstallError::UnsupportedVersion);
    }
    Ok(ArcGisInstallation {
        root,
        executable,
        version,
        source: ArcGisInstallSource::Compatible,
    })
}

pub fn choose_arcgis_executable(path: &Path) -> Result<ArcGisInstallation, ArcGisInstallError> {
    choose_arcgis_executable_with(path, file_version)
}

pub fn choose_arcgis_executable_with<F>(
    path: &Path,
    read_version: F,
) -> Result<ArcGisInstallation, ArcGisInstallError>
where
    F: FnOnce(&Path) -> Option<String>,
{
    let is_arcgis_executable = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("ArcGISPro.exe"));
    let bin = path
        .parent()
        .ok_or(ArcGisInstallError::InvalidInstallation)?;
    let is_bin = bin
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("bin"));
    if !is_arcgis_executable || !is_bin {
        return Err(ArcGisInstallError::InvalidInstallation);
    }
    let root = bin
        .parent()
        .ok_or(ArcGisInstallError::InvalidInstallation)?;
    let mut installation = validate_installation_with(root, read_version)?;
    installation.source = ArcGisInstallSource::Manual;
    Ok(installation)
}

pub fn discover_arcgis(
    saved_root: Option<&Path>,
) -> Result<ArcGisInstallation, ArcGisInstallError> {
    let mut candidates = Vec::with_capacity(4);
    if let Some(root) = saved_root {
        candidates.push((ArcGisInstallSource::Saved, root.to_path_buf()));
    }
    if let Some(root) = registry_install_dir() {
        candidates.push((ArcGisInstallSource::Registry, root));
    }
    candidates.push((
        ArcGisInstallSource::Standard,
        PathBuf::from(r"C:\Program Files\ArcGIS\Pro"),
    ));
    candidates.push((
        ArcGisInstallSource::Compatible,
        PathBuf::from(r"D:\arcgis_pro"),
    ));
    select_sourced_installation(candidates, validate_installation)
}

pub fn select_sourced_installation<I, F>(
    candidates: I,
    mut validate: F,
) -> Result<ArcGisInstallation, ArcGisInstallError>
where
    I: IntoIterator<Item = (ArcGisInstallSource, PathBuf)>,
    F: FnMut(&Path) -> Result<ArcGisInstallation, ArcGisInstallError>,
{
    for (source, root) in candidates {
        if let Ok(mut installation) = validate(&root) {
            installation.source = source;
            return Ok(installation);
        }
    }
    Err(ArcGisInstallError::NotFound)
}

pub fn is_supported_version(version: &str) -> bool {
    let mut components = version.split('.');
    matches!(components.next(), Some("3")) && matches!(components.next(), Some("7"))
}

#[cfg(windows)]
fn file_version(path: &Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;

    use windows::{
        Win32::Storage::FileSystem::{
            GetFileVersionInfoSizeW, GetFileVersionInfoW, VS_FIXEDFILEINFO, VerQueryValueW,
        },
        core::PCWSTR,
    };

    let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let size = unsafe { GetFileVersionInfoSizeW(PCWSTR(wide_path.as_ptr()), None) };
    if size == 0 {
        return None;
    }
    let mut buffer = vec![0_u8; size as usize];
    unsafe {
        GetFileVersionInfoW(
            PCWSTR(wide_path.as_ptr()),
            None,
            size,
            buffer.as_mut_ptr().cast(),
        )
    }
    .ok()?;
    let root_query = [b'\\' as u16, 0];
    let mut fixed_info = std::ptr::null_mut();
    let mut fixed_info_len = 0_u32;
    if !unsafe {
        VerQueryValueW(
            buffer.as_ptr().cast(),
            PCWSTR(root_query.as_ptr()),
            &mut fixed_info,
            &mut fixed_info_len,
        )
    }
    .as_bool()
        || fixed_info.is_null()
        || fixed_info_len < size_of::<VS_FIXEDFILEINFO>() as u32
    {
        return None;
    }
    let version = unsafe { &*fixed_info.cast::<VS_FIXEDFILEINFO>() };
    if version.dwSignature != 0xFEEF_04BD {
        return None;
    }
    let major = version.dwProductVersionMS >> 16;
    let minor = version.dwProductVersionMS & 0xffff;
    let build = version.dwProductVersionLS >> 16;
    let revision = version.dwProductVersionLS & 0xffff;
    Some(format!("{major}.{minor}.{build}.{revision}"))
}

#[cfg(not(windows))]
fn file_version(_path: &Path) -> Option<String> {
    None
}

#[cfg(windows)]
fn registry_install_dir() -> Option<PathBuf> {
    use windows::{
        Win32::System::Registry::{
            HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6464KEY, RegGetValueW,
        },
        core::w,
    };

    let flags = RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY;
    let mut size = 0_u32;
    let first = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"SOFTWARE\ESRI\ArcGISPro"),
            w!("InstallDir"),
            flags,
            None,
            None,
            Some(&mut size),
        )
    };
    if first.0 != 0 || size < 2 {
        return None;
    }
    let mut buffer = vec![0_u16; (size as usize).div_ceil(size_of::<u16>())];
    let second = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"SOFTWARE\ESRI\ArcGISPro"),
            w!("InstallDir"),
            flags,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if second.0 != 0 {
        return None;
    }
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    let value = String::from_utf16(&buffer[..end]).ok()?;
    (!value.trim().is_empty()).then(|| PathBuf::from(value))
}

#[cfg(not(windows))]
fn registry_install_dir() -> Option<PathBuf> {
    None
}

pub fn choose_installation<I, F>(candidates: I, mut is_valid: F) -> Option<ArcGisInstallation>
where
    I: IntoIterator<Item = PathBuf>,
    F: FnMut(&Path) -> bool,
{
    candidates
        .into_iter()
        .enumerate()
        .find_map(|(index, root)| {
            is_valid(&root).then(|| ArcGisInstallation {
                executable: root.join("bin").join("ArcGISPro.exe"),
                root,
                version: None,
                source: match index {
                    0 => ArcGisInstallSource::Saved,
                    1 => ArcGisInstallSource::Registry,
                    2 => ArcGisInstallSource::Standard,
                    _ => ArcGisInstallSource::Compatible,
                },
            })
        })
}

pub fn arcgis_launch_command(installation: &ArcGisInstallation) -> Command {
    Command::new(&installation.executable)
}

pub fn launch_arcgis(installation: &ArcGisInstallation) -> Result<u32, ArcGisInstallError> {
    arcgis_launch_command(installation)
        .spawn()
        .map(|child| child.id())
        .map_err(|_| ArcGisInstallError::InvalidInstallation)
}

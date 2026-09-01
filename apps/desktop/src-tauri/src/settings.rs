use std::{
    error::Error,
    fmt,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::providers::ProviderKind;

pub const SETTINGS_SCHEMA_VERSION: u32 = 1;
static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppSettings {
    pub schema_version: u32,
    pub active_provider: ProviderKind,
    pub arcgis_pro_root: Option<PathBuf>,
    pub deepseek: DeepSeekSettings,
    pub onboarding_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeepSeekSettings {
    pub base_url: String,
    pub model: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            active_provider: ProviderKind::Codex,
            arcgis_pro_root: None,
            deepseek: DeepSeekSettings {
                base_url: "https://api.deepseek.com".to_owned(),
                model: "deepseek-v4-flash".to_owned(),
            },
            onboarding_complete: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsError {
    InvalidSettings,
    UnsupportedSchemaVersion,
    Unavailable,
}

impl fmt::Display for SettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidSettings => "settings are invalid",
            Self::UnsupportedSchemaVersion => "settings schema version is unsupported",
            Self::Unavailable => "settings storage is unavailable",
        })
    }
}

impl Error for SettingsError {}

#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(local_app_data: impl AsRef<Path>) -> Self {
        Self {
            path: local_app_data
                .as_ref()
                .join("ArcGISProAgent")
                .join("preview")
                .join("settings.json"),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<AppSettings, SettingsError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(AppSettings::default());
            }
            Err(_) => return Err(SettingsError::Unavailable),
        };
        let settings: AppSettings =
            serde_json::from_slice(&bytes).map_err(|_| SettingsError::InvalidSettings)?;
        if settings.schema_version != SETTINGS_SCHEMA_VERSION {
            return Err(SettingsError::UnsupportedSchemaVersion);
        }
        Ok(settings)
    }

    pub fn load_chatgpt_only(&self) -> Result<AppSettings, SettingsError> {
        let mut settings = self.load()?;
        if settings.active_provider != ProviderKind::Codex {
            settings.active_provider = ProviderKind::Codex;
            self.save(&settings)?;
        }
        Ok(settings)
    }

    pub fn save(&self, settings: &AppSettings) -> Result<(), SettingsError> {
        let parent = self.path.parent().ok_or(SettingsError::Unavailable)?;
        fs::create_dir_all(parent).map_err(|_| SettingsError::Unavailable)?;
        let bytes =
            serde_json::to_vec_pretty(settings).map_err(|_| SettingsError::InvalidSettings)?;
        let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temp_path = parent.join(format!(
            ".settings.json.{}.{sequence}.tmp",
            std::process::id()
        ));
        let write_result = (|| -> io::Result<()> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)?;
            file.write_all(&bytes)?;
            file.sync_all()
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temp_path);
            return Err(SettingsError::Unavailable);
        }
        if atomic_replace(&temp_path, &self.path).is_err() {
            let _ = fs::remove_file(&temp_path);
            return Err(SettingsError::Unavailable);
        }
        Ok(())
    }

    pub fn save_arcgis_pro_root(&self, root: PathBuf) -> Result<(), SettingsError> {
        let mut settings = self.load()?;
        settings.arcgis_pro_root = Some(root);
        self.save(&settings)
    }
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    use windows::{
        Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        },
        core::PCWSTR,
    };

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(io::Error::other)
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

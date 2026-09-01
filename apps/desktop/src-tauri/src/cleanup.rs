use std::{fs, path::Path};

use crate::{credential_store, paths::APPLICATION_DIRECTORY};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupError {
    UnsafeRoot,
    DataRemoval,
    CredentialRemoval,
}

pub fn cleanup_owned_data(local_app_data: &Path) -> Result<(), CleanupError> {
    if let Some(application_root) = validate_owned_data_root(local_app_data)? {
        fs::remove_dir_all(application_root).map_err(|_| CleanupError::DataRemoval)?;
    }
    Ok(())
}

pub fn cleanup_for_uninstall_with(
    local_app_data: &Path,
    delete_owned_credential: impl FnOnce() -> Result<(), credential_store::SecretError>,
) -> Result<(), CleanupError> {
    let application_root = validate_owned_data_root(local_app_data)?;
    delete_owned_credential().map_err(|_| CleanupError::CredentialRemoval)?;
    if let Some(application_root) = application_root {
        fs::remove_dir_all(application_root).map_err(|_| CleanupError::DataRemoval)?;
    }
    Ok(())
}

fn validate_owned_data_root(
    local_app_data: &Path,
) -> Result<Option<std::path::PathBuf>, CleanupError> {
    let application_root = local_app_data.join(APPLICATION_DIRECTORY);
    let metadata = match fs::symlink_metadata(&application_root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(CleanupError::DataRemoval),
    };
    if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        return Err(CleanupError::UnsafeRoot);
    }
    Ok(Some(application_root))
}

pub fn cleanup_for_uninstall() -> i32 {
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    match cleanup_for_uninstall_with(
        &local_app_data,
        credential_store::clear_owned_credential_for_uninstall,
    ) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

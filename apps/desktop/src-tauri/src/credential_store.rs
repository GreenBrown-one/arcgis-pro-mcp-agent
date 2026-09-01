use std::{error::Error, fmt};

use crate::providers::BoxFuture;

pub const DEEPSEEK_CREDENTIAL_TARGET: &str = "ArcGISProAgent.Preview.DeepSeek";
const MIN_DEEPSEEK_SECRET_BYTES: usize = 16;
const MAX_DEEPSEEK_SECRET_BYTES: usize = 512 * 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretError {
    InvalidSecret,
    InvalidStoredSecret,
    Unavailable,
}

impl fmt::Display for SecretError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidSecret => "invalid secret",
            Self::InvalidStoredSecret => "stored secret is invalid",
            Self::Unavailable => "credential storage is unavailable",
        })
    }
}

impl Error for SecretError {}

pub trait SecretStore: Send + Sync {
    fn get<'a>(&'a self, target: &'a str) -> BoxFuture<'a, Result<Option<String>, SecretError>>;
    fn set<'a>(
        &'a self,
        target: &'a str,
        secret: &'a str,
    ) -> BoxFuture<'a, Result<(), SecretError>>;
    fn delete<'a>(&'a self, target: &'a str) -> BoxFuture<'a, Result<(), SecretError>>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsCredentialStore;

pub async fn configure_deepseek(store: &dyn SecretStore, secret: &str) -> Result<(), SecretError> {
    validate_deepseek_secret(secret)?;
    store.set(DEEPSEEK_CREDENTIAL_TARGET, secret).await
}

pub async fn clear_deepseek(store: &dyn SecretStore) -> Result<(), SecretError> {
    store.delete(DEEPSEEK_CREDENTIAL_TARGET).await
}

#[cfg(windows)]
pub fn clear_owned_credential_for_uninstall() -> Result<(), SecretError> {
    windows_delete(DEEPSEEK_CREDENTIAL_TARGET)
}

#[cfg(not(windows))]
pub fn clear_owned_credential_for_uninstall() -> Result<(), SecretError> {
    Err(SecretError::Unavailable)
}

pub async fn deepseek_credential_is_configured(
    store: &dyn SecretStore,
) -> Result<bool, SecretError> {
    match store.get(DEEPSEEK_CREDENTIAL_TARGET).await? {
        Some(secret) => {
            validate_deepseek_secret(&secret).map_err(|_| SecretError::InvalidStoredSecret)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

#[allow(dead_code)]
pub(crate) fn windows_deepseek_credential_is_configured() -> Result<bool, SecretError> {
    #[cfg(windows)]
    {
        windows_get(DEEPSEEK_CREDENTIAL_TARGET).map(|secret| secret.is_some())
    }
    #[cfg(not(windows))]
    {
        Err(SecretError::Unavailable)
    }
}

fn validate_deepseek_secret(secret: &str) -> Result<(), SecretError> {
    let character_count = secret.chars().count();
    if !(16..=512).contains(&character_count) || secret.chars().any(char::is_control) {
        return Err(SecretError::InvalidSecret);
    }
    Ok(())
}

#[cfg(windows)]
impl SecretStore for WindowsCredentialStore {
    fn get<'a>(&'a self, target: &'a str) -> BoxFuture<'a, Result<Option<String>, SecretError>> {
        Box::pin(async move { windows_get(target) })
    }

    fn set<'a>(
        &'a self,
        target: &'a str,
        secret: &'a str,
    ) -> BoxFuture<'a, Result<(), SecretError>> {
        Box::pin(async move { windows_set(target, secret) })
    }

    fn delete<'a>(&'a self, target: &'a str) -> BoxFuture<'a, Result<(), SecretError>> {
        Box::pin(async move { windows_delete(target) })
    }
}

#[cfg(not(windows))]
impl SecretStore for WindowsCredentialStore {
    fn get<'a>(&'a self, _target: &'a str) -> BoxFuture<'a, Result<Option<String>, SecretError>> {
        Box::pin(async { Err(SecretError::Unavailable) })
    }

    fn set<'a>(
        &'a self,
        _target: &'a str,
        _secret: &'a str,
    ) -> BoxFuture<'a, Result<(), SecretError>> {
        Box::pin(async { Err(SecretError::Unavailable) })
    }

    fn delete<'a>(&'a self, _target: &'a str) -> BoxFuture<'a, Result<(), SecretError>> {
        Box::pin(async { Err(SecretError::Unavailable) })
    }
}

#[cfg(windows)]
fn windows_get(target: &str) -> Result<Option<String>, SecretError> {
    use std::{ptr, slice};

    use windows::{
        Win32::{
            Foundation::ERROR_NOT_FOUND,
            Security::Credentials::{CRED_TYPE_GENERIC, CREDENTIALW, CredFree, CredReadW},
        },
        core::{HRESULT, PCWSTR},
    };

    let target = wide_string(target);
    let mut credential = ptr::null_mut::<CREDENTIALW>();
    let read = unsafe {
        CredReadW(
            PCWSTR(target.as_ptr()),
            CRED_TYPE_GENERIC,
            None,
            &mut credential,
        )
    };
    if let Err(error) = read {
        if error.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0) {
            return Ok(None);
        }
        return Err(SecretError::Unavailable);
    }

    if credential.is_null() {
        return Err(SecretError::InvalidStoredSecret);
    }

    struct CredentialBuffer(*mut CREDENTIALW);
    impl Drop for CredentialBuffer {
        fn drop(&mut self) {
            unsafe { CredFree(self.0.cast()) };
        }
    }

    let buffer = CredentialBuffer(credential);
    let credential = unsafe { &*buffer.0 };
    let blob_size = credential.CredentialBlobSize as usize;
    if credential.CredentialBlob.is_null()
        || !(MIN_DEEPSEEK_SECRET_BYTES..=MAX_DEEPSEEK_SECRET_BYTES).contains(&blob_size)
    {
        return Err(SecretError::InvalidStoredSecret);
    }
    let bytes = unsafe { slice::from_raw_parts(credential.CredentialBlob.cast_const(), blob_size) };
    let secret = String::from_utf8(bytes.to_vec()).map_err(|_| SecretError::InvalidStoredSecret)?;
    validate_deepseek_secret(&secret).map_err(|_| SecretError::InvalidStoredSecret)?;
    Ok(Some(secret))
}

#[cfg(windows)]
fn windows_set(target: &str, secret: &str) -> Result<(), SecretError> {
    use windows::{
        Win32::Security::Credentials::{
            CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredWriteW,
        },
        core::PWSTR,
    };

    let mut target = wide_string(target);
    let mut username = wide_string("ArcGISProAgent");
    let mut blob = secret.as_bytes().to_vec();
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR(username.as_mut_ptr()),
        ..Default::default()
    };
    unsafe { CredWriteW(&credential, 0) }.map_err(|_| SecretError::Unavailable)
}

#[cfg(windows)]
fn windows_delete(target: &str) -> Result<(), SecretError> {
    use windows::{
        Win32::{
            Foundation::ERROR_NOT_FOUND,
            Security::Credentials::{CRED_TYPE_GENERIC, CredDeleteW},
        },
        core::{HRESULT, PCWSTR},
    };

    let target = wide_string(target);
    match unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None) } {
        Ok(()) => Ok(()),
        Err(error) if error.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0) => Ok(()),
        Err(_) => Err(SecretError::Unavailable),
    }
}

#[cfg(windows)]
fn wide_string(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

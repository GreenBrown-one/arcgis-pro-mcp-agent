use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::paths::{RUNTIME_FILE_NAME, runtime_directory};

const PIPE_NAME: &str = "ArcGISProAgent.Bridge.v1";
const TOKEN_BYTES: usize = 32;

pub struct RuntimeFile {
    path: PathBuf,
    pipe_name: &'static str,
    _token: SecretToken,
}

impl RuntimeFile {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn redaction_secret(&self) -> &str {
        &self._token.0
    }
}

impl fmt::Debug for RuntimeFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeFile")
            .field("path", &self.path)
            .field("pipe_name", &self.pipe_name)
            .field("token", &Redacted)
            .finish()
    }
}

impl fmt::Display for RuntimeFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "RuntimeFile(path={}, pipe_name={}, token={Redacted})",
            self.path.display(),
            self.pipe_name
        )
    }
}

pub struct RuntimeError {
    operation: &'static str,
    os_code: Option<i32>,
}

impl RuntimeError {
    fn io(operation: &'static str, error: &io::Error) -> Self {
        Self {
            operation,
            os_code: error.raw_os_error(),
        }
    }

    fn operation(operation: &'static str) -> Self {
        Self {
            operation,
            os_code: None,
        }
    }
}

impl fmt::Debug for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeError")
            .field("operation", &self.operation)
            .field("os_code", &self.os_code)
            .finish()
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "runtime credential operation failed: {}",
            self.operation
        )?;
        if let Some(code) = self.os_code {
            write!(formatter, " (OS error {code})")?;
        }
        Ok(())
    }
}

impl std::error::Error for RuntimeError {}

#[derive(Serialize)]
#[serde(transparent)]
struct SecretToken(String);

struct Redacted;

impl fmt::Debug for Redacted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl fmt::Display for Redacted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimePayload<'a> {
    pipe_name: &'a str,
    token: &'a SecretToken,
}

pub fn create_runtime_file(local_app_data: &Path) -> Result<RuntimeFile, RuntimeError> {
    let runtime_dir = runtime_directory(local_app_data);
    fs::create_dir_all(&runtime_dir)
        .map_err(|error| RuntimeError::io("create runtime directory", &error))?;

    let token = SecretToken(generate_token()?);
    let payload = RuntimePayload {
        pipe_name: PIPE_NAME,
        token: &token,
    };
    let bytes = serde_json::to_vec(&payload)
        .map_err(|_| RuntimeError::operation("serialize runtime credential"))?;
    let path = runtime_dir.join(RUNTIME_FILE_NAME);
    atomic_write_current_user_only(&path, &bytes)?;

    Ok(RuntimeFile {
        path,
        pipe_name: PIPE_NAME,
        _token: token,
    })
}

fn generate_token() -> Result<String, RuntimeError> {
    let mut bytes = [0_u8; TOKEN_BYTES];
    getrandom::fill(&mut bytes).map_err(|_| RuntimeError::operation("generate random token"))?;
    Ok(base64url_no_padding(&bytes))
}

fn base64url_no_padding(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut encoded = String::with_capacity((bytes.len() * 4).div_ceil(3));
    let mut chunks = bytes.chunks_exact(3);

    for chunk in &mut chunks {
        let value = u32::from(chunk[0]) << 16 | u32::from(chunk[1]) << 8 | u32::from(chunk[2]);
        encoded.push(ALPHABET[((value >> 18) & 0x3f) as usize] as char);
        encoded.push(ALPHABET[((value >> 12) & 0x3f) as usize] as char);
        encoded.push(ALPHABET[((value >> 6) & 0x3f) as usize] as char);
        encoded.push(ALPHABET[(value & 0x3f) as usize] as char);
    }

    let remainder = chunks.remainder();
    if let Some(&first) = remainder.first() {
        encoded.push(ALPHABET[(first >> 2) as usize] as char);
        if remainder.len() == 1 {
            encoded.push(ALPHABET[((first & 0x03) << 4) as usize] as char);
        } else {
            let second = remainder[1];
            encoded.push(ALPHABET[(((first & 0x03) << 4) | (second >> 4)) as usize] as char);
            encoded.push(ALPHABET[((second & 0x0f) << 2) as usize] as char);
        }
    }

    encoded
}

fn temporary_path(target: &Path) -> Result<PathBuf, RuntimeError> {
    let mut random = [0_u8; 12];
    getrandom::fill(&mut random)
        .map_err(|_| RuntimeError::operation("generate temporary file name"))?;
    let suffix = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(target.with_file_name(format!(".{RUNTIME_FILE_NAME}.{suffix}.tmp")))
}

#[cfg(windows)]
fn atomic_write_current_user_only(target: &Path, bytes: &[u8]) -> Result<(), RuntimeError> {
    windows_secure_file::write(target, bytes)
}

#[cfg(not(windows))]
fn atomic_write_current_user_only(target: &Path, bytes: &[u8]) -> Result<(), RuntimeError> {
    use std::fs::OpenOptions;
    use std::io::Write;

    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    let temporary = temporary_path(target)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(&temporary)
        .map_err(|error| RuntimeError::io("create protected temporary credential", &error))?;
    let result = (|| {
        file.write_all(bytes)
            .map_err(|error| RuntimeError::io("write runtime credential", &error))?;
        file.sync_all()
            .map_err(|error| RuntimeError::io("flush runtime credential", &error))?;
        drop(file);
        fs::rename(&temporary, target)
            .map_err(|error| RuntimeError::io("replace runtime credential", &error))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(windows)]
mod windows_secure_file {
    use std::{
        fs,
        io::Write,
        mem::size_of,
        os::windows::{ffi::OsStrExt, io::FromRawHandle},
        path::Path,
    };

    use windows::{
        Win32::{
            Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree},
            Security::{
                Authorization::{
                    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
                    SDDL_REVISION_1,
                },
                GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY,
                TOKEN_USER, TokenUser,
            },
            Storage::FileSystem::{
                CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_WRITE,
                FILE_SHARE_NONE, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
            },
            System::Threading::{GetCurrentProcess, OpenProcessToken},
        },
        core::{PCWSTR, PWSTR},
    };

    use super::{RuntimeError, temporary_path};

    pub(super) fn write(target: &Path, bytes: &[u8]) -> Result<(), RuntimeError> {
        let temporary = temporary_path(target)?;
        let descriptor = current_user_and_system_descriptor()?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0.0,
            bInheritHandle: false.into(),
        };
        let temporary_wide = wide(temporary.as_path());

        let handle = unsafe {
            CreateFileW(
                PCWSTR(temporary_wide.as_ptr()),
                FILE_GENERIC_WRITE.0,
                FILE_SHARE_NONE,
                Some(&attributes),
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        }
        .map_err(|error| {
            RuntimeError::io("create protected temporary credential", &io_error(error))
        })?;

        let mut file = unsafe { fs::File::from_raw_handle(handle.0) };
        let result = (|| {
            file.write_all(bytes)
                .map_err(|error| RuntimeError::io("write runtime credential", &error))?;
            file.sync_all()
                .map_err(|error| RuntimeError::io("flush runtime credential", &error))?;
            drop(file);

            let target_wide = wide(target);
            unsafe {
                MoveFileExW(
                    PCWSTR(temporary_wide.as_ptr()),
                    PCWSTR(target_wide.as_ptr()),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            }
            .map_err(|error| RuntimeError::io("replace runtime credential", &io_error(error)))?;
            Ok(())
        })();

        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }

    fn current_user_and_system_descriptor() -> Result<LocalAllocation, RuntimeError> {
        let mut token = HANDLE::default();
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
            .map_err(|error| RuntimeError::io("open process token", &io_error(error)))?;
        let token = OwnedHandle(token);

        let mut required = 0_u32;
        let _ = unsafe { GetTokenInformation(token.0, TokenUser, None, 0, &mut required) };
        if required == 0 {
            return Err(RuntimeError::operation("measure current user token"));
        }

        let mut information = vec![0_u8; required as usize];
        unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                Some(information.as_mut_ptr().cast()),
                required,
                &mut required,
            )
        }
        .map_err(|error| RuntimeError::io("read current user token", &io_error(error)))?;
        let token_user = unsafe { &*(information.as_ptr().cast::<TOKEN_USER>()) };

        let mut sid_text = PWSTR::null();
        unsafe { ConvertSidToStringSidW(token_user.User.Sid, &mut sid_text) }
            .map_err(|error| RuntimeError::io("format current user SID", &io_error(error)))?;
        let sid_allocation = LocalAllocation(HLOCAL(sid_text.0.cast()));
        let sid = unsafe { sid_text.to_string() }
            .map_err(|_| RuntimeError::operation("decode current user SID"))?;

        let sddl = format!("D:P(A;;FA;;;SY)(A;;FA;;;{sid})");
        drop(sid_allocation);
        let sddl_wide = wide_text(&sddl);
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl_wide.as_ptr()),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
        }
        .map_err(|error| RuntimeError::io("create protected DACL", &io_error(error)))?;
        Ok(LocalAllocation(HLOCAL(descriptor.0)))
    }

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain([0]).collect()
    }

    fn wide_text(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }

    fn io_error(error: windows::core::Error) -> std::io::Error {
        std::io::Error::from_raw_os_error(error.code().0)
    }

    struct OwnedHandle(HANDLE);

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }

    struct LocalAllocation(HLOCAL);

    impl Drop for LocalAllocation {
        fn drop(&mut self) {
            if !self.0.0.is_null() {
                let _ = unsafe { LocalFree(Some(self.0)) };
            }
        }
    }
}

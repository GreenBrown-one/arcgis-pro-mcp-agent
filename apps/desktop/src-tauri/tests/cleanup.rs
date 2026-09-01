use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

use arcgis_pro_agent_desktop_lib::{
    cleanup::{CleanupError, cleanup_for_uninstall_with},
    credential_store::SecretError,
};
use tempfile::TempDir;

struct WindowsJunctionFixture {
    _temp: TempDir,
    local_app_data: PathBuf,
    outside: PathBuf,
}

impl WindowsJunctionFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let local_app_data = temp.path().join("local-app-data");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&local_app_data).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let application_root = local_app_data.join("ArcGISProAgent");
        let status = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&application_root)
            .arg(&outside)
            .status()
            .unwrap();
        assert!(status.success());
        Self {
            _temp: temp,
            local_app_data,
            outside,
        }
    }

    fn local_app_data(&self) -> &Path {
        &self.local_app_data
    }

    fn outside_file(&self, name: &str) -> PathBuf {
        let path = self.outside.join(name);
        fs::write(&path, "keep").unwrap();
        path
    }
}

#[test]
fn cleanup_rejects_a_redirected_application_data_root() {
    let fixture = WindowsJunctionFixture::new();
    let outside = fixture.outside_file("keep.txt");
    let credential_delete_called = AtomicBool::new(false);
    let result = cleanup_for_uninstall_with(fixture.local_app_data(), || {
        credential_delete_called.store(true, Ordering::Release);
        Ok(())
    });
    assert_eq!(result, Err(CleanupError::UnsafeRoot));
    assert!(!credential_delete_called.load(Ordering::Acquire));
    assert_eq!(fs::read_to_string(outside).unwrap(), "keep");
}

#[test]
fn credential_delete_failure_leaves_owned_data_intact() {
    let temp = tempfile::tempdir().unwrap();
    let local_app_data = temp.path().join("local-app-data");
    let owned_file = local_app_data.join("ArcGISProAgent").join("keep.txt");
    fs::create_dir_all(owned_file.parent().unwrap()).unwrap();
    fs::write(&owned_file, "keep").unwrap();

    let result = cleanup_for_uninstall_with(&local_app_data, || Err(SecretError::Unavailable));

    assert_eq!(result, Err(CleanupError::CredentialRemoval));
    assert_eq!(fs::read_to_string(owned_file).unwrap(), "keep");
}

#[test]
fn successful_credential_delete_removes_only_the_owned_data_root() {
    let temp = tempfile::tempdir().unwrap();
    let local_app_data = temp.path().join("local-app-data");
    let owned_file = local_app_data.join("ArcGISProAgent").join("delete.txt");
    let sibling_file = local_app_data.join("other-app").join("keep.txt");
    fs::create_dir_all(owned_file.parent().unwrap()).unwrap();
    fs::create_dir_all(sibling_file.parent().unwrap()).unwrap();
    fs::write(&owned_file, "delete").unwrap();
    fs::write(&sibling_file, "keep").unwrap();

    assert_eq!(
        cleanup_for_uninstall_with(&local_app_data, || Ok(())),
        Ok(())
    );
    assert!(!owned_file.parent().unwrap().exists());
    assert_eq!(fs::read_to_string(sibling_file).unwrap(), "keep");
}

#[test]
fn missing_owned_data_still_runs_credential_cleanup_and_succeeds() {
    let temp = tempfile::tempdir().unwrap();
    let credential_delete_called = AtomicBool::new(false);

    let result = cleanup_for_uninstall_with(temp.path(), || {
        credential_delete_called.store(true, Ordering::Release);
        Ok(())
    });

    assert_eq!(result, Ok(()));
    assert!(credential_delete_called.load(Ordering::Acquire));
}

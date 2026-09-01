use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use arcgis_pro_agent_desktop_lib::{
    app_state::DesktopState,
    commands::{
        deepseek_clear_with, deepseek_configure_with, provider_select_with,
        save_arcgis_pro_root_with,
    },
    credential_store::{DEEPSEEK_CREDENTIAL_TARGET, SecretError, SecretStore, configure_deepseek},
    providers::{BoxFuture, ProviderAuthSnapshot, ProviderKind, ProviderRuntimeSnapshot},
    settings::{AppSettings, SettingsError, SettingsStore},
};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "arcgis-pro-agent-settings-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct MemorySecretStore {
    values: Mutex<HashMap<String, String>>,
}

#[derive(Default)]
struct CountingSecretStore {
    read_count: AtomicUsize,
}

impl CountingSecretStore {
    fn read_count(&self) -> usize {
        self.read_count.load(Ordering::Acquire)
    }
}

impl SecretStore for CountingSecretStore {
    fn get<'a>(&'a self, _target: &'a str) -> BoxFuture<'a, Result<Option<String>, SecretError>> {
        self.read_count.fetch_add(1, Ordering::AcqRel);
        Box::pin(async { Ok(None) })
    }

    fn set<'a>(
        &'a self,
        _target: &'a str,
        _secret: &'a str,
    ) -> BoxFuture<'a, Result<(), SecretError>> {
        Box::pin(async { Ok(()) })
    }

    fn delete<'a>(&'a self, _target: &'a str) -> BoxFuture<'a, Result<(), SecretError>> {
        Box::pin(async { Ok(()) })
    }
}

impl SecretStore for MemorySecretStore {
    fn get<'a>(&'a self, target: &'a str) -> BoxFuture<'a, Result<Option<String>, SecretError>> {
        Box::pin(async move {
            Ok(self
                .values
                .lock()
                .expect("memory secret store lock")
                .get(target)
                .cloned())
        })
    }

    fn set<'a>(
        &'a self,
        target: &'a str,
        secret: &'a str,
    ) -> BoxFuture<'a, Result<(), SecretError>> {
        Box::pin(async move {
            self.values
                .lock()
                .expect("memory secret store lock")
                .insert(target.to_owned(), secret.to_owned());
            Ok(())
        })
    }

    fn delete<'a>(&'a self, target: &'a str) -> BoxFuture<'a, Result<(), SecretError>> {
        Box::pin(async move {
            self.values
                .lock()
                .expect("memory secret store lock")
                .remove(target);
            Ok(())
        })
    }
}

#[test]
fn settings_never_serialize_a_deepseek_key() {
    let settings = AppSettings::default();
    let json = serde_json::to_string(&settings).unwrap();
    assert!(!json.to_ascii_lowercase().contains("api_key"));
    assert!(!json.to_ascii_lowercase().contains("apikey"));
    assert_eq!(settings.deepseek.base_url, "https://api.deepseek.com");
    assert_eq!(settings.deepseek.model, "deepseek-v4-flash");
}

#[test]
fn settings_store_atomically_replaces_the_versioned_non_secret_file() {
    use arcgis_pro_agent_desktop_lib::providers::ProviderKind;

    let local_app_data = TestDirectory::new();
    let store = SettingsStore::new(local_app_data.path());
    assert_eq!(
        store.path(),
        local_app_data
            .path()
            .join("ArcGISProAgent")
            .join("preview")
            .join("settings.json")
    );

    let mut settings = AppSettings::default();
    settings.active_provider = ProviderKind::DeepSeek;
    settings.arcgis_pro_root = Some(PathBuf::from(r"C:\Program Files\ArcGIS\Pro"));
    settings.onboarding_complete = true;
    store.save(&settings).unwrap();

    settings.deepseek.model = "deepseek-v4-flash-preview".to_owned();
    store.save(&settings).unwrap();
    assert_eq!(store.load().unwrap(), settings);

    let json = fs::read_to_string(store.path()).unwrap();
    assert!(!json.to_ascii_lowercase().contains("apikey"));
    assert!(!json.contains("sk-test-not-a-real-key"));
    assert_eq!(
        fs::read_dir(store.path().parent().unwrap())
            .unwrap()
            .count(),
        1,
        "successful replacement must not leave a temporary settings file"
    );
}

#[test]
fn settings_store_uses_defaults_when_the_settings_file_is_missing() {
    let local_app_data = TestDirectory::new();
    let store = SettingsStore::new(local_app_data.path());

    assert_eq!(store.load().unwrap(), AppSettings::default());
    assert!(!store.path().exists());
}

#[test]
fn settings_store_removes_the_temporary_file_when_atomic_replace_fails() {
    let local_app_data = TestDirectory::new();
    let store = SettingsStore::new(local_app_data.path());
    fs::create_dir_all(store.path()).unwrap();

    assert_eq!(
        store.save(&AppSettings::default()),
        Err(SettingsError::Unavailable)
    );
    let entries = fs::read_dir(store.path().parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(entries, vec![store.path().file_name().unwrap()]);
}

#[test]
fn settings_store_rejects_an_unknown_schema_version() {
    let local_app_data = TestDirectory::new();
    let store = SettingsStore::new(local_app_data.path());
    let mut json = serde_json::to_value(AppSettings::default()).unwrap();
    json["schemaVersion"] = serde_json::json!(2);
    fs::create_dir_all(store.path().parent().unwrap()).unwrap();
    fs::write(store.path(), serde_json::to_vec(&json).unwrap()).unwrap();

    assert_eq!(store.load(), Err(SettingsError::UnsupportedSchemaVersion));
}

#[tokio::test]
async fn selecting_a_provider_persists_it_and_updates_the_safe_snapshot() {
    let local_app_data = TestDirectory::new();
    let secrets = Arc::new(MemorySecretStore::default());
    let state = DesktopState::with_secret_store(local_app_data.path().to_owned(), secrets).await;

    let snapshot = provider_select_with(&state, ProviderKind::DeepSeek)
        .await
        .unwrap();

    assert_eq!(snapshot.provider.kind, ProviderKind::DeepSeek);
    assert_eq!(snapshot.provider.auth, ProviderAuthSnapshot::NeedsSetup);
    assert_eq!(snapshot.provider.runtime, ProviderRuntimeSnapshot::Stopped);
    assert_eq!(
        SettingsStore::new(local_app_data.path())
            .load()
            .unwrap()
            .active_provider,
        ProviderKind::DeepSeek
    );
}

#[tokio::test]
async fn desktop_state_surfaces_settings_initialization_errors() {
    for (fixture, expected_code) in [
        (b"{".as_slice(), "settings_invalid"),
        (
            br#"{"schemaVersion":2,"activeProvider":"codex","arcgisProRoot":null,"deepseek":{"baseUrl":"https://api.deepseek.com","model":"deepseek-v4-flash"},"onboardingComplete":false}"#
                .as_slice(),
            "settings_version_unsupported",
        ),
    ] {
        let local_app_data = TestDirectory::new();
        let store = SettingsStore::new(local_app_data.path());
        fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        fs::write(store.path(), fixture).unwrap();
        let state = DesktopState::with_secret_store(
            local_app_data.path().to_owned(),
            Arc::new(MemorySecretStore::default()),
        )
        .await;

        assert_eq!(
            state.snapshot().await.provider.auth,
            ProviderAuthSnapshot::Error {
                code: expected_code.to_owned()
            }
        );
    }

    let local_app_data = TestDirectory::new();
    let store = SettingsStore::new(local_app_data.path());
    fs::create_dir_all(store.path()).unwrap();
    let state = DesktopState::with_secret_store(
        local_app_data.path().to_owned(),
        Arc::new(MemorySecretStore::default()),
    )
    .await;
    assert_eq!(
        state.snapshot().await.provider.auth,
        ProviderAuthSnapshot::Error {
            code: "settings_unavailable".to_owned()
        }
    );
}

#[tokio::test]
async fn persisted_deepseek_selection_is_normalized_to_codex_without_reading_a_secret() {
    let local_app_data = TestDirectory::new();
    let settings_store = SettingsStore::new(local_app_data.path());
    let mut settings = AppSettings::default();
    settings.active_provider = ProviderKind::DeepSeek;
    settings_store.save(&settings).unwrap();
    let secrets = Arc::new(CountingSecretStore::default());

    let state =
        DesktopState::with_secret_store(local_app_data.path().to_owned(), secrets.clone()).await;

    assert_eq!(state.snapshot().await.provider.kind, ProviderKind::Codex);
    assert_eq!(
        state.settings_store().load().unwrap().active_provider,
        ProviderKind::Codex
    );
    assert_eq!(secrets.read_count(), 0);
}

#[tokio::test]
async fn deepseek_commands_mutate_only_the_credential_and_safe_snapshot() {
    let local_app_data = TestDirectory::new();
    let secrets = Arc::new(MemorySecretStore::default());
    let state =
        DesktopState::with_secret_store(local_app_data.path().to_owned(), secrets.clone()).await;
    provider_select_with(&state, ProviderKind::DeepSeek)
        .await
        .unwrap();
    let secret = "sk-command-not-a-real-key";

    let configured = deepseek_configure_with(&state, secret).await.unwrap();
    assert_eq!(
        secrets
            .get(DEEPSEEK_CREDENTIAL_TARGET)
            .await
            .unwrap()
            .as_deref(),
        Some(secret)
    );
    assert!(matches!(
        configured.provider.auth,
        ProviderAuthSnapshot::Ready { .. }
    ));
    assert!(!serde_json::to_string(&configured).unwrap().contains(secret));
    assert!(
        !fs::read_to_string(state.settings_store().path())
            .unwrap()
            .contains(secret)
    );

    let cleared = deepseek_clear_with(&state).await.unwrap();
    assert_eq!(secrets.get(DEEPSEEK_CREDENTIAL_TARGET).await.unwrap(), None);
    assert_eq!(cleared.provider.auth, ProviderAuthSnapshot::NeedsSetup);
}

#[tokio::test]
async fn provider_mutations_are_serialized_across_settings_credentials_and_snapshot() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use tokio::sync::{Semaphore, mpsc};

    struct BarrierSecretStore {
        values: Mutex<HashMap<String, String>>,
        block_next_get: AtomicBool,
        get_observed: mpsc::UnboundedSender<()>,
        get_release: Semaphore,
        set_observed: mpsc::UnboundedSender<()>,
        set_release: Semaphore,
    }

    impl SecretStore for BarrierSecretStore {
        fn get<'a>(
            &'a self,
            target: &'a str,
        ) -> BoxFuture<'a, Result<Option<String>, SecretError>> {
            let captured = self
                .values
                .lock()
                .expect("barrier secret store lock")
                .get(target)
                .cloned();
            let should_block = self.block_next_get.swap(false, Ordering::SeqCst);
            Box::pin(async move {
                if should_block {
                    self.get_observed.send(()).unwrap();
                    self.get_release.acquire().await.unwrap().forget();
                }
                Ok(captured)
            })
        }

        fn set<'a>(
            &'a self,
            target: &'a str,
            secret: &'a str,
        ) -> BoxFuture<'a, Result<(), SecretError>> {
            Box::pin(async move {
                self.values
                    .lock()
                    .expect("barrier secret store lock")
                    .insert(target.to_owned(), secret.to_owned());
                self.set_observed.send(()).unwrap();
                self.set_release.acquire().await.unwrap().forget();
                Ok(())
            })
        }

        fn delete<'a>(&'a self, target: &'a str) -> BoxFuture<'a, Result<(), SecretError>> {
            Box::pin(async move {
                self.values
                    .lock()
                    .expect("barrier secret store lock")
                    .remove(target);
                Ok(())
            })
        }
    }

    let local_app_data = TestDirectory::new();
    let (get_observed_tx, mut get_observed_rx) = mpsc::unbounded_channel();
    let (set_observed_tx, mut set_observed_rx) = mpsc::unbounded_channel();
    let secrets = Arc::new(BarrierSecretStore {
        values: Mutex::new(HashMap::new()),
        block_next_get: AtomicBool::new(true),
        get_observed: get_observed_tx,
        get_release: Semaphore::new(0),
        set_observed: set_observed_tx,
        set_release: Semaphore::new(0),
    });
    let state = Arc::new(
        DesktopState::with_secret_store(local_app_data.path().to_owned(), secrets.clone()).await,
    );

    let select_state = state.clone();
    let select =
        tokio::spawn(
            async move { provider_select_with(&select_state, ProviderKind::DeepSeek).await },
        );
    tokio::time::timeout(Duration::from_secs(1), get_observed_rx.recv())
        .await
        .expect("provider selection must reach credential read")
        .expect("credential read observation channel");

    let arcgis_root = PathBuf::from(r"D:\arcgis_pro");
    let arcgis_state = state.clone();
    let arcgis_root_for_task = arcgis_root.clone();
    let (arcgis_done_tx, mut arcgis_done_rx) = mpsc::unbounded_channel();
    let save_arcgis = tokio::spawn(async move {
        let result = save_arcgis_pro_root_with(&arcgis_state, arcgis_root_for_task).await;
        arcgis_done_tx.send(()).unwrap();
        result
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(100), arcgis_done_rx.recv())
            .await
            .is_err(),
        "ArcGIS settings save must wait for the in-flight provider mutation"
    );

    let configure_state = state.clone();
    let configure = tokio::spawn(async move {
        deepseek_configure_with(&configure_state, "sk-concurrent-not-a-real-key").await
    });
    let set_reached_before_read_release =
        tokio::time::timeout(Duration::from_secs(1), set_observed_rx.recv())
            .await
            .is_ok();

    if set_reached_before_read_release {
        secrets.set_release.add_permits(1);
        configure.await.unwrap().unwrap();
        secrets.get_release.add_permits(1);
        select.await.unwrap().unwrap();
    } else {
        secrets.get_release.add_permits(1);
        select.await.unwrap().unwrap();
        tokio::time::timeout(Duration::from_secs(1), set_observed_rx.recv())
            .await
            .expect("configuration must reach credential write after selection")
            .expect("credential write observation channel");
        secrets.set_release.add_permits(1);
        configure.await.unwrap().unwrap();
    }
    save_arcgis.await.unwrap().unwrap();

    let persisted = SettingsStore::new(local_app_data.path()).load().unwrap();
    assert_eq!(persisted.active_provider, ProviderKind::DeepSeek);
    assert_eq!(persisted.arcgis_pro_root, Some(arcgis_root));
    assert!(
        secrets
            .values
            .lock()
            .unwrap()
            .contains_key(DEEPSEEK_CREDENTIAL_TARGET)
    );
    assert!(matches!(
        state.snapshot().await.provider.auth,
        ProviderAuthSnapshot::Ready { .. }
    ));
}

#[tokio::test]
async fn configuring_deepseek_stores_only_the_secret_reference() {
    let secrets = MemorySecretStore::default();
    configure_deepseek(&secrets, "sk-test-not-a-real-key")
        .await
        .unwrap();
    assert_eq!(
        secrets
            .get(DEEPSEEK_CREDENTIAL_TARGET)
            .await
            .unwrap()
            .as_deref(),
        Some("sk-test-not-a-real-key")
    );
}

#[tokio::test]
async fn configuring_deepseek_validates_length_and_control_characters_before_writing() {
    for invalid in [
        "",
        "123456789012345",
        "sk-test-has-newline\n",
        "sk-test-has-control\u{0000}",
        &"x".repeat(513),
    ] {
        let secrets = MemorySecretStore::default();
        assert_eq!(
            configure_deepseek(&secrets, invalid).await,
            Err(SecretError::InvalidSecret)
        );
        assert!(
            secrets
                .get(DEEPSEEK_CREDENTIAL_TARGET)
                .await
                .unwrap()
                .is_none()
        );
    }

    for valid in ["x".repeat(16), "x".repeat(512)] {
        let secrets = MemorySecretStore::default();
        configure_deepseek(&secrets, &valid).await.unwrap();
        assert_eq!(
            secrets
                .get(DEEPSEEK_CREDENTIAL_TARGET)
                .await
                .unwrap()
                .as_deref(),
            Some(valid.as_str())
        );
    }
}

#[cfg(windows)]
#[tokio::test]
async fn windows_credential_store_round_trips_a_unique_dummy_secret() {
    use std::time::{SystemTime, UNIX_EPOCH};

    use arcgis_pro_agent_desktop_lib::credential_store::WindowsCredentialStore;
    use windows::{
        Win32::Security::Credentials::{CRED_TYPE_GENERIC, CredDeleteW},
        core::PCWSTR,
    };

    struct CredentialCleanup(Vec<u16>);

    impl Drop for CredentialCleanup {
        fn drop(&mut self) {
            unsafe {
                let _ = CredDeleteW(PCWSTR(self.0.as_ptr()), CRED_TYPE_GENERIC, None);
            }
        }
    }

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let target = format!("ArcGISProAgent.Preview.Test.{}.{nonce}", std::process::id());
    let target_wide = target.encode_utf16().chain(Some(0)).collect();
    let _cleanup = CredentialCleanup(target_wide);
    let secret = format!("sk-test-{nonce}-not-real");
    let store = WindowsCredentialStore;

    store.set(&target, &secret).await.unwrap();
    assert_eq!(
        store.get(&target).await.unwrap().as_deref(),
        Some(secret.as_str())
    );
    store.delete(&target).await.unwrap();
    assert_eq!(store.get(&target).await.unwrap(), None);
}

#[cfg(windows)]
#[tokio::test]
async fn windows_credential_store_rejects_invalid_raw_blobs() {
    use arcgis_pro_agent_desktop_lib::credential_store::WindowsCredentialStore;
    use windows::{
        Win32::Security::Credentials::{
            CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredWriteW,
        },
        core::{PCWSTR, PWSTR},
    };

    struct CredentialCleanup(Vec<u16>);

    impl Drop for CredentialCleanup {
        fn drop(&mut self) {
            unsafe {
                let _ = CredDeleteW(PCWSTR(self.0.as_ptr()), CRED_TYPE_GENERIC, None);
            }
        }
    }

    fn write_raw(target: &str, bytes: &[u8]) {
        let mut target = target.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let mut username = "ArcGISProAgent"
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(target.as_mut_ptr()),
            CredentialBlobSize: bytes.len() as u32,
            CredentialBlob: bytes.as_ptr().cast_mut(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: PWSTR(username.as_mut_ptr()),
            ..Default::default()
        };
        unsafe { CredWriteW(&credential, 0) }.unwrap();
    }

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let target = format!(
        "ArcGISProAgent.Preview.InvalidTest.{}.{nonce}",
        std::process::id()
    );
    let target_wide = target.encode_utf16().chain(Some(0)).collect();
    let _cleanup = CredentialCleanup(target_wide);
    let store = WindowsCredentialStore;
    let invalid_utf8 = vec![0xff; 16];
    let too_long = vec![b'x'; 513];

    for invalid in [
        &[][..],
        &invalid_utf8,
        b"123456789012345",
        &too_long,
        b"123456789012345\n",
    ] {
        write_raw(&target, invalid);
        assert_eq!(
            store.get(&target).await,
            Err(SecretError::InvalidStoredSecret)
        );
    }
}

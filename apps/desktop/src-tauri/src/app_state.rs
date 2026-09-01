use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    sync::{Mutex, RwLock},
    task::JoinHandle,
};

use crate::{
    arcgis_install::ArcGisInstallSnapshot,
    codex::CodexEvent,
    credential_store::{SecretError, SecretStore, WindowsCredentialStore},
    mcp_status::{ArcGisMcpReadiness, ArcGisStatusUpdate, Lifecycle},
    providers::{
        ProviderAuthSnapshot, ProviderKind, ProviderRuntimeSnapshot, ProviderSnapshot,
        codex::snapshot as codex_provider_snapshot,
    },
    settings::{SettingsError, SettingsStore},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BridgeStatus {
    Connected,
    Disconnected,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextExtentSnapshot {
    pub x_min: f64,
    pub y_min: f64,
    pub x_max: f64,
    pub y_max: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wkid: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveViewSnapshot {
    pub uri: String,
    pub name: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extent: Option<ContextExtentSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerSnapshot {
    pub uri: String,
    pub name: String,
    pub long_name: String,
    pub layer_type: String,
    pub parent_uri: Option<String>,
    pub depth: u16,
    pub visible: bool,
    pub is_feature_layer: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeSnapshot {
    pub status: BridgeStatus,
    pub context_is_live: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub add_in_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arc_gis_pro_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_has_unsaved_changes: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_map_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_view: Option<ActiveViewSnapshot>,
    pub layers: Vec<LayerSnapshot>,
    pub last_updated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Default for BridgeSnapshot {
    fn default() -> Self {
        Self {
            status: BridgeStatus::Disconnected,
            context_is_live: false,
            protocol_version: None,
            add_in_version: None,
            arc_gis_pro_version: None,
            project_name: None,
            project_has_unsaved_changes: None,
            active_map_name: None,
            active_view: None,
            layers: Vec::new(),
            last_updated: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum AccountSnapshot {
    Checking,
    SignedOut,
    LoginPending {
        #[serde(rename = "loginId")]
        login_id: String,
    },
    UnsupportedAuth,
    SignedIn {
        email: Option<String>,
        #[serde(rename = "planType")]
        plan_type: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum CodexSnapshot {
    Starting,
    Ready {
        version: String,
        #[serde(rename = "versionVerified")]
        version_verified: bool,
    },
    Error {
        code: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSnapshot {
    #[serde(skip_serializing)]
    pub account: AccountSnapshot,
    pub arcgis: BridgeSnapshot,
    pub arcgis_install: ArcGisInstallSnapshot,
    #[serde(skip_serializing)]
    pub codex: CodexSnapshot,
    pub provider: ProviderSnapshot,
    pub session_generation: u64,
}

impl Default for DesktopSnapshot {
    fn default() -> Self {
        let account = AccountSnapshot::Checking;
        let codex = CodexSnapshot::Starting;
        Self {
            provider: codex_provider_snapshot(&account, &codex),
            account,
            arcgis: BridgeSnapshot::default(),
            arcgis_install: ArcGisInstallSnapshot::Checking,
            codex,
            session_generation: 0,
        }
    }
}

impl DesktopSnapshot {
    fn sync_provider(&mut self, active_provider: ProviderKind, deepseek_configured: bool) {
        self.provider = match active_provider {
            ProviderKind::Codex => codex_provider_snapshot(&self.account, &self.codex),
            ProviderKind::DeepSeek => ProviderSnapshot {
                kind: ProviderKind::DeepSeek,
                auth: if deepseek_configured {
                    ProviderAuthSnapshot::Ready {
                        label: None,
                        plan: None,
                    }
                } else {
                    ProviderAuthSnapshot::NeedsSetup
                },
                runtime: ProviderRuntimeSnapshot::Stopped,
            },
        };
    }
}

pub fn retain_stale_snapshot(last_success: &BridgeSnapshot, _error: &str) -> BridgeSnapshot {
    BridgeSnapshot {
        status: BridgeStatus::Disconnected,
        context_is_live: false,
        error: Some("ArcGIS 连接检查失败".to_owned()),
        ..last_success.clone()
    }
}

#[derive(Debug, Default)]
pub struct PollGate {
    cancelled: AtomicBool,
}

#[derive(Debug, Default, Clone)]
struct ActiveConversation {
    thread_id: Option<String>,
    turn_id: Option<String>,
    turn_starting: bool,
    turn_interrupting: bool,
    conversation_starting: bool,
}

pub type AppServerFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq)]
pub enum ServerReply {
    Result(Value),
    Error { code: i64, message: String },
}

pub trait AppServerClient: Send + Sync {
    fn request<'a>(&'a self, method: &'a str, params: Value) -> AppServerFuture<'a, Value>;
    fn respond<'a>(&'a self, id: Value, reply: ServerReply) -> AppServerFuture<'a, ()>;
}

struct Coordinator {
    snapshot: DesktopSnapshot,
    active: ActiveConversation,
    client: Option<Arc<dyn AppServerClient>>,
    generation: u64,
    runtime_epoch: u64,
    pending_runtime_epoch: Option<u64>,
    ready_epoch: u64,
    visibility_epoch: u64,
    arcgis_mcp: ArcGisMcpReadiness,
    visible: bool,
    active_provider: ProviderKind,
    deepseek_configured: bool,
    settings_initialization_error: Option<SettingsError>,
    provider_initialization_error: Option<SecretError>,
}

impl Default for Coordinator {
    fn default() -> Self {
        Self {
            snapshot: DesktopSnapshot::default(),
            active: ActiveConversation::default(),
            client: None,
            generation: 0,
            runtime_epoch: 0,
            pending_runtime_epoch: None,
            ready_epoch: 0,
            visibility_epoch: 0,
            arcgis_mcp: ArcGisMcpReadiness::default(),
            visible: false,
            active_provider: ProviderKind::Codex,
            deepseek_configured: false,
            settings_initialization_error: None,
            provider_initialization_error: None,
        }
    }
}

impl Coordinator {
    fn sync_provider(&mut self) {
        if let Some(error) = self.settings_initialization_error {
            self.snapshot.provider = ProviderSnapshot {
                kind: self.active_provider,
                auth: ProviderAuthSnapshot::Error {
                    code: match error {
                        SettingsError::InvalidSettings => "settings_invalid",
                        SettingsError::UnsupportedSchemaVersion => "settings_version_unsupported",
                        SettingsError::Unavailable => "settings_unavailable",
                    }
                    .to_owned(),
                },
                runtime: ProviderRuntimeSnapshot::Stopped,
            };
            return;
        }
        if self.active_provider == ProviderKind::DeepSeek
            && let Some(error) = self.provider_initialization_error
        {
            self.snapshot.provider = ProviderSnapshot {
                kind: ProviderKind::DeepSeek,
                auth: ProviderAuthSnapshot::Error {
                    code: match error {
                        SecretError::InvalidSecret | SecretError::InvalidStoredSecret => {
                            "provider_credentials_invalid"
                        }
                        SecretError::Unavailable => "provider_credentials_unavailable",
                    }
                    .to_owned(),
                },
                runtime: ProviderRuntimeSnapshot::Stopped,
            };
            return;
        }
        self.snapshot
            .sync_provider(self.active_provider, self.deepseek_configured);
    }
}

#[derive(Clone)]
pub struct TurnLease {
    generation: u64,
    thread_id: String,
}

impl TurnLease {
    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }
}

#[derive(Clone)]
pub struct InterruptLease {
    generation: u64,
    thread_id: String,
    turn_id: String,
}

impl InterruptLease {
    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }

    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }
}

#[derive(Clone)]
pub struct ConversationLease {
    generation: u64,
    runtime_epoch: u64,
}

#[derive(Clone)]
pub struct McpDiscoveryLease {
    generation: u64,
    runtime_epoch: u64,
    thread_id: String,
}

impl McpDiscoveryLease {
    pub fn runtime_epoch(&self) -> u64 {
        self.runtime_epoch
    }

    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }
}

#[derive(Clone)]
pub struct HealthLease {
    generation: u64,
    runtime_epoch: u64,
    ready_epoch: u64,
    visibility_epoch: u64,
    thread_id: String,
}

impl HealthLease {
    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }
}

pub(crate) type RuntimeFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub(crate) trait RuntimeProcess: Send + Sync {
    fn next_event<'a>(&'a self) -> RuntimeFuture<'a, Option<CodexEvent>>;
    fn persistent_event_waiter_count(&self) -> usize;
    fn shutdown<'a>(&'a self) -> RuntimeFuture<'a, Result<(), String>>;
    fn ensure_terminated<'a>(&'a self) -> RuntimeFuture<'a, Result<(), String>>;
}

#[derive(Clone)]
pub(crate) struct ManagedRuntime {
    process: Arc<dyn RuntimeProcess>,
    client: Arc<dyn AppServerClient>,
}

impl ManagedRuntime {
    pub(crate) fn new(process: Arc<dyn RuntimeProcess>, client: Arc<dyn AppServerClient>) -> Self {
        Self { process, client }
    }

    pub(crate) fn process(&self) -> &dyn RuntimeProcess {
        self.process.as_ref()
    }

    pub(crate) fn client(&self) -> Arc<dyn AppServerClient> {
        self.client.clone()
    }
}

struct InstalledRuntime {
    epoch: u64,
    runtime: ManagedRuntime,
    event_task: Option<JoinHandle<()>>,
}

pub(crate) struct RuntimeOwnership {
    runtime: ManagedRuntime,
    event_task: Option<JoinHandle<()>>,
    join_event_task: bool,
}

impl RuntimeOwnership {
    pub(crate) fn failed_start(
        runtime: ManagedRuntime,
        event_task: Option<JoinHandle<()>>,
    ) -> Self {
        Self {
            runtime,
            event_task,
            join_event_task: true,
        }
    }

    pub(crate) fn runtime(&self) -> &ManagedRuntime {
        &self.runtime
    }

    pub(crate) fn into_parts(self) -> (ManagedRuntime, Option<JoinHandle<()>>, bool) {
        (self.runtime, self.event_task, self.join_event_task)
    }
}

impl From<InstalledRuntime> for RuntimeOwnership {
    fn from(installed: InstalledRuntime) -> Self {
        Self {
            runtime: installed.runtime,
            event_task: installed.event_task,
            join_event_task: false,
        }
    }
}

pub struct DesktopState {
    local_app_data: PathBuf,
    settings_store: SettingsStore,
    secret_store: Arc<dyn SecretStore>,
    settings_mutation: Mutex<()>,
    runtime_restart: Mutex<()>,
    runtime: RwLock<Option<InstalledRuntime>>,
    quarantined_runtime: Mutex<Option<RuntimeOwnership>>,
    coordinator: Mutex<Coordinator>,
    poll_gate: PollGate,
}

impl DesktopState {
    pub fn new(local_app_data: PathBuf) -> Self {
        let settings_store = SettingsStore::new(&local_app_data);
        let settings_result = settings_store.load_chatgpt_only();
        Self::from_initialization(
            local_app_data,
            settings_store,
            Arc::new(WindowsCredentialStore),
            settings_result,
            Ok(false),
        )
    }

    pub async fn with_secret_store(
        local_app_data: PathBuf,
        secret_store: Arc<dyn SecretStore>,
    ) -> Self {
        let settings_store = SettingsStore::new(&local_app_data);
        let settings_result = settings_store.load_chatgpt_only();
        Self::from_initialization(
            local_app_data,
            settings_store,
            secret_store,
            settings_result,
            Ok(false),
        )
    }

    fn from_initialization(
        local_app_data: PathBuf,
        settings_store: SettingsStore,
        secret_store: Arc<dyn SecretStore>,
        settings_result: Result<crate::settings::AppSettings, SettingsError>,
        credential_result: Result<bool, SecretError>,
    ) -> Self {
        let (active_provider, settings_initialization_error) = match settings_result {
            Ok(settings) => (settings.active_provider, None),
            Err(error) => (ProviderKind::Codex, Some(error)),
        };
        let (deepseek_configured, provider_initialization_error) = match credential_result {
            Ok(configured) => (configured, None),
            Err(error) => (false, Some(error)),
        };
        let mut coordinator = Coordinator {
            active_provider,
            deepseek_configured,
            settings_initialization_error,
            provider_initialization_error,
            ..Coordinator::default()
        };
        coordinator.sync_provider();
        Self {
            local_app_data,
            settings_store,
            secret_store,
            settings_mutation: Mutex::new(()),
            runtime_restart: Mutex::new(()),
            runtime: RwLock::new(None),
            quarantined_runtime: Mutex::new(None),
            coordinator: Mutex::new(coordinator),
            poll_gate: PollGate::new(),
        }
    }

    pub fn local_app_data(&self) -> &Path {
        &self.local_app_data
    }

    pub fn settings_store(&self) -> &SettingsStore {
        &self.settings_store
    }

    pub fn secret_store(&self) -> &dyn SecretStore {
        self.secret_store.as_ref()
    }

    pub(crate) async fn lock_settings_mutation(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.settings_mutation.lock().await
    }

    pub async fn select_provider(
        &self,
        provider: ProviderKind,
        deepseek_configured: bool,
    ) -> DesktopSnapshot {
        let mut coordinator = self.coordinator.lock().await;
        coordinator.active_provider = provider;
        coordinator.deepseek_configured = deepseek_configured;
        coordinator.provider_initialization_error = None;
        coordinator.sync_provider();
        coordinator.snapshot.clone()
    }

    pub async fn snapshot(&self) -> DesktopSnapshot {
        self.coordinator.lock().await.snapshot.clone()
    }

    pub async fn update_snapshot(
        &self,
        update: impl FnOnce(&mut DesktopSnapshot),
    ) -> DesktopSnapshot {
        let mut coordinator = self.coordinator.lock().await;
        update(&mut coordinator.snapshot);
        coordinator.sync_provider();
        coordinator.snapshot.clone()
    }

    pub async fn apply_account(&self, account: AccountSnapshot) -> DesktopSnapshot {
        let mut coordinator = self.coordinator.lock().await;
        apply_account_locked(&mut coordinator, account);
        coordinator.sync_provider();
        coordinator.snapshot.clone()
    }

    pub async fn session_generation(&self) -> u64 {
        self.coordinator.lock().await.generation
    }

    pub(crate) async fn lock_runtime_restart(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.runtime_restart.lock().await
    }

    pub(crate) async fn runtime_epoch(&self) -> u64 {
        self.coordinator.lock().await.runtime_epoch
    }

    pub(crate) async fn restart_completed_since(&self, observed_epoch: u64) -> bool {
        let coordinator = self.coordinator.lock().await;
        coordinator.runtime_epoch != observed_epoch
            && coordinator.pending_runtime_epoch.is_none()
            && matches!(coordinator.snapshot.codex, CodexSnapshot::Ready { .. })
    }

    pub(crate) async fn begin_runtime_restart(
        &self,
    ) -> (u64, Option<RuntimeOwnership>, DesktopSnapshot) {
        let old_runtime = self
            .runtime
            .write()
            .await
            .take()
            .map(RuntimeOwnership::from);
        let mut coordinator = self.coordinator.lock().await;
        coordinator.generation = coordinator.generation.wrapping_add(1);
        coordinator.snapshot.session_generation = coordinator.generation;
        coordinator.client = None;
        coordinator.runtime_epoch = coordinator.runtime_epoch.wrapping_add(1);
        let pending_epoch = coordinator.runtime_epoch.wrapping_add(1);
        coordinator.pending_runtime_epoch = Some(pending_epoch);
        coordinator.active = ActiveConversation::default();
        coordinator.arcgis_mcp = ArcGisMcpReadiness::default();
        coordinator.ready_epoch = coordinator.ready_epoch.wrapping_add(1);
        coordinator.snapshot.account = AccountSnapshot::Checking;
        coordinator.snapshot.codex = CodexSnapshot::Starting;
        coordinator.snapshot.arcgis =
            retain_stale_snapshot(&coordinator.snapshot.arcgis, "Codex runtime restarting");
        coordinator.sync_provider();
        (pending_epoch, old_runtime, coordinator.snapshot.clone())
    }

    pub(crate) async fn publish_runtime_ready(
        &self,
        runtime_epoch: u64,
        runtime: ManagedRuntime,
        installation_version: String,
        version_verified: bool,
        account: AccountSnapshot,
    ) -> bool {
        {
            let coordinator = self.coordinator.lock().await;
            if coordinator.pending_runtime_epoch != Some(runtime_epoch) {
                return false;
            }
        }
        *self.runtime.write().await = Some(InstalledRuntime {
            epoch: runtime_epoch,
            runtime: runtime.clone(),
            event_task: None,
        });
        let mut coordinator = self.coordinator.lock().await;
        if coordinator.pending_runtime_epoch != Some(runtime_epoch) {
            drop(coordinator);
            let mut installed = self.runtime.write().await;
            if installed
                .as_ref()
                .is_some_and(|installed| installed.epoch == runtime_epoch)
            {
                *installed = None;
            }
            return false;
        }
        coordinator.runtime_epoch = runtime_epoch;
        coordinator.pending_runtime_epoch = None;
        coordinator.client = Some(runtime.client());
        coordinator.snapshot.codex = CodexSnapshot::Ready {
            version: installation_version,
            version_verified,
        };
        apply_account_locked(&mut coordinator, account);
        coordinator.sync_provider();
        true
    }

    pub(crate) async fn install_runtime_event_task(
        &self,
        runtime_epoch: u64,
        event_task: JoinHandle<()>,
    ) -> Result<(), JoinHandle<()>> {
        let mut installed = self.runtime.write().await;
        let Some(installed) = installed
            .as_mut()
            .filter(|installed| installed.epoch == runtime_epoch)
        else {
            return Err(event_task);
        };
        installed.event_task = Some(event_task);
        Ok(())
    }

    pub(crate) async fn take_quarantined_runtime(&self) -> Option<RuntimeOwnership> {
        self.quarantined_runtime.lock().await.take()
    }

    pub(crate) async fn restore_quarantined_runtime(&self, runtime: RuntimeOwnership) {
        let mut quarantined = self.quarantined_runtime.lock().await;
        debug_assert!(quarantined.is_none());
        *quarantined = Some(runtime);
    }

    pub(crate) async fn take_runtime_if_epoch(
        &self,
        runtime_epoch: u64,
    ) -> Option<RuntimeOwnership> {
        let mut installed = self.runtime.write().await;
        if installed
            .as_ref()
            .is_some_and(|installed| installed.epoch == runtime_epoch)
        {
            installed.take().map(RuntimeOwnership::from)
        } else {
            None
        }
    }

    pub(crate) async fn mark_runtime_protocol_error_if_epoch(
        &self,
        runtime_epoch: u64,
    ) -> Option<DesktopSnapshot> {
        self.publish_runtime_error_if_epoch(runtime_epoch, "codex_incompatible")
            .await
    }

    pub(crate) async fn mark_runtime_error_if_epoch(
        &self,
        runtime_epoch: u64,
        code: &str,
    ) -> Option<DesktopSnapshot> {
        let snapshot = self
            .publish_runtime_error_if_epoch(runtime_epoch, code)
            .await?;
        let mut installed = self.runtime.write().await;
        if installed
            .as_ref()
            .is_some_and(|installed| installed.epoch == runtime_epoch)
        {
            *installed = None;
        }
        Some(snapshot)
    }

    async fn publish_runtime_error_if_epoch(
        &self,
        runtime_epoch: u64,
        code: &str,
    ) -> Option<DesktopSnapshot> {
        let mut coordinator = self.coordinator.lock().await;
        if coordinator.runtime_epoch != runtime_epoch
            && coordinator.pending_runtime_epoch != Some(runtime_epoch)
        {
            return None;
        }
        coordinator.generation = coordinator.generation.wrapping_add(1);
        coordinator.snapshot.session_generation = coordinator.generation;
        coordinator.client = None;
        coordinator.runtime_epoch = runtime_epoch.wrapping_add(1);
        coordinator.pending_runtime_epoch = None;
        coordinator.active = ActiveConversation::default();
        coordinator.arcgis_mcp = ArcGisMcpReadiness::default();
        coordinator.ready_epoch = coordinator.ready_epoch.wrapping_add(1);
        coordinator.snapshot.account = AccountSnapshot::Checking;
        coordinator.snapshot.codex = CodexSnapshot::Error {
            code: code.to_owned(),
        };
        coordinator.snapshot.arcgis =
            retain_stale_snapshot(&coordinator.snapshot.arcgis, "Codex runtime stopped");
        coordinator.sync_provider();
        Some(coordinator.snapshot.clone())
    }

    pub(crate) async fn mark_runtime_stopped_if_epoch(
        &self,
        runtime_epoch: u64,
    ) -> Option<DesktopSnapshot> {
        self.mark_runtime_error_if_epoch(runtime_epoch, "codex_incompatible")
            .await
    }

    pub(crate) async fn runtime_is_current(&self, runtime_epoch: u64) -> bool {
        let coordinator = self.coordinator.lock().await;
        coordinator.runtime_epoch == runtime_epoch
            && coordinator.pending_runtime_epoch.is_none()
            && matches!(coordinator.snapshot.codex, CodexSnapshot::Ready { .. })
    }

    pub async fn install_client(&self, client: Arc<dyn AppServerClient>) -> u64 {
        let mut coordinator = self.coordinator.lock().await;
        let was_ready = coordinator.arcgis_mcp.is_ready();
        coordinator.client = Some(client);
        coordinator.runtime_epoch = coordinator.runtime_epoch.wrapping_add(1);
        coordinator.pending_runtime_epoch = None;
        coordinator.active.conversation_starting = false;
        coordinator.arcgis_mcp = ArcGisMcpReadiness::default();
        if was_ready {
            coordinator.ready_epoch = coordinator.ready_epoch.wrapping_add(1);
        }
        coordinator.runtime_epoch
    }

    pub async fn app_server_client(&self) -> Option<Arc<dyn AppServerClient>> {
        self.coordinator.lock().await.client.clone()
    }

    pub(crate) async fn take_runtime(&self) -> Option<RuntimeOwnership> {
        let runtime = self
            .runtime
            .write()
            .await
            .take()
            .map(RuntimeOwnership::from);
        self.mark_runtime_stopped().await;
        runtime
    }

    pub async fn set_active_thread(&self, thread_id: String) {
        let mut coordinator = self.coordinator.lock().await;
        coordinator.active = ActiveConversation {
            thread_id: Some(thread_id),
            ..ActiveConversation::default()
        };
        coordinator.arcgis_mcp = ArcGisMcpReadiness::default();
        coordinator.ready_epoch = coordinator.ready_epoch.wrapping_add(1);
    }

    pub async fn set_active_turn(&self, turn_id: String) {
        self.coordinator.lock().await.active.turn_id = Some(turn_id);
    }

    pub async fn clear_active_turn(&self) {
        let mut coordinator = self.coordinator.lock().await;
        coordinator.active.turn_id = None;
        coordinator.active.turn_starting = false;
        coordinator.active.turn_interrupting = false;
    }

    pub async fn active_ids(&self) -> (Option<String>, Option<String>) {
        let coordinator = self.coordinator.lock().await;
        let active = &coordinator.active;
        (active.thread_id.clone(), active.turn_id.clone())
    }

    pub async fn clear_conversation(&self) {
        let mut coordinator = self.coordinator.lock().await;
        coordinator.generation = coordinator.generation.wrapping_add(1);
        coordinator.snapshot.session_generation = coordinator.generation;
        coordinator.active = ActiveConversation::default();
        coordinator.arcgis_mcp = ArcGisMcpReadiness::default();
        coordinator.ready_epoch = coordinator.ready_epoch.wrapping_add(1);
    }

    #[doc(hidden)]
    pub async fn set_arcgis_ready_for(&self, thread_id: Option<&str>, ready: bool) -> bool {
        if ready {
            return false;
        }
        let mut coordinator = self.coordinator.lock().await;
        if thread_id.is_some_and(|id| coordinator.active.thread_id.as_deref() != Some(id)) {
            return false;
        }
        let before = coordinator.arcgis_mcp.is_ready();
        if coordinator.arcgis_mcp == ArcGisMcpReadiness::default() {
            return false;
        }
        coordinator.arcgis_mcp = ArcGisMcpReadiness::default();
        if before {
            coordinator.ready_epoch = coordinator.ready_epoch.wrapping_add(1);
        }
        true
    }

    pub async fn arcgis_ready(&self) -> bool {
        self.coordinator.lock().await.arcgis_mcp.is_ready()
    }

    pub async fn apply_arcgis_status(
        &self,
        runtime_epoch: u64,
        update: ArcGisStatusUpdate,
    ) -> bool {
        let mut coordinator = self.coordinator.lock().await;
        if coordinator.runtime_epoch != runtime_epoch {
            return false;
        }
        match update.thread_id.as_deref() {
            Some(thread_id)
                if coordinator.active.thread_id.as_deref() == Some(thread_id)
                    && !thread_id.is_empty() => {}
            None if update.lifecycle != Lifecycle::Ready => {}
            Some(_) | None => return false,
        }
        let before = coordinator.arcgis_mcp.is_ready();
        if !coordinator.arcgis_mcp.apply_status(update) {
            return false;
        }
        let after = coordinator.arcgis_mcp.is_ready();
        if before != after {
            coordinator.ready_epoch = coordinator.ready_epoch.wrapping_add(1);
        }
        true
    }

    pub async fn apply_arcgis_inventory(&self, lease: &McpDiscoveryLease, valid: bool) -> bool {
        let mut coordinator = self.coordinator.lock().await;
        if coordinator.runtime_epoch != lease.runtime_epoch
            || coordinator.generation != lease.generation
            || coordinator.active.thread_id.as_deref() != Some(&lease.thread_id)
            || lease.thread_id.is_empty()
        {
            return false;
        }
        let before = coordinator.arcgis_mcp.is_ready();
        if !coordinator.arcgis_mcp.apply_inventory(valid) {
            return false;
        }
        let after = coordinator.arcgis_mcp.is_ready();
        if before != after {
            coordinator.ready_epoch = coordinator.ready_epoch.wrapping_add(1);
        }
        true
    }

    pub async fn begin_turn(&self) -> Result<(TurnLease, Arc<dyn AppServerClient>), String> {
        let mut coordinator = self.coordinator.lock().await;
        if !matches!(
            coordinator.snapshot.account,
            AccountSnapshot::SignedIn { .. }
        ) {
            return Err("Please sign in with ChatGPT first".to_owned());
        }
        let thread_id = coordinator
            .active
            .thread_id
            .clone()
            .ok_or_else(|| "No active conversation".to_owned())?;
        if coordinator.active.turn_id.is_some() || coordinator.active.turn_starting {
            return Err("A turn is already in progress".to_owned());
        }
        let client = coordinator
            .client
            .clone()
            .ok_or_else(|| "Codex App Server is not ready".to_owned())?;
        coordinator.active.turn_starting = true;
        Ok((
            TurnLease {
                generation: coordinator.generation,
                thread_id,
            },
            client,
        ))
    }

    pub async fn commit_turn(&self, lease: &TurnLease, turn_id: String) -> Result<(), String> {
        let mut coordinator = self.coordinator.lock().await;
        let matches = coordinator.generation == lease.generation
            && coordinator.active.thread_id.as_deref() == Some(&lease.thread_id)
            && coordinator.active.turn_starting
            && matches!(
                coordinator.snapshot.account,
                AccountSnapshot::SignedIn { .. }
            );
        if !matches {
            return Err("Conversation changed while starting the turn".to_owned());
        }
        coordinator.active.turn_starting = false;
        coordinator.active.turn_id = Some(turn_id);
        Ok(())
    }

    pub async fn abort_turn(&self, lease: &TurnLease) {
        let mut coordinator = self.coordinator.lock().await;
        if coordinator.generation == lease.generation
            && coordinator.active.thread_id.as_deref() == Some(&lease.thread_id)
        {
            coordinator.active.turn_starting = false;
        }
    }

    pub async fn complete_turn_if_matching(&self, thread_id: &str, turn_id: &str) -> bool {
        let mut coordinator = self.coordinator.lock().await;
        if coordinator.active.thread_id.as_deref() != Some(thread_id)
            || coordinator.active.turn_id.as_deref() != Some(turn_id)
        {
            return false;
        }
        coordinator.active.turn_id = None;
        coordinator.active.turn_interrupting = false;
        true
    }

    pub async fn begin_interrupt(
        &self,
    ) -> Result<(InterruptLease, Arc<dyn AppServerClient>), String> {
        let mut coordinator = self.coordinator.lock().await;
        if !matches!(
            coordinator.snapshot.account,
            AccountSnapshot::SignedIn { .. }
        ) {
            return Err("Please sign in with ChatGPT first".to_owned());
        }
        let thread_id = coordinator
            .active
            .thread_id
            .clone()
            .ok_or_else(|| "No active conversation".to_owned())?;
        let turn_id = coordinator
            .active
            .turn_id
            .clone()
            .ok_or_else(|| "No turn is running".to_owned())?;
        if coordinator.active.turn_interrupting {
            return Err("The turn is already being interrupted".to_owned());
        }
        let client = coordinator
            .client
            .clone()
            .ok_or_else(|| "Codex App Server is not ready".to_owned())?;
        coordinator.active.turn_interrupting = true;
        Ok((
            InterruptLease {
                generation: coordinator.generation,
                thread_id,
                turn_id,
            },
            client,
        ))
    }

    pub async fn finish_interrupt(&self, lease: &InterruptLease, succeeded: bool) -> bool {
        let mut coordinator = self.coordinator.lock().await;
        if coordinator.generation != lease.generation
            || coordinator.active.thread_id.as_deref() != Some(&lease.thread_id)
            || coordinator.active.turn_id.as_deref() != Some(&lease.turn_id)
        {
            return false;
        }
        coordinator.active.turn_interrupting = false;
        if succeeded {
            coordinator.active.turn_id = None;
        }
        true
    }

    pub async fn begin_conversation(
        &self,
    ) -> Result<(ConversationLease, Arc<dyn AppServerClient>), String> {
        let mut coordinator = self.coordinator.lock().await;
        if !matches!(
            coordinator.snapshot.account,
            AccountSnapshot::SignedIn { .. }
        ) {
            return Err("Please sign in with ChatGPT first".to_owned());
        }
        if coordinator.active.conversation_starting {
            return Err("A conversation is already starting".to_owned());
        }
        let client = coordinator
            .client
            .clone()
            .ok_or_else(|| "Codex App Server is not ready".to_owned())?;
        coordinator.active.conversation_starting = true;
        Ok((
            ConversationLease {
                generation: coordinator.generation,
                runtime_epoch: coordinator.runtime_epoch,
            },
            client,
        ))
    }

    pub async fn commit_conversation(
        &self,
        lease: &ConversationLease,
        thread_id: String,
    ) -> Result<McpDiscoveryLease, String> {
        let mut coordinator = self.coordinator.lock().await;
        if coordinator.generation != lease.generation
            || coordinator.runtime_epoch != lease.runtime_epoch
            || !coordinator.active.conversation_starting
            || !matches!(
                coordinator.snapshot.account,
                AccountSnapshot::SignedIn { .. }
            )
        {
            return Err("Account changed while starting the conversation".to_owned());
        }
        coordinator.active = ActiveConversation {
            thread_id: Some(thread_id.clone()),
            ..ActiveConversation::default()
        };
        coordinator.arcgis_mcp = ArcGisMcpReadiness::default();
        coordinator.ready_epoch = coordinator.ready_epoch.wrapping_add(1);
        Ok(McpDiscoveryLease {
            generation: coordinator.generation,
            runtime_epoch: coordinator.runtime_epoch,
            thread_id,
        })
    }

    pub async fn abort_conversation(&self, lease: &ConversationLease) {
        let mut coordinator = self.coordinator.lock().await;
        if coordinator.generation == lease.generation
            && coordinator.runtime_epoch == lease.runtime_epoch
        {
            coordinator.active.conversation_starting = false;
        }
    }

    pub async fn set_visible(&self, visible: bool) {
        let mut coordinator = self.coordinator.lock().await;
        if coordinator.visible != visible {
            coordinator.visible = visible;
            coordinator.visibility_epoch = coordinator.visibility_epoch.wrapping_add(1);
        }
    }

    pub async fn health_lease(&self) -> Option<(HealthLease, Arc<dyn AppServerClient>)> {
        let coordinator = self.coordinator.lock().await;
        if !coordinator.visible
            || !coordinator.arcgis_mcp.is_ready()
            || !matches!(
                coordinator.snapshot.account,
                AccountSnapshot::SignedIn { .. }
            )
        {
            return None;
        }
        let thread_id = coordinator.active.thread_id.clone()?;
        let client = coordinator.client.clone()?;
        Some((
            HealthLease {
                generation: coordinator.generation,
                runtime_epoch: coordinator.runtime_epoch,
                ready_epoch: coordinator.ready_epoch,
                visibility_epoch: coordinator.visibility_epoch,
                thread_id,
            },
            client,
        ))
    }

    pub async fn commit_health(&self, lease: &HealthLease, health: BridgeSnapshot) -> bool {
        let mut coordinator = self.coordinator.lock().await;
        let current = health_lease_is_current(&coordinator, lease);
        if current {
            coordinator.snapshot.arcgis = health;
        }
        current
    }

    pub async fn health_lease_is_current(&self, lease: &HealthLease) -> bool {
        let coordinator = self.coordinator.lock().await;
        health_lease_is_current(&coordinator, lease)
    }

    pub async fn mark_runtime_stopped(&self) -> DesktopSnapshot {
        let mut coordinator = self.coordinator.lock().await;
        coordinator.generation = coordinator.generation.wrapping_add(1);
        coordinator.snapshot.session_generation = coordinator.generation;
        coordinator.client = None;
        coordinator.runtime_epoch = coordinator.runtime_epoch.wrapping_add(1);
        coordinator.pending_runtime_epoch = None;
        coordinator.arcgis_mcp = ArcGisMcpReadiness::default();
        coordinator.ready_epoch = coordinator.ready_epoch.wrapping_add(1);
        coordinator.active = ActiveConversation::default();
        coordinator.snapshot.clone()
    }

    pub fn poll_gate(&self) -> &PollGate {
        &self.poll_gate
    }
}

fn apply_account_locked(coordinator: &mut Coordinator, account: AccountSnapshot) {
    let changed_signed_in_account = matches!(
        (&coordinator.snapshot.account, &account),
        (
            AccountSnapshot::SignedIn { .. },
            AccountSnapshot::SignedIn { .. }
        )
    ) && coordinator.snapshot.account != account;
    if !matches!(account, AccountSnapshot::SignedIn { .. }) || changed_signed_in_account {
        coordinator.generation = coordinator.generation.wrapping_add(1);
        coordinator.snapshot.session_generation = coordinator.generation;
        coordinator.active = ActiveConversation::default();
        coordinator.arcgis_mcp = ArcGisMcpReadiness::default();
        coordinator.ready_epoch = coordinator.ready_epoch.wrapping_add(1);
        coordinator.snapshot.arcgis =
            retain_stale_snapshot(&coordinator.snapshot.arcgis, "account session changed");
    }
    coordinator.snapshot.account = account;
}

fn health_lease_is_current(coordinator: &Coordinator, lease: &HealthLease) -> bool {
    coordinator.generation == lease.generation
        && coordinator.runtime_epoch == lease.runtime_epoch
        && coordinator.ready_epoch == lease.ready_epoch
        && coordinator.visibility_epoch == lease.visibility_epoch
        && coordinator.active.thread_id.as_deref() == Some(&lease.thread_id)
        && coordinator.visible
        && coordinator.arcgis_mcp.is_ready()
        && coordinator.client.is_some()
        && matches!(
            coordinator.snapshot.account,
            AccountSnapshot::SignedIn { .. }
        )
}

impl PollGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn should_poll(&self, visible: bool, arcgis_ready: bool, thread_id: Option<&str>) -> bool {
        !self.cancelled.load(Ordering::Acquire)
            && visible
            && arcgis_ready
            && thread_id.is_some_and(|id| !id.is_empty())
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

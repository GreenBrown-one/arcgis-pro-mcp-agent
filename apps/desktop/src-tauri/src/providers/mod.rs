use serde::{Deserialize, Serialize};

pub mod codex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    Codex,
    DeepSeek,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ProviderAuthSnapshot {
    Checking,
    NeedsSetup,
    LoginPending {
        #[serde(rename = "loginId")]
        login_id: String,
    },
    Ready {
        label: Option<String>,
        plan: Option<String>,
    },
    Error {
        code: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ProviderRuntimeSnapshot {
    Stopped,
    Starting,
    Ready {
        version: Option<String>,
        #[serde(rename = "versionVerified")]
        version_verified: bool,
    },
    Error {
        code: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSnapshot {
    pub kind: ProviderKind,
    pub auth: ProviderAuthSnapshot,
    pub runtime: ProviderRuntimeSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ProviderEvent {
    TextDelta {
        #[serde(rename = "itemId")]
        item_id: String,
        text: String,
    },
    ToolCompleted {
        item: serde_json::Value,
    },
    TurnCompleted {
        #[serde(rename = "turnId")]
        turn_id: String,
    },
}

pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

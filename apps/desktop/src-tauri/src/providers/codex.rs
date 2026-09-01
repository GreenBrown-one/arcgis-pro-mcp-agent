use crate::app_state::{AccountSnapshot, CodexSnapshot};

use super::{ProviderAuthSnapshot, ProviderKind, ProviderRuntimeSnapshot, ProviderSnapshot};

pub fn snapshot(account: &AccountSnapshot, runtime: &CodexSnapshot) -> ProviderSnapshot {
    ProviderSnapshot {
        kind: ProviderKind::Codex,
        auth: match account {
            AccountSnapshot::Checking => ProviderAuthSnapshot::Checking,
            AccountSnapshot::SignedOut => ProviderAuthSnapshot::NeedsSetup,
            AccountSnapshot::LoginPending { login_id } => ProviderAuthSnapshot::LoginPending {
                login_id: login_id.clone(),
            },
            AccountSnapshot::UnsupportedAuth => ProviderAuthSnapshot::Error {
                code: "unsupportedAuth".to_owned(),
            },
            AccountSnapshot::SignedIn { email, plan_type } => ProviderAuthSnapshot::Ready {
                label: email.clone(),
                plan: Some(plan_type.clone()),
            },
        },
        runtime: match runtime {
            CodexSnapshot::Starting => ProviderRuntimeSnapshot::Starting,
            CodexSnapshot::Ready {
                version,
                version_verified,
            } => ProviderRuntimeSnapshot::Ready {
                version: Some(version.clone()),
                version_verified: *version_verified,
            },
            CodexSnapshot::Error { code } => ProviderRuntimeSnapshot::Error { code: code.clone() },
        },
    }
}

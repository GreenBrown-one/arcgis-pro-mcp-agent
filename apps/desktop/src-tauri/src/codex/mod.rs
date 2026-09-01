mod client;
pub mod discovery;
mod protocol;

pub(crate) use client::CodexStartFailure;
pub use client::{CodexError, CodexRuntime, CodexStartOptions, build_codex_command};
pub use discovery::{
    CODEX_INSTALL_URL, CodexDiscoveryError, CodexInstallation, CodexVersionConfidence,
    CodexVersionProbe, ProcessCodexVersionProbe, TESTED_CODEX_VERSION, codex_candidates,
    discover_codex, discover_codex_with,
};
pub use protocol::CodexEvent;

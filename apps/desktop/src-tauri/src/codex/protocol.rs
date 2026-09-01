use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
pub(crate) struct WireRequest<'a> {
    pub method: &'a str,
    pub id: u64,
    pub params: Value,
}

#[derive(Serialize)]
pub(crate) struct WireNotification<'a> {
    pub method: &'a str,
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CodexEvent {
    Notification {
        method: String,
        params: Value,
    },
    ServerRequest {
        id: Value,
        method: String,
        params: Value,
    },
    ProtocolError {
        message: String,
    },
    ProcessExited {
        code: Option<i32>,
    },
}

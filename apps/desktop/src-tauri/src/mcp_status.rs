use serde_json::{Value, json};

pub const CURRENT_STATUS_METHOD: &str = "mcpServer/startupStatus/updated";
pub const LEGACY_STATUS_METHOD: &str = "mcpServer/status/updated";
const REQUIRED_TOOLS: [&str; 3] = [
    "arcgis_connection_status",
    "arcgis_describe_context",
    "arcgis_list_layers",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusSource {
    Current,
    Legacy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    Starting,
    Ready,
    Failed,
    Cancelled,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCategory {
    StartupFailed,
    ReauthenticationRequired,
    Cancelled,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArcGisStatusUpdate {
    pub source: StatusSource,
    pub thread_id: Option<String>,
    pub lifecycle: Lifecycle,
    pub failure: Option<FailureCategory>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArcGisMcpReadiness {
    lifecycle: Option<Lifecycle>,
    inventory_valid: bool,
    current_seen: bool,
    failure: Option<FailureCategory>,
}

impl ArcGisMcpReadiness {
    pub fn apply_status(&mut self, update: ArcGisStatusUpdate) -> bool {
        if self.current_seen && update.source == StatusSource::Legacy {
            return false;
        }
        let before = self.clone();
        if update.source == StatusSource::Current {
            self.current_seen = true;
        }
        self.lifecycle = Some(update.lifecycle);
        self.failure = update.failure;
        *self != before
    }

    pub fn apply_inventory(&mut self, valid: bool) -> bool {
        let changed = self.inventory_valid != valid;
        self.inventory_valid = valid;
        changed
    }

    pub fn is_ready(&self) -> bool {
        self.inventory_valid && self.lifecycle == Some(Lifecycle::Ready)
    }

    pub fn failure(&self) -> Option<FailureCategory> {
        self.failure
    }
}

pub fn parse_arcgis_status_notification(
    method: &str,
    params: &Value,
) -> Option<ArcGisStatusUpdate> {
    let source = match method {
        CURRENT_STATUS_METHOD => StatusSource::Current,
        LEGACY_STATUS_METHOD => StatusSource::Legacy,
        _ => return None,
    };
    if params.get("name").and_then(Value::as_str) != Some("arcgis") {
        return None;
    }
    let lifecycle = match params.get("status").and_then(Value::as_str) {
        Some("starting") => Lifecycle::Starting,
        Some("ready") => Lifecycle::Ready,
        Some("failed") => Lifecycle::Failed,
        Some("cancelled") => Lifecycle::Cancelled,
        _ => Lifecycle::Unknown,
    };
    let failure = match lifecycle {
        Lifecycle::Failed
            if source == StatusSource::Current
                && params.get("failureReason").and_then(Value::as_str)
                    == Some("reauthenticationRequired") =>
        {
            Some(FailureCategory::ReauthenticationRequired)
        }
        Lifecycle::Failed => Some(FailureCategory::StartupFailed),
        Lifecycle::Cancelled => Some(FailureCategory::Cancelled),
        Lifecycle::Unknown => Some(FailureCategory::Unknown),
        Lifecycle::Starting | Lifecycle::Ready => None,
    };
    Some(ArcGisStatusUpdate {
        source,
        thread_id: match params.get("threadId") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
            _ => {
                return Some(ArcGisStatusUpdate {
                    source,
                    thread_id: None,
                    lifecycle: Lifecycle::Unknown,
                    failure: Some(FailureCategory::Unknown),
                });
            }
        },
        lifecycle,
        failure,
    })
}

pub fn mcp_status_list_params(thread_id: &str) -> Value {
    json!({"threadId": thread_id, "detail": "toolsAndAuthOnly"})
}

pub fn arcgis_inventory_is_valid(response: &Value) -> bool {
    response
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|server| server.get("name").and_then(Value::as_str) == Some("arcgis"))
        .and_then(|server| server.get("tools").and_then(Value::as_object))
        .is_some_and(|tools| REQUIRED_TOOLS.iter().all(|name| tools.contains_key(*name)))
}

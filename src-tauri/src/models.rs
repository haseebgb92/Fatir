use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRef {
    pub label: String,
    pub path: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceItem {
    pub title: String,
    pub detail: String,
    pub status: String,
    #[serde(default)]
    pub resources: Vec<ResourceRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingAction {
    pub id: String,
    pub tool: String,
    pub arguments: Value,
    pub risk: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub text: String,
    pub model: String,
    pub trace: Vec<TraceItem>,
    pub pending: Option<PendingAction>,
}

#[derive(Debug, Clone)]
pub struct StoredPending {
    pub id: String,
    pub session_id: String,
    pub tool: String,
    pub arguments: Value,
    pub risk: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LargestFile {
    pub path: String,
    pub bytes: u64,
    pub human: String,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub hostname: String,
    pub os: String,
    pub kernel: String,
    pub cpu: String,
    pub cpu_usage_percent: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub root_used: String,
    pub root_available: String,
}

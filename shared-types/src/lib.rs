use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_usage: f32,
    pub memory_bytes: u64,
    pub disk_read_bytes: u64,
    pub disk_written_bytes: u64,
    pub user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemMetricPayload {
    pub device_id: String,
    pub timestamp_sec: u64,
    pub cpu_usage_pct: f32,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub network_rx_bytes_sec: u64,
    pub network_tx_bytes_sec: u64,
    pub cpu_temperature_c: f32,
    pub disk_usage_pct: f32,
    pub disk_read_bytes_sec: u64,
    pub disk_written_bytes_sec: u64,
    pub uptime_sec: u64,
    pub running_processes: u32,
    pub processes: Vec<ProcessInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchMetricRequest {
    pub metrics: Vec<SystemMetricPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlertRule {
    pub id: String,
    pub user_id: String,
    pub device_id: Option<String>,
    pub metric_type: String, // "cpu", "memory", "disk", "temperature"
    pub threshold_value: f32,
    pub cooldown_seconds: u32,
    pub notify_email: bool,
    pub notify_browser: bool,
    pub last_triggered: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAlertRuleRequest {
    pub device_id: Option<String>,
    pub metric_type: String,
    pub threshold_value: f32,
    pub cooldown_seconds: Option<u32>,
    pub notify_email: Option<bool>,
    pub notify_browser: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlertEvent {
    pub id: String,
    pub user_id: String,
    pub device_id: String,
    pub device_name: String,
    pub metric_type: String,
    pub metric_value: f32,
    pub threshold_value: f32,
    pub message: String,
    pub created_at: String,
    pub read_at: Option<String>,
}

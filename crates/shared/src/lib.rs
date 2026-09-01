use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

// =============================================================================
// 1. DTOs & Models for Auth
// =============================================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDTO {
    pub id: String,
    pub email: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserDTO,
}

// =============================================================================
// 2. DTOs & Models for DB Pools
// =============================================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbPoolDTO {
    pub id: Option<String>,
    pub name: String,
    pub engine: String, // 'surrealdb' | 'postgres' | 'mysql'
    pub connection_url: String,
    pub auth_namespace: Option<String>,
    pub auth_database: Option<String>,
    pub auth_username: Option<String>,
    pub auth_password: Option<String>,
    pub max_connections: i64,
    pub tags: Vec<String>,
    pub is_active: bool,
    pub created_at: Option<DateTime<Utc>>,
}

// =============================================================================
// 3. DTOs & Models for Microservices
// =============================================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicroserviceDTO {
    pub id: Option<String>,
    pub uuid: Option<String>,
    pub name: String,
    pub language: String, // 'rust'
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub active_version_id: Option<String>,
    pub active_version_tag: Option<String>,
    pub on_success_action: Option<String>,
    pub on_success_config: Option<String>,
    pub on_error_action: Option<String>,
    pub on_error_config: Option<String>,
    pub is_active: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicroserviceVersionDTO {
    pub id: Option<String>,
    pub microservice_id: String,
    pub version_number: i64,
    pub version_tag: String, // "v1.0.0"
    pub source_type: String, // 'textarea' | 'git' | 'upload'
    pub source_code: String,
    pub container_image_tag: Option<String>,
    pub container_id: Option<String>,
    pub status: String, // 'draft' | 'building' | 'running' | 'stopped' | 'failed'
    pub changelog: Option<String>,
    pub error_message: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

// =============================================================================
// 4. DTOs & Models for Queues
// =============================================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueDTO {
    pub id: Option<String>,
    pub stream_key: String,
    pub name: Option<String>,
    pub consumer_group: String,
    pub is_active: bool,
    pub tags: Vec<String>,
    pub created_at: Option<DateTime<Utc>>,
}

// =============================================================================
// 5. DTOs & Models for Bindings
// =============================================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingDTO {
    pub id: Option<String>,
    pub queue_id: String,
    pub microservice_id: String,
    pub queue: Option<QueueDTO>,
    pub microservice: Option<MicroserviceDTO>,
    pub target_version_id: Option<String>,
    pub on_success_action: String, // 'ack' | 'forward' | 'store_db'
    pub on_success_config: serde_json::Value,
    pub on_error_action: String,   // 'dlq' | 'retry' | 'alert'
    pub on_error_config: serde_json::Value,
    pub is_active: bool,
}

// =============================================================================
// 6. DTOs & Models for System Health & Metrics
// =============================================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    pub hostname: String,
    pub cpu_usage_total: f32,
    pub cpu_cores: Vec<f32>,
    pub memory_total_kb: u64,
    pub memory_used_kb: u64,
    pub memory_free_kb: u64,
    pub swap_total_kb: u64,
    pub swap_used_kb: u64,
    pub disk_total_bytes: u64,
    pub disk_free_bytes: u64,
    pub disk_read_bytes_sec: u64,
    pub disk_write_bytes_sec: u64,
    pub uptime_seconds: u64,
    pub load_average: (f64, f64, f64),
    pub network_rx_bytes_sec: u64,
    pub network_tx_bytes_sec: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerMetric {
    pub id: String,
    pub name: String,
    pub cpu_usage_percent: f64,
    pub memory_usage_bytes: u64,
    pub memory_limit_bytes: u64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealthResponse {
    pub host: SystemHealth,
    pub containers: Vec<ContainerMetric>,
}

// =============================================================================
// 7. Advanced Log Filtering DTOs
// =============================================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogFilterQuery {
    pub microservice_id: Option<String>,
    pub queue_id: Option<String>,
    pub status: Option<String>,          // "success" | "error" | "timeout" | "panic"
    pub tags: Option<Vec<String>>,        // Filtro por tags do serviço ou fila
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub min_duration_ms: Option<i64>,
    pub max_duration_ms: Option<i64>,
    pub search_term: Option<String>,     // Busca textual dentro do erro ou payload
    pub page: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionLogDTO {
    pub id: Option<String>,
    pub queue_id: String,
    pub microservice_id: String,
    pub version_id: Option<String>,
    pub stream_message_id: String,
    pub payload_input: serde_json::Value,
    pub payload_output: Option<serde_json::Value>,
    pub status: String, // 'success' | 'error' | 'timeout' | 'panic'
    pub error_message: Option<String>,
    pub execution_time_ms: i64,
    pub tags: Vec<String>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSearchResponse {
    pub logs: Vec<ExecutionLogDTO>,
    pub total: u64,
    pub page: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicroserviceEnvDTO {
    pub id: Option<String>,
    pub microservice_id: Option<String>,
    pub name: String,
    pub config: serde_json::Value,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyDTO {
    pub id: Option<String>,
    pub name: String,
    pub key_value: Option<String>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

// =============================================================================
// 8. DTOs & Models for Scheduled Executions
// =============================================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleJobRequest {
    pub run_at: Option<DateTime<Utc>>,
    pub delay_seconds: Option<i64>,
    pub cron_expression: Option<String>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJobDTO {
    pub id: Option<String>,
    pub microservice_id: String,
    pub payload: serde_json::Value,
    pub run_at: DateTime<Utc>,
    pub status: String, // 'pending' | 'completed' | 'failed'
    pub cron_expression: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

// =============================================================================
// 9. DTOs & Models for Event Publishing
// =============================================================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishEventRequest {
    pub stream_key: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishEventResponse {
    pub success: bool,
    pub stream_key: String,
    pub message_id: Option<String>,
}


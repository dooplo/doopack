mod db;
mod auth;
mod sys_monitor;
mod docker_manager;
mod redis_worker;
mod scheduler;

use std::sync::Arc;
use axum::{
    routing::{get, post, delete, put},
    Router,
    Json,
    http::StatusCode,
    response::IntoResponse,
};
use tower_http::cors::CorsLayer;
use deadpool_redis::{Config, Runtime};
use shared::{
    LoginRequest, LoginResponse, UserDTO, DbPoolDTO, MicroserviceDTO,
    MicroserviceVersionDTO, QueueDTO, BindingDTO, LogFilterQuery, ExecutionLogDTO,
    LogSearchResponse, SystemHealthResponse
};
use db::DbProxy;
use sys_monitor::SysMonitor;
use docker_manager::DockerManager;
use redis_worker::RedisWorkerManager;
use serde_json::json;
use axum::extract::Path;
use sqlx::Row;
use redis::AsyncCommands;
use chrono::Utc;

#[derive(Clone)]
struct AppState {
    db_proxy: DbProxy,
    sys_monitor: Arc<SysMonitor>,
    docker_manager: DockerManager,
    redis_pool: deadpool_redis::Pool,
}

impl axum::extract::FromRef<AppState> for sqlx::SqlitePool {
    fn from_ref(state: &AppState) -> Self {
        state.db_proxy.pool.clone()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    println!("Starting Distributed Event Orchestrator server (SQLite + SQLx)...");

    // 1. Initialize Redis Pool
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://:redisroot@127.0.0.1:6381".to_string());
    let cfg = Config::from_url(redis_url);
    let redis_pool = cfg.create_pool(Some(Runtime::Tokio1))?;

    // 2. Initialize Database Proxy (SQLite)
    let db_proxy = DbProxy::new().await?;

    // 3. Initialize Docker Manager
    let docker_manager = DockerManager::new()?;

    // 4. Initialize Sys Monitor
    let sys_monitor = Arc::new(SysMonitor::new());

    // 5. Build Shared State
    let state = AppState {
        db_proxy: db_proxy.clone(),
        sys_monitor,
        docker_manager: docker_manager.clone(),
        redis_pool: redis_pool.clone(),
    };

    // 6. Start Redis consumer background thread
    let worker_manager = Arc::new(RedisWorkerManager::new(redis_pool, db_proxy, docker_manager));
    worker_manager.start_worker_loop();

    // 6.5. Start Scheduler background thread
    let scheduler_manager = Arc::new(scheduler::SchedulerManager::new(state.db_proxy.clone(), state.docker_manager.clone()));
    scheduler_manager.start_scheduler_loop();

    // 7. Setup Axum router
    let app = Router::new()
        // Auth public routes
        .route("/api/v1/auth/register", post(register_handler))
        .route("/api/v1/auth/login", post(login_handler))
        // API Keys CRUD (authenticated)
        .route("/api/v1/auth/keys", get(list_api_keys).post(create_api_key))
        .route("/api/v1/auth/keys/{id}", delete(delete_api_key))
        // System Metrics & Monitoring
        .route("/api/v1/system/health", get(system_health_handler))
        .route("/api/v1/system/export", get(export_system_data))
        .route("/api/v1/system/import", post(import_system_data))
        // Log Filtering & Search
        .route("/api/v1/logs/search", post(search_logs_handler))
        .route("/api/v1/logs/{id}", get(get_log_by_id_handler).delete(delete_log_handler))
        .route("/api/v1/logs/{id}/resend", post(resend_log_handler))
        // Event Publishing
        .route("/api/v1/events/publish", post(publish_event_handler))
        .route("/api/v1/queues/{stream_key}/publish", post(publish_to_queue_handler))
        // DB Pools CRUD
        .route("/api/v1/pools", get(list_pools).post(create_pool))
        .route("/api/v1/pools/test", post(test_pool_connection_payload))
        .route("/api/v1/pools/{id}", delete(delete_pool).put(update_pool))
        .route("/api/v1/pools/{id}/test", post(test_pool_connection))
        // Microservices CRUD
        .route("/api/v1/services", get(list_services).post(create_service))
        .route("/api/v1/services/{id}", delete(delete_service).put(update_service))
        .route("/api/v1/services/{id}/envs", get(list_envs).post(create_env))
        .route("/api/v1/services/{id}/envs/{env_id}", get(get_env_by_id).put(edit_env).delete(delete_env_by_id))
        .route("/api/v1/services/{id}/envs/{env_id}/edit", post(edit_env))
        .route("/api/v1/services/{id}/build-logs", get(get_build_logs))
        .route("/api/v1/services/{id}/schedule", post(schedule_job_handler))
        // Versions
        .route("/api/v1/services/{id}/versions", get(list_versions).post(create_version))
        .route("/api/v1/services/{id}/rollback", post(rollback_version))
        .route("/api/v1/versions/{version_id}/test", post(test_version))
        .route("/api/v1/versions/{version_id}/status", get(get_version_container_status))
        // Queues CRUD
        .route("/api/v1/queues", get(list_queues).post(create_queue))
        .route("/api/v1/queues/{id}", delete(delete_queue))
        // Bindings CRUD
        .route("/api/v1/bindings", get(list_bindings).post(create_binding))
        // Scheduler CRUD
        .route("/api/v1/schedules", get(list_schedules))
        .route("/api/v1/schedules/{id}", delete(delete_schedule_handler))
        .route("/api/v1/bindings/{id}", delete(delete_binding))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:4500").await?;
    println!("Server running on http://0.0.0.0:4500");
    axum::serve(listener, app).await?;

    Ok(())
}

// =============================================================================
// HTTP Request Handlers
// =============================================================================

async fn register_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
    let password_hash = match auth::hash_password(&payload.password) {
        Ok(h) => h,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response(),
    };

    let check_query = "SELECT id FROM users WHERE email = ?";
    let check_res = sqlx::query(check_query)
        .bind(&payload.email)
        .fetch_optional(&state.db_proxy.pool)
        .await;

    if let Ok(Some(_)) = check_res {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "User already exists" }))).into_response();
    }

    let insert_query = "INSERT INTO users (email, password_hash) VALUES (?, ?)";
    let res = sqlx::query(insert_query)
        .bind(&payload.email)
        .bind(password_hash)
        .execute(&state.db_proxy.pool)
        .await;

    match res {
        Ok(r) => {
            let id = r.last_insert_rowid();
            (StatusCode::CREATED, Json(json!({ "id": id.to_string(), "email": payload.email }))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn login_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
    let query = "SELECT id, email, password_hash, created_at FROM users WHERE email = ? LIMIT 1";
    let row = match sqlx::query(query).bind(&payload.email).fetch_optional(&state.db_proxy.pool).await {
        Ok(Some(r)) => r,
        Ok(None) => return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "Invalid credentials" }))).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    };

    let hash: String = row.get("password_hash");
    if !auth::verify_password(&hash, &payload.password) {
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "Invalid credentials" }))).into_response();
    }

    let id: i64 = row.get("id");
    let token = match auth::generate_token(&id.to_string()) {
        Ok(t) => t,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response(),
    };

    let user_dto = UserDTO {
        id: id.to_string(),
        email: row.get("email"),
        created_at: chrono::Utc::now(),
    };

    (StatusCode::OK, Json(LoginResponse { token, user: user_dto })).into_response()
}

async fn system_health_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> impl IntoResponse {
    let host = state.sys_monitor.get_system_health().await;
    let containers = state.sys_monitor.get_container_metrics().await;
    Json(SystemHealthResponse { host, containers })
}

async fn export_system_data(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> impl IntoResponse {
    let pools_rows = sqlx::query("SELECT id, name, engine, connection_url, auth_namespace, auth_database, auth_username, auth_password, max_connections, tags, is_active, created_at FROM db_pools")
        .fetch_all(&state.db_proxy.pool)
        .await
        .unwrap_or_default();
    let mut pools = Vec::new();
    for r in pools_rows {
        let tags_raw: Option<String> = r.try_get("tags").ok();
        let tags: Vec<String> = tags_raw.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        pools.push(shared::DbPoolDTO {
            id: Some(r.get::<i64, _>("id").to_string()),
            name: r.get("name"),
            engine: r.get("engine"),
            connection_url: r.get("connection_url"),
            auth_namespace: r.get("auth_namespace"),
            auth_database: r.get("auth_database"),
            auth_username: r.get("auth_username"),
            auth_password: r.get("auth_password"),
            max_connections: r.get("max_connections"),
            tags,
            is_active: r.get("is_active"),
            created_at: r.try_get("created_at").ok(),
        });
    }

    let queues_rows = sqlx::query("SELECT id, stream_key, consumer_group, is_active, tags, created_at FROM queues")
        .fetch_all(&state.db_proxy.pool)
        .await
        .unwrap_or_default();
    let mut queues = Vec::new();
    for r in queues_rows {
        let tags_raw: Option<String> = r.try_get("tags").ok();
        let tags: Vec<String> = tags_raw.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        queues.push(shared::QueueDTO {
            id: Some(r.get::<i64, _>("id").to_string()),
            stream_key: r.get("stream_key"),
            name: Some(r.get("stream_key")),
            consumer_group: r.get("consumer_group"),
            is_active: r.get("is_active"),
            tags,
            created_at: r.try_get("created_at").ok(),
        });
    }

    let services_rows = sqlx::query("SELECT m.id, m.uuid, m.name, m.description, m.language, m.tags, m.on_success_action, m.on_success_config, m.on_error_action, m.on_error_config, m.active_version_id, mv.version_tag AS active_version_tag, m.is_active, m.created_at, m.updated_at FROM microservices m LEFT JOIN microservice_versions mv ON m.active_version_id = mv.id")
        .fetch_all(&state.db_proxy.pool)
        .await
        .unwrap_or_default();
    let mut services = Vec::new();
    for r in services_rows {
        let tags_raw: Option<String> = r.try_get("tags").ok();
        let tags: Vec<String> = tags_raw.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        services.push(shared::MicroserviceDTO {
            id: Some(r.get::<i64, _>("id").to_string()),
            uuid: r.try_get("uuid").ok(),
            name: r.get("name"),
            description: r.get("description"),
            language: r.get("language"),
            tags,
            on_success_action: r.get("on_success_action"),
            on_success_config: r.get("on_success_config"),
            on_error_action: r.get("on_error_action"),
            on_error_config: r.get("on_error_config"),
            active_version_id: r.get::<Option<i64>, _>("active_version_id").map(|v| v.to_string()),
            active_version_tag: r.try_get("active_version_tag").ok(),
            is_active: r.get("is_active"),
            created_at: r.try_get("created_at").ok(),
            updated_at: r.try_get("updated_at").ok(),
        });
    }

    let versions_rows = sqlx::query("SELECT id, microservice_id, version_number, version_tag, source_type, source_code, container_image_tag, container_id, status, changelog, error_message, created_at FROM microservice_versions")
        .fetch_all(&state.db_proxy.pool)
        .await
        .unwrap_or_default();
    let mut versions = Vec::new();
    for r in versions_rows {
        versions.push(shared::MicroserviceVersionDTO {
            id: Some(r.get::<i64, _>("id").to_string()),
            microservice_id: r.get::<i64, _>("microservice_id").to_string(),
            version_number: r.get("version_number"),
            version_tag: r.get("version_tag"),
            source_type: r.get("source_type"),
            source_code: r.get("source_code"),
            container_image_tag: r.get("container_image_tag"),
            container_id: r.get("container_id"),
            status: r.get("status"),
            changelog: r.get("changelog"),
            error_message: r.get("error_message"),
            created_at: r.try_get("created_at").ok(),
        });
    }

    let bindings_rows = sqlx::query("SELECT id, queue_id, microservice_id, target_version_id, on_success_action, on_success_config, on_error_action, on_error_config, is_active FROM bindings")
        .fetch_all(&state.db_proxy.pool)
        .await
        .unwrap_or_default();
    let mut bindings = Vec::new();
    for r in bindings_rows {
        let sc_str: Option<String> = r.try_get("on_success_config").ok();
        let ec_str: Option<String> = r.try_get("on_error_config").ok();
        let on_success_config = sc_str.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(serde_json::Value::Null);
        let on_error_config = ec_str.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(serde_json::Value::Null);

        bindings.push(shared::BindingDTO {
            id: Some(r.get::<i64, _>("id").to_string()),
            queue_id: r.get::<i64, _>("queue_id").to_string(),
            microservice_id: r.get::<i64, _>("microservice_id").to_string(),
            queue: None,
            microservice: None,
            target_version_id: r.get::<Option<i64>, _>("target_version_id").map(|v| v.to_string()),
            on_success_action: r.get("on_success_action"),
            on_success_config,
            on_error_action: r.get("on_error_action"),
            on_error_config,
            is_active: r.get("is_active"),
        });
    }

    let envs_rows = sqlx::query("SELECT id, microservice_id, name, config, is_default FROM microservice_envs")
        .fetch_all(&state.db_proxy.pool)
        .await
        .unwrap_or_default();
    let mut envs = Vec::new();
    for r in envs_rows {
        let config_str: String = r.get("config");
        let config = serde_json::from_str(&config_str).unwrap_or(serde_json::Value::Null);
        envs.push(shared::MicroserviceEnvDTO {
            id: Some(r.get::<i64, _>("id").to_string()),
            microservice_id: Some(r.get::<i64, _>("microservice_id").to_string()),
            name: r.get("name"),
            config,
            is_default: r.get("is_default"),
        });
    }

    let schedules_rows = sqlx::query("SELECT id, microservice_id, payload, run_at, status, cron_expression, created_at FROM scheduled_jobs")
        .fetch_all(&state.db_proxy.pool)
        .await
        .unwrap_or_default();
    let mut schedules = Vec::new();
    for r in schedules_rows {
        let payload_str: String = r.get("payload");
        let payload = serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);
        schedules.push(shared::ScheduledJobDTO {
            id: Some(r.get::<i64, _>("id").to_string()),
            microservice_id: r.get::<i64, _>("microservice_id").to_string(),
            payload,
            run_at: r.get("run_at"),
            status: r.get("status"),
            cron_expression: r.get("cron_expression"),
            created_at: r.try_get("created_at").ok(),
        });
    }

    (StatusCode::OK, Json(json!({
        "version": "1.0",
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "pools": pools,
        "queues": queues,
        "services": services,
        "versions": versions,
        "bindings": bindings,
        "envs": envs,
        "schedules": schedules
    }))).into_response()
}

#[derive(serde::Deserialize)]
struct ImportDataRequest {
    #[serde(default)]
    pools: Vec<shared::DbPoolDTO>,
    #[serde(default)]
    services: Vec<shared::MicroserviceDTO>,
    #[serde(default)]
    versions: Vec<shared::MicroserviceVersionDTO>,
    #[serde(default)]
    queues: Vec<shared::QueueDTO>,
    #[serde(default)]
    bindings: Vec<shared::BindingDTO>,
    #[serde(default)]
    envs: Vec<shared::MicroserviceEnvDTO>,
    #[serde(default)]
    schedules: Vec<shared::ScheduledJobDTO>,
}

async fn import_system_data(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(payload): Json<ImportDataRequest>,
) -> impl IntoResponse {
    let mut tx = match state.db_proxy.pool.begin().await {
        Ok(t) => t,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    };

    // 1. Pools
    for p in payload.pools {
        let query = "INSERT OR REPLACE INTO db_pools (id, name, engine, connection_url, auth_namespace, auth_database, auth_username, auth_password, max_connections, tags, is_active) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
        let id_val = p.id.and_then(|id| id.parse::<i64>().ok());
        let tags_str = serde_json::to_string(&p.tags).unwrap_or_else(|_| "[]".to_string());
        if let Err(e) = sqlx::query(query)
            .bind(id_val)
            .bind(p.name)
            .bind(p.engine)
            .bind(p.connection_url)
            .bind(p.auth_namespace)
            .bind(p.auth_database)
            .bind(p.auth_username)
            .bind(p.auth_password)
            .bind(p.max_connections)
            .bind(tags_str)
            .bind(p.is_active)
            .execute(&mut *tx)
            .await
        {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("Failed to import db pool: {}", e) }))).into_response();
        }
    }

    // 2. Queues
    for q in payload.queues {
        let query = "INSERT OR REPLACE INTO queues (id, stream_key, consumer_group, is_active, tags) VALUES (?, ?, ?, ?, ?)";
        let id_val = q.id.and_then(|id| id.parse::<i64>().ok());
        let tags_str = serde_json::to_string(&q.tags).unwrap_or_else(|_| "[]".to_string());
        if let Err(e) = sqlx::query(query)
            .bind(id_val)
            .bind(q.stream_key)
            .bind(q.consumer_group)
            .bind(q.is_active)
            .bind(tags_str)
            .execute(&mut *tx)
            .await
        {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("Failed to import queue: {}", e) }))).into_response();
        }
    }

    // 3. Microservices
    for s in payload.services {
        let query = "INSERT OR REPLACE INTO microservices (id, uuid, name, description, language, tags, on_success_action, on_success_config, on_error_action, on_error_config, active_version_id, is_active) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
        let id_val = s.id.and_then(|id| id.parse::<i64>().ok());
        let active_ver_id = s.active_version_id.and_then(|id| id.parse::<i64>().ok());
        let tags_str = serde_json::to_string(&s.tags).unwrap_or_else(|_| "[]".to_string());
        if let Err(e) = sqlx::query(query)
            .bind(id_val)
            .bind(s.uuid)
            .bind(s.name)
            .bind(s.description)
            .bind(s.language)
            .bind(tags_str)
            .bind(s.on_success_action)
            .bind(s.on_success_config)
            .bind(s.on_error_action)
            .bind(s.on_error_config)
            .bind(active_ver_id)
            .bind(s.is_active)
            .execute(&mut *tx)
            .await
        {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("Failed to import microservice: {}", e) }))).into_response();
        }
    }

    // 4. Microservice Versions
    for v in payload.versions {
        let query = "INSERT OR REPLACE INTO microservice_versions (id, microservice_id, version_number, version_tag, source_type, source_code, container_image_tag, container_id, status, changelog, error_message) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
        let id_val = v.id.and_then(|id| id.parse::<i64>().ok());
        let ms_id = v.microservice_id.parse::<i64>().unwrap_or(0);
        if let Err(e) = sqlx::query(query)
            .bind(id_val)
            .bind(ms_id)
            .bind(v.version_number)
            .bind(v.version_tag)
            .bind(v.source_type)
            .bind(v.source_code)
            .bind(v.container_image_tag)
            .bind(v.container_id)
            .bind(v.status)
            .bind(v.changelog)
            .bind(v.error_message)
            .execute(&mut *tx)
            .await
        {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("Failed to import version: {}", e) }))).into_response();
        }
    }

    // 5. Bindings
    for b in payload.bindings {
        let query = "INSERT OR REPLACE INTO bindings (id, queue_id, microservice_id, target_version_id, on_success_action, on_success_config, on_error_action, on_error_config, is_active) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)";
        let id_val = b.id.and_then(|id| id.parse::<i64>().ok());
        let q_id = b.queue_id.parse::<i64>().unwrap_or(0);
        let ms_id = b.microservice_id.parse::<i64>().unwrap_or(0);
        let tv_id = b.target_version_id.and_then(|v| v.parse::<i64>().ok());
        let sc_str = serde_json::to_string(&b.on_success_config).unwrap_or_else(|_| "{}".to_string());
        let ec_str = serde_json::to_string(&b.on_error_config).unwrap_or_else(|_| "{}".to_string());

        if let Err(e) = sqlx::query(query)
            .bind(id_val)
            .bind(q_id)
            .bind(ms_id)
            .bind(tv_id)
            .bind(b.on_success_action)
            .bind(sc_str)
            .bind(b.on_error_action)
            .bind(ec_str)
            .bind(b.is_active)
            .execute(&mut *tx)
            .await
        {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("Failed to import binding: {}", e) }))).into_response();
        }
    }

    // 6. Environments
    for env in payload.envs {
        let query = "INSERT OR REPLACE INTO microservice_envs (id, microservice_id, name, config, is_default) VALUES (?, ?, ?, ?, ?)";
        let id_val = env.id.and_then(|id| id.parse::<i64>().ok());
        let ms_id = env.microservice_id.and_then(|id| id.parse::<i64>().ok()).unwrap_or(0);
        let config_str = serde_json::to_string(&env.config).unwrap_or_else(|_| "{}".to_string());
        if let Err(e) = sqlx::query(query)
            .bind(id_val)
            .bind(ms_id)
            .bind(env.name)
            .bind(config_str)
            .bind(env.is_default)
            .execute(&mut *tx)
            .await
        {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("Failed to import environment: {}", e) }))).into_response();
        }
    }

    // 7. Scheduled Jobs
    for s in payload.schedules {
        let query = "INSERT OR REPLACE INTO scheduled_jobs (id, microservice_id, payload, run_at, status, cron_expression) VALUES (?, ?, ?, ?, ?, ?)";
        let id_val = s.id.and_then(|id| id.parse::<i64>().ok());
        let ms_id = s.microservice_id.parse::<i64>().unwrap_or(0);
        let payload_str = serde_json::to_string(&s.payload).unwrap_or_else(|_| "{}".to_string());
        if let Err(e) = sqlx::query(query)
            .bind(id_val)
            .bind(ms_id)
            .bind(payload_str)
            .bind(s.run_at)
            .bind(s.status)
            .bind(s.cron_expression)
            .execute(&mut *tx)
            .await
        {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("Failed to import scheduled job: {}", e) }))).into_response();
        }
    }

    if let Err(e) = tx.commit().await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response();
    }

    StatusCode::OK.into_response()
}

async fn search_logs_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(query): Json<LogFilterQuery>,
) -> impl IntoResponse {
    let mut conditions = vec![];
    let mut args = vec![];

    if let Some(microservice_id) = &query.microservice_id {
        conditions.push("microservice_id = ?");
        args.push(microservice_id.clone());
    }
    if let Some(queue_id) = &query.queue_id {
        conditions.push("queue_id = ?");
        args.push(queue_id.clone());
    }
    if let Some(status) = &query.status {
        conditions.push("status = ?");
        args.push(status.clone());
    }
    if let Some(tags) = &query.tags {
        for tag in tags {
            let t = tag.trim();
            if !t.is_empty() {
                conditions.push("tags LIKE ?");
                args.push(format!("%{}%", t));
            }
        }
    }
    if let Some(start_date) = &query.start_date {
        conditions.push("datetime(created_at) >= datetime(?)");
        args.push(start_date.format("%Y-%m-%d %H:%M:%S").to_string());
    }
    if let Some(end_date) = &query.end_date {
        conditions.push("datetime(created_at) <= datetime(?)");
        args.push(end_date.format("%Y-%m-%d %H:%M:%S").to_string());
    }
    if let Some(min_duration) = query.min_duration_ms {
        conditions.push("execution_time_ms >= ?");
        args.push(min_duration.to_string());
    }
    if let Some(max_duration) = query.max_duration_ms {
        conditions.push("execution_time_ms <= ?");
        args.push(max_duration.to_string());
    }
    if let Some(search_term) = &query.search_term {
        if !search_term.is_empty() {
            conditions.push("(error_message LIKE ? OR payload_input LIKE ?)");
            args.push(format!("%{}%", search_term));
            args.push(format!("%{}%", search_term));
        }
    }

    let where_clause = if conditions.is_empty() {
        "".to_string()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let start = (query.page - 1) * query.limit;
    let main_query_str = format!(
        "SELECT * FROM execution_logs {} ORDER BY created_at DESC LIMIT ? OFFSET ?",
        where_clause
    );
    let count_query_str = format!("SELECT COUNT(*) FROM execution_logs {}", where_clause);

    // Build sqlx execution
    let mut sqlx_query = sqlx::query(&main_query_str);
    for arg in &args {
        sqlx_query = sqlx_query.bind(arg);
    }
    // Bind limit and offset
    sqlx_query = sqlx_query.bind(query.limit as i64).bind(start as i64);

    let rows_res = sqlx_query.fetch_all(&state.db_proxy.pool).await;
    let rows = match rows_res {
        Ok(r) => r,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    };

    let mut count_query = sqlx::query(&count_query_str);
    for arg in &args {
        count_query = count_query.bind(arg);
    }
    let total: i64 = match count_query.fetch_one(&state.db_proxy.pool).await {
        Ok(r) => r.get(0),
        Err(_) => 0,
    };

    let mut logs = Vec::new();
    for row in rows {
        let id: i64 = row.get("id");
        let queue_id: i64 = row.get("queue_id");
        let microservice_id: i64 = row.get("microservice_id");
        let version_id: Option<i64> = row.get("version_id");
        
        let payload_input_str: String = row.get("payload_input");
        let payload_input: serde_json::Value = serde_json::from_str(&payload_input_str).unwrap_or(serde_json::Value::Null);
        
        let payload_output_str: Option<String> = row.get("payload_output");
        let payload_output = payload_output_str.and_then(|s| serde_json::from_str(&s).ok());

        let tags_str: String = row.get("tags");
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();

        logs.push(ExecutionLogDTO {
            id: Some(id.to_string()),
            queue_id: queue_id.to_string(),
            microservice_id: microservice_id.to_string(),
            version_id: version_id.map(|v| v.to_string()),
            stream_message_id: row.get("stream_message_id"),
            payload_input,
            payload_output,
            status: row.get("status"),
            error_message: row.get("error_message"),
            execution_time_ms: row.get("execution_time_ms"),
            tags,
            created_at: row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok(),
        });
    }

    Json(LogSearchResponse {
        logs,
        total: total as u64,
        page: query.page,
        limit: query.limit,
    }).into_response()
}

async fn delete_log_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(log_id): Path<String>,
) -> impl IntoResponse {
    let id = match log_id.parse::<i64>() {
        Ok(val) => val,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid log ID" }))).into_response(),
    };

    match sqlx::query("DELETE FROM execution_logs WHERE id = ?")
        .bind(id)
        .execute(&state.db_proxy.pool)
        .await
    {
        Ok(_) => (StatusCode::OK, Json(json!({ "success": true }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn resend_log_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(log_id): Path<String>,
) -> impl IntoResponse {
    let id = match log_id.parse::<i64>() {
        Ok(val) => val,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid log ID" }))).into_response(),
    };

    let log_row = match sqlx::query("SELECT queue_id, payload_input FROM execution_logs WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db_proxy.pool)
        .await
    {
        Ok(Some(r)) => r,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(json!({ "error": "Log not found" }))).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    };

    let queue_id: i64 = log_row.get("queue_id");
    let payload_input_str: String = log_row.get("payload_input");

    let queue_row = match sqlx::query("SELECT stream_key FROM queues WHERE id = ?")
        .bind(queue_id)
        .fetch_optional(&state.db_proxy.pool)
        .await
    {
        Ok(Some(r)) => r,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(json!({ "error": "Queue not found" }))).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    };

    let stream_key: String = queue_row.get("stream_key");

    let mut conn = match state.redis_pool.get().await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Redis connection failed: {}", e) }))).into_response(),
    };

    let payload_val: serde_json::Value = serde_json::from_str(&payload_input_str).unwrap_or(serde_json::Value::Null);
    let out_str = serde_json::to_string(&payload_val).unwrap_or_default();
    
    let mut fields = vec![("payload".to_string(), out_str)];
    if let Some(obj) = payload_val.as_object() {
        for (k, v) in obj {
            let val_str = if v.is_string() {
                v.as_str().unwrap().to_string()
            } else {
                v.to_string()
            };
            fields.push((k.clone(), val_str));
        }
    }
    let fields_ref: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

    let res: redis::RedisResult<()> = conn.xadd(&stream_key, "*", &fields_ref).await;
    match res {
        Ok(_) => (StatusCode::OK, Json(json!({ "success": true }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Failed to publish to stream: {}", e) }))).into_response(),
    }
}

async fn get_log_by_id_handler(
    _user: auth::AuthenticatedUser,
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(log_id): Path<String>,
) -> impl IntoResponse {
    let id = match log_id.parse::<i64>() {
        Ok(val) => val,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid log ID" }))).into_response(),
    };

    let row = match sqlx::query("SELECT * FROM execution_logs WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.db_proxy.pool)
        .await
    {
        Ok(Some(r)) => r,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(json!({ "error": "Log not found" }))).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    };

    let queue_id: i64 = row.get("queue_id");
    let microservice_id: i64 = row.get("microservice_id");
    let version_id: Option<i64> = row.get("version_id");
    
    let payload_input_str: String = row.get("payload_input");
    let payload_input: serde_json::Value = serde_json::from_str(&payload_input_str).unwrap_or(serde_json::Value::Null);
    
    let payload_output_str: Option<String> = row.get("payload_output");
    let payload_output = payload_output_str.and_then(|s| serde_json::from_str(&s).ok());

    let tags_str: String = row.get("tags");
    let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();

    let log = shared::ExecutionLogDTO {
        id: Some(id.to_string()),
        queue_id: queue_id.to_string(),
        microservice_id: microservice_id.to_string(),
        version_id: version_id.map(|v| v.to_string()),
        stream_message_id: row.get("stream_message_id"),
        payload_input,
        payload_output,
        status: row.get("status"),
        error_message: row.get("error_message"),
        execution_time_ms: row.get("execution_time_ms"),
        tags,
        created_at: row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok(),
    };

    (StatusCode::OK, Json(log)).into_response()
}

async fn publish_event_handler(
    _user: auth::AuthenticatedUser,
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(payload): Json<shared::PublishEventRequest>,
) -> impl IntoResponse {
    let stream_key = payload.stream_key.trim();
    if stream_key.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "stream_key cannot be empty" }))).into_response();
    }

    let mut conn = match state.redis_pool.get().await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Redis connection failed: {}", e) }))).into_response(),
    };

    let payload_val = payload.payload;
    let out_str = serde_json::to_string(&payload_val).unwrap_or_default();
    
    let mut fields = vec![("payload".to_string(), out_str)];
    if let Some(obj) = payload_val.as_object() {
        for (k, v) in obj {
            let val_str = if v.is_string() {
                v.as_str().unwrap().to_string()
            } else {
                v.to_string()
            };
            fields.push((k.clone(), val_str));
        }
    }
    let fields_ref: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

    let res: redis::RedisResult<String> = conn.xadd(stream_key, "*", &fields_ref).await;
    match res {
        Ok(msg_id) => (StatusCode::OK, Json(shared::PublishEventResponse {
            success: true,
            stream_key: stream_key.to_string(),
            message_id: Some(msg_id),
        })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Failed to publish to stream: {}", e) }))).into_response(),
    }
}

async fn publish_to_queue_handler(
    user: auth::AuthenticatedUser,
    state: axum::extract::State<AppState>,
    Path(stream_key): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    publish_event_handler(user, state, Json(shared::PublishEventRequest {
        stream_key,
        payload,
    })).await
}


// --- DB Pools CRUD ---

async fn list_pools(axum::extract::State(state): axum::extract::State<AppState>) -> impl IntoResponse {
    let rows = sqlx::query("SELECT * FROM db_pools")
        .fetch_all(&state.db_proxy.pool)
        .await
        .unwrap_or_default();

    let mut pools = Vec::new();
    for row in rows {
        let id: i64 = row.get("id");
        let tags_str: String = row.get("tags");
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();

        pools.push(DbPoolDTO {
            id: Some(id.to_string()),
            name: row.get("name"),
            engine: row.get("engine"),
            connection_url: row.get("connection_url"),
            auth_namespace: row.get("auth_namespace"),
            auth_database: row.get("auth_database"),
            auth_username: row.get("auth_username"),
            auth_password: row.get("auth_password"),
            max_connections: row.get("max_connections"),
            tags,
            is_active: row.get("is_active"),
            created_at: Some(chrono::Utc::now()),
        });
    }
    Json(pools)
}

async fn create_pool(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(payload): Json<DbPoolDTO>,
) -> impl IntoResponse {
    let tags_str = serde_json::to_string(&payload.tags).unwrap_or_else(|_| "[]".to_string());
    let query = r#"
        INSERT INTO db_pools (
            name, engine, connection_url, auth_namespace, auth_database, 
            auth_username, auth_password, max_connections, tags, is_active
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    "#;
    let res = sqlx::query(query)
        .bind(payload.name.clone())
        .bind(payload.engine.clone())
        .bind(payload.connection_url.clone())
        .bind(payload.auth_namespace.clone())
        .bind(payload.auth_database.clone())
        .bind(payload.auth_username.clone())
        .bind(payload.auth_password.clone())
        .bind(payload.max_connections)
        .bind(tags_str)
        .bind(payload.is_active)
        .execute(&state.db_proxy.pool)
        .await;

    match res {
        Ok(r) => {
            let id = r.last_insert_rowid();
            let mut created = payload;
            created.id = Some(id.to_string());
            (StatusCode::CREATED, Json(created)).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn delete_pool(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Ok(id_int) = id.parse::<i64>() {
        let _ = sqlx::query("DELETE FROM db_pools WHERE id = ?")
            .bind(id_int)
            .execute(&state.db_proxy.pool)
            .await;
    }
    StatusCode::NO_CONTENT
}

async fn update_pool(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<DbPoolDTO>,
) -> impl IntoResponse {
    let id_int = match id.parse::<i64>() {
        Ok(i) => i,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid ID" }))).into_response(),
    };

    let tags_str = serde_json::to_string(&payload.tags).unwrap_or_else(|_| "[]".to_string());
    let query = r#"
        UPDATE db_pools SET 
            name = ?, engine = ?, connection_url = ?, auth_namespace = ?, auth_database = ?, 
            auth_username = ?, auth_password = ?, max_connections = ?, tags = ?, is_active = ?
        WHERE id = ?
    "#;
    let res = sqlx::query(query)
        .bind(payload.name.clone())
        .bind(payload.engine.clone())
        .bind(payload.connection_url.clone())
        .bind(payload.auth_namespace.clone())
        .bind(payload.auth_database.clone())
        .bind(payload.auth_username.clone())
        .bind(payload.auth_password.clone())
        .bind(payload.max_connections)
        .bind(tags_str)
        .bind(payload.is_active)
        .bind(id_int)
        .execute(&state.db_proxy.pool)
        .await;

    match res {
        Ok(_) => {
            (StatusCode::OK, Json(payload)).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

fn parse_host_port(url_str: &str) -> Option<(String, u16)> {
    let cleaned = url_str
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_start_matches("ws://")
        .trim_start_matches("wss://");
    
    let parts: Vec<&str> = cleaned.split('/').next()?.split(':').collect();
    if parts.is_empty() {
        return None;
    }
    let mut host = parts[0].to_string();
    if host == "host.docker.internal" {
        host = "127.0.0.1".to_string();
    }
    let port = if parts.len() > 1 {
        parts[1].parse::<u16>().ok()?
    } else if url_str.starts_with("https://") || url_str.starts_with("wss://") {
        443
    } else {
        8000
    };
    Some((host, port))
}

async fn test_pool_connection(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let id_int = match id.parse::<i64>() {
        Ok(i) => i,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "status": "error", "message": "Invalid ID" }))).into_response(),
    };

    let pool_res = sqlx::query("SELECT connection_url FROM db_pools WHERE id = ?")
        .bind(id_int)
        .fetch_optional(&state.db_proxy.pool)
        .await;

    let connection_url = match pool_res {
        Ok(Some(row)) => row.get::<String, _>("connection_url"),
        _ => return (StatusCode::NOT_FOUND, Json(json!({ "status": "error", "message": "Connection pool not found" }))).into_response(),
    };

    let (host, port) = match parse_host_port(&connection_url) {
        Some(hp) => hp,
        None => return (StatusCode::BAD_REQUEST, Json(json!({ "status": "error", "message": "Failed to parse host/port from URL" }))).into_response(),
    };

    let addr = format!("{}:{}", host, port);
    let connect_fut = tokio::net::TcpStream::connect(&addr);
    match tokio::time::timeout(std::time::Duration::from_secs(10), connect_fut).await {
        Ok(Ok(_stream)) => {
            (StatusCode::OK, Json(json!({ "status": "success", "message": format!("Successfully connected to {}", addr) }))).into_response()
        }
        Ok(Err(e)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "status": "error", "message": format!("Connection failed: {}", e) }))).into_response()
        }
        Err(_) => {
            (StatusCode::GATEWAY_TIMEOUT, Json(json!({ "status": "error", "message": "Connection timed out (3s)" }))).into_response()
        }
    }
}

#[derive(serde::Deserialize)]
struct TestPoolConnectionPayloadRequest {
    connection_url: String,
}

async fn test_pool_connection_payload(
    Json(req): Json<TestPoolConnectionPayloadRequest>,
) -> impl IntoResponse {
    let (host, port) = match parse_host_port(&req.connection_url) {
        Some(hp) => hp,
        None => return (StatusCode::BAD_REQUEST, Json(json!({ "status": "error", "message": "Failed to parse host/port from URL" }))).into_response(),
    };

    let addr = format!("{}:{}", host, port);
    let connect_fut = tokio::net::TcpStream::connect(&addr);
    match tokio::time::timeout(std::time::Duration::from_secs(3), connect_fut).await {
        Ok(Ok(_stream)) => {
            (StatusCode::OK, Json(json!({ "status": "success", "message": format!("Successfully connected to {}", addr) }))).into_response()
        }
        Ok(Err(e)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "status": "error", "message": format!("Connection failed: {}", e) }))).into_response()
        }
        Err(_) => {
            (StatusCode::GATEWAY_TIMEOUT, Json(json!({ "status": "error", "message": "Connection timed out (3s)" }))).into_response()
        }
    }
}

// --- Microservices CRUD ---

async fn list_services(axum::extract::State(state): axum::extract::State<AppState>) -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT m.id, m.uuid, m.name, m.language, m.description, m.tags, m.active_version_id, m.on_success_action, m.on_success_config, m.on_error_action, m.on_error_config, m.is_active, mv.version_tag AS active_version_tag 
         FROM microservices m 
         LEFT JOIN microservice_versions mv ON m.active_version_id = mv.id"
     )
    .fetch_all(&state.db_proxy.pool)
    .await
    .unwrap_or_default();

    let mut services = Vec::new();
    for row in rows {
        let id: i64 = row.get("id");
        let active_version_id: Option<i64> = row.get("active_version_id");
        let active_version_tag: Option<String> = row.get("active_version_tag");
        let tags_str: String = row.get("tags");
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();

        let on_success_action: Option<String> = row.get("on_success_action");
        let on_success_config: Option<String> = row.get("on_success_config");
        let on_error_action: Option<String> = row.get("on_error_action");
        let on_error_config: Option<String> = row.get("on_error_config");

        services.push(MicroserviceDTO {
            id: Some(id.to_string()),
            uuid: row.get("uuid"),
            name: row.get("name"),
            language: row.get("language"),
            description: row.get("description"),
            tags,
            active_version_id: active_version_id.map(|v| v.to_string()),
            active_version_tag,
            on_success_action,
            on_success_config,
            on_error_action,
            on_error_config,
            is_active: row.get("is_active"),
            created_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
        });
    }
    Json(services)
}

async fn create_service(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(payload): Json<MicroserviceDTO>,
) -> impl IntoResponse {
    let tags_str = serde_json::to_string(&payload.tags).unwrap_or_else(|_| "[]".to_string());
    
    let generated_uuid = {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let bytes: [u8; 16] = rng.gen();
        format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5],
            bytes[6], bytes[7],
            bytes[8], bytes[9],
            bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
        )
    };

    let query = r#"
        INSERT INTO microservices (
            uuid, name, language, description, tags, active_version_id,
            on_success_action, on_success_config, on_error_action, on_error_config, is_active
        ) VALUES (?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?)
    "#;
    let res = sqlx::query(query)
        .bind(generated_uuid.clone())
        .bind(payload.name.clone())
        .bind(payload.language.clone())
        .bind(payload.description.clone())
        .bind(tags_str)
        .bind(payload.on_success_action.clone().unwrap_or_else(|| "end".to_string()))
        .bind(payload.on_success_config.clone())
        .bind(payload.on_error_action.clone().unwrap_or_else(|| "end".to_string()))
        .bind(payload.on_error_config.clone())
        .bind(payload.is_active)
        .execute(&state.db_proxy.pool)
        .await;

    match res {
        Ok(r) => {
            let id = r.last_insert_rowid();
            let mut created = payload;
            created.id = Some(id.to_string());
            created.uuid = Some(generated_uuid);
            (StatusCode::CREATED, Json(created)).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn update_service(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<MicroserviceDTO>,
) -> impl IntoResponse {
    let id_int = match id.parse::<i64>() {
        Ok(i) => i,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid ID" }))).into_response(),
    };
    let tags_str = serde_json::to_string(&payload.tags).unwrap_or_else(|_| "[]".to_string());
    let query = r#"
        UPDATE microservices 
        SET name = ?, description = ?, tags = ?, 
            on_success_action = ?, on_success_config = ?, 
            on_error_action = ?, on_error_config = ?,
            is_active = ?,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = ?
    "#;
    let res = sqlx::query(query)
        .bind(payload.name.clone())
        .bind(payload.description.clone())
        .bind(tags_str)
        .bind(payload.on_success_action.clone().unwrap_or_else(|| "end".to_string()))
        .bind(payload.on_success_config.clone())
        .bind(payload.on_error_action.clone().unwrap_or_else(|| "end".to_string()))
        .bind(payload.on_error_config.clone())
        .bind(payload.is_active)
        .bind(id_int)
        .execute(&state.db_proxy.pool)
        .await;

    let query_uuid: Option<String> = sqlx::query("SELECT uuid FROM microservices WHERE id = ?")
        .bind(id_int)
        .fetch_optional(&state.db_proxy.pool)
        .await
        .ok()
        .flatten()
        .and_then(|r| r.get("uuid"));

    match res {
        Ok(_) => {
            let mut updated = payload;
            updated.uuid = query_uuid;
            (StatusCode::OK, Json(updated)).into_response()
        }
        Err(e) => {
            eprintln!("❌ [update_service] DB error: {}", e);
            (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response()
        }
    }
}

async fn delete_service(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Ok(id_int) = id.parse::<i64>() {
        let log_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM execution_logs WHERE microservice_id = ?")
            .bind(id_int)
            .fetch_one(&state.db_proxy.pool)
            .await
            .unwrap_or((0,));
        if log_count.0 > 0 {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Cannot delete microservice because it has execution logs." }))).into_response();
        }

        let _ = sqlx::query("DELETE FROM microservices WHERE id = ?")
            .bind(id_int)
            .execute(&state.db_proxy.pool)
            .await;
    }
    StatusCode::NO_CONTENT.into_response()
}

// --- Environments ---

async fn resolve_ms_id(pool: &sqlx::SqlitePool, id_or_uuid: &str) -> Result<i64, String> {
    if let Ok(id) = id_or_uuid.parse::<i64>() {
        return Ok(id);
    }
    let row = sqlx::query("SELECT id FROM microservices WHERE uuid = ? LIMIT 1")
        .bind(id_or_uuid)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;

    match row {
        Some(r) => Ok(r.get::<i64, _>("id")),
        None => Err("Microservice not found by ID or UUID".to_string()),
    }
}

async fn list_envs(
    _user: auth::AuthenticatedUser,
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let ms_id = match resolve_ms_id(&state.db_proxy.pool, &id).await {
        Ok(val) => val,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    };

    let rows = match sqlx::query("SELECT id, microservice_id, name, config, is_default FROM microservice_envs WHERE microservice_id = ? ORDER BY name ASC")
        .bind(ms_id)
        .fetch_all(&state.db_proxy.pool)
        .await
    {
        Ok(r) => r,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    };

    let mut list = Vec::new();
    for r in rows {
        let config_str: String = r.get("config");
        let config: serde_json::Value = serde_json::from_str(&config_str).unwrap_or(json!({}));
        list.push(shared::MicroserviceEnvDTO {
            id: Some(r.get::<i64, _>("id").to_string()),
            microservice_id: Some(r.get::<i64, _>("microservice_id").to_string()),
            name: r.get("name"),
            config,
            is_default: r.get::<bool, _>("is_default"),
        });
    }

    (StatusCode::OK, Json(list)).into_response()
}

async fn create_env(
    _user: auth::AuthenticatedUser,
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<shared::MicroserviceEnvDTO>,
) -> impl IntoResponse {
    let ms_id = match resolve_ms_id(&state.db_proxy.pool, &id).await {
        Ok(val) => val,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    };

    let config_str = serde_json::to_string(&payload.config).unwrap_or_else(|_| "{}".to_string());

    let mut tx = match state.db_proxy.pool.begin().await {
        Ok(t) => t,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    };

    if payload.is_default {
        let _ = sqlx::query("UPDATE microservice_envs SET is_default = 0 WHERE microservice_id = ?")
            .bind(ms_id)
            .execute(&mut *tx)
            .await;
    }

    let query = "INSERT OR REPLACE INTO microservice_envs (microservice_id, name, config, is_default) VALUES (?, ?, ?, ?)";
    let res = sqlx::query(query)
        .bind(ms_id)
        .bind(&payload.name)
        .bind(&config_str)
        .bind(payload.is_default)
        .execute(&mut *tx)
        .await;

    match res {
        Ok(r) => {
            if let Err(e) = tx.commit().await {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response()
            } else {
                let insert_id = r.last_insert_rowid();
                (StatusCode::CREATED, Json(shared::MicroserviceEnvDTO {
                    id: Some(insert_id.to_string()),
                    microservice_id: Some(ms_id.to_string()),
                    name: payload.name,
                    config: payload.config,
                    is_default: payload.is_default,
                })).into_response()
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

fn parse_cron_and_next(cron_expr: &str) -> Result<chrono::DateTime<chrono::Utc>, String> {
    use std::str::FromStr;
    use cron::Schedule;
    let mut clean_expr = cron_expr.trim().to_string();
    let fields_count = clean_expr.split_whitespace().count();
    if fields_count == 5 {
        clean_expr = format!("0 {}", clean_expr);
    }
    let schedule = Schedule::from_str(&clean_expr).map_err(|e| e.to_string())?;
    let next = schedule.upcoming(Utc).next().ok_or_else(|| "No upcoming execution time found".to_string())?;
    Ok(next)
}

async fn schedule_job_handler(
    _user: auth::AuthenticatedUser,
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<shared::ScheduleJobRequest>,
) -> impl IntoResponse {
    let ms_id = match resolve_ms_id(&state.db_proxy.pool, &id).await {
        Ok(val) => val,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    };

    let (run_at, cron_expr_val) = if let Some(ref cron_str) = req.cron_expression {
        if cron_str.trim().is_empty() {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Empty cron expression" }))).into_response();
        }
        match parse_cron_and_next(cron_str) {
            Ok(next_time) => (next_time, Some(cron_str.clone())),
            Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("Invalid cron expression: {}", e) }))).into_response(),
        }
    } else {
        let computed_run_at = match (req.run_at, req.delay_seconds) {
            (Some(time), _) => time,
            (None, Some(delay)) => {
                let dur = match chrono::Duration::try_seconds(delay) {
                    Some(d) => d,
                    None => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid delay value" }))).into_response(),
                };
                Utc::now() + dur
            }
            (None, None) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Must specify either cron_expression, run_at or delay_seconds" }))).into_response(),
        };
        (computed_run_at, None)
    };

    let payload_str = serde_json::to_string(&req.payload).unwrap_or_else(|_| "{}".to_string());

    let query = "INSERT INTO scheduled_jobs (microservice_id, payload, run_at, status, cron_expression) VALUES (?, ?, ?, 'pending', ?)";
    let res = sqlx::query(query)
        .bind(ms_id)
        .bind(&payload_str)
        .bind(run_at)
        .bind(&cron_expr_val)
        .execute(&state.db_proxy.pool)
        .await;

    match res {
        Ok(r) => {
            let insert_id = r.last_insert_rowid();
            (StatusCode::CREATED, Json(shared::ScheduledJobDTO {
                id: Some(insert_id.to_string()),
                microservice_id: ms_id.to_string(),
                payload: req.payload,
                run_at,
                status: "pending".to_string(),
                cron_expression: cron_expr_val,
                created_at: Some(Utc::now()),
            })).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn list_schedules(
    _user: auth::AuthenticatedUser,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> impl IntoResponse {
    let rows = match sqlx::query("SELECT id, microservice_id, payload, run_at, status, cron_expression, created_at FROM scheduled_jobs ORDER BY run_at DESC")
        .fetch_all(&state.db_proxy.pool)
        .await
    {
        Ok(r) => r,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    };

    let mut list = Vec::new();
    for r in rows {
        let payload_str: String = r.get("payload");
        let payload: serde_json::Value = serde_json::from_str(&payload_str).unwrap_or(json!({}));
        list.push(shared::ScheduledJobDTO {
            id: Some(r.get::<i64, _>("id").to_string()),
            microservice_id: r.get::<i64, _>("microservice_id").to_string(),
            payload,
            run_at: r.get("run_at"),
            status: r.get("status"),
            cron_expression: r.get("cron_expression"),
            created_at: Some(r.get("created_at")),
        });
    }

    (StatusCode::OK, Json(list)).into_response()
}

async fn delete_schedule_handler(
    _user: auth::AuthenticatedUser,
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let job_id = match id.parse::<i64>() {
        Ok(val) => val,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    let _ = sqlx::query("DELETE FROM scheduled_jobs WHERE id = ?")
        .bind(job_id)
        .execute(&state.db_proxy.pool)
        .await;

    StatusCode::NO_CONTENT.into_response()
}

async fn delete_env_by_id(
    _user: auth::AuthenticatedUser,
    axum::extract::State(state): axum::extract::State<AppState>,
    Path((id, env_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let ms_id = match resolve_ms_id(&state.db_proxy.pool, &id).await {
        Ok(val) => val,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let e_id = match env_id.parse::<i64>() {
        Ok(val) => val,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    let _ = sqlx::query("DELETE FROM microservice_envs WHERE microservice_id = ? AND id = ?")
        .bind(ms_id)
        .bind(e_id)
        .execute(&state.db_proxy.pool)
        .await;

    StatusCode::NO_CONTENT.into_response()
}

async fn edit_env(
    _user: auth::AuthenticatedUser,
    axum::extract::State(state): axum::extract::State<AppState>,
    Path((id, env_id)): Path<(String, String)>,
    Json(payload): Json<shared::MicroserviceEnvDTO>,
) -> impl IntoResponse {
    let ms_id = match resolve_ms_id(&state.db_proxy.pool, &id).await {
        Ok(val) => val,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    };
    let e_id = match env_id.parse::<i64>() {
        Ok(val) => val,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid environment ID" }))).into_response(),
    };

    let config_str = serde_json::to_string(&payload.config).unwrap_or_else(|_| "{}".to_string());

    let mut tx = match state.db_proxy.pool.begin().await {
        Ok(t) => t,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    };

    if payload.is_default {
        let _ = sqlx::query("UPDATE microservice_envs SET is_default = 0 WHERE microservice_id = ?")
            .bind(ms_id)
            .execute(&mut *tx)
            .await;
    }

    let query = "UPDATE microservice_envs SET name = ?, config = ?, is_default = ? WHERE id = ? AND microservice_id = ?";
    let res = sqlx::query(query)
        .bind(&payload.name)
        .bind(&config_str)
        .bind(payload.is_default)
        .bind(e_id)
        .bind(ms_id)
        .execute(&mut *tx)
        .await;

    match res {
        Ok(_) => {
            if let Err(e) = tx.commit().await {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response()
            } else {
                (StatusCode::OK, Json(shared::MicroserviceEnvDTO {
                    id: Some(env_id),
                    microservice_id: Some(id),
                    name: payload.name,
                    config: payload.config,
                    is_default: payload.is_default,
                })).into_response()
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn get_env_by_id(
    _user: auth::AuthenticatedUser,
    axum::extract::State(state): axum::extract::State<AppState>,
    Path((id, env_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let ms_id = match resolve_ms_id(&state.db_proxy.pool, &id).await {
        Ok(val) => val,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    };
    let e_id = match env_id.parse::<i64>() {
        Ok(val) => val,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid environment ID" }))).into_response(),
    };

    let row = match sqlx::query("SELECT id, microservice_id, name, config, is_default FROM microservice_envs WHERE id = ? AND microservice_id = ?")
        .bind(e_id)
        .bind(ms_id)
        .fetch_optional(&state.db_proxy.pool)
        .await
    {
        Ok(Some(r)) => r,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(json!({ "error": "Environment variable not found" }))).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    };

    let config_str: String = row.get("config");
    let config = serde_json::from_str(&config_str).unwrap_or(serde_json::Value::Null);

    let env = shared::MicroserviceEnvDTO {
        id: Some(row.get::<i64, _>("id").to_string()),
        microservice_id: Some(id),
        name: row.get("name"),
        config,
        is_default: row.get("is_default"),
    };

    (StatusCode::OK, Json(env)).into_response()
}

async fn get_build_logs(
    Path(id): Path<String>,
) -> impl IntoResponse {
    let log_file_path = format!("/tmp/build_log_{}.log", id);
    let logs = std::fs::read_to_string(log_file_path).unwrap_or_default();
    (StatusCode::OK, Json(serde_json::json!({ "logs": logs }))).into_response()
}

// --- Versions ---

async fn list_versions(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let ms_id = match id.parse::<i64>() {
        Ok(val) => val,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid microservice ID" }))).into_response(),
    };

    let rows = sqlx::query("SELECT * FROM microservice_versions WHERE microservice_id = ? ORDER BY version_number DESC")
        .bind(ms_id)
        .fetch_all(&state.db_proxy.pool)
        .await
        .unwrap_or_default();

    let mut versions = Vec::new();
    for row in rows {
        let id: i64 = row.get("id");
        let microservice_id: i64 = row.get("microservice_id");
        versions.push(MicroserviceVersionDTO {
            id: Some(id.to_string()),
            microservice_id: microservice_id.to_string(),
            version_number: row.get("version_number"),
            version_tag: row.get("version_tag"),
            source_type: row.get("source_type"),
            source_code: row.get("source_code"),
            container_image_tag: row.get("container_image_tag"),
            container_id: row.get("container_id"),
            status: row.get("status"),
            changelog: row.get("changelog"),
            error_message: row.get("error_message"),
            created_at: row.get("created_at"),
        });
    }

    Json(versions).into_response()
}

async fn create_version(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<MicroserviceVersionDTO>,
) -> impl IntoResponse {
    let ms_id = match id.parse::<i64>() {
        Ok(val) => val,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid microservice ID" }))).into_response(),
    };

    // Write code to a persistent folder on disk: services_runtime/service_<id>/<version_tag>/src/main.rs
    let service_dir = std::path::PathBuf::from("services_runtime")
        .join(format!("service_{}", id))
        .join(&payload.version_tag);
    if service_dir.exists() {
        let _ = std::fs::remove_dir_all(&service_dir);
    }
    let _ = std::fs::create_dir_all(&service_dir);
    
    let extract_res = docker_manager::extract_source(&payload.source_type, &payload.source_code, &service_dir);
    if let Err(e) = extract_res {
        println!("Failed to extract persistent files to disk: {}", e);
    }

    // 1. Build image using DockerManager
    let build_res = state.docker_manager.build_image(&id, &payload.version_tag, &payload.source_type, &payload.source_code).await;
    let (status, container_image_tag, error_message) = match build_res {
        Ok((tag, logs)) => ("running".to_string(), Some(tag), Some(logs)),
        Err(e) => {
            println!("Docker build failed: {}", e);
            ("failed".to_string(), None, Some(e))
        }
    };

    // 2. Persist version metadata
    let query = r#"
        INSERT OR REPLACE INTO microservice_versions (
            microservice_id, version_number, version_tag, source_type, 
            source_code, container_image_tag, status, changelog, error_message
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
    "#;

    let res = sqlx::query(query)
        .bind(ms_id)
        .bind(payload.version_number)
        .bind(payload.version_tag.clone())
        .bind(payload.source_type.clone())
        .bind(payload.source_code.clone())
        .bind(container_image_tag.clone())
        .bind(status.clone())
        .bind(payload.changelog.clone())
        .bind(error_message.clone())
        .execute(&state.db_proxy.pool)
        .await;

    match res {
        Ok(r) => {
            let version_id = r.last_insert_rowid();
            
            // If version was build successfully, update the active_version_id on service
            if status == "running" {
                let update_ms_query = "UPDATE microservices SET active_version_id = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?";
                let _ = sqlx::query(update_ms_query)
                    .bind(version_id)
                    .bind(ms_id)
                    .execute(&state.db_proxy.pool)
                    .await;
            }
            (StatusCode::CREATED, Json(json!({
                "id": version_id.to_string(),
                "microservice_id": ms_id.to_string(),
                "version_number": payload.version_number,
                "version_tag": payload.version_tag,
                "source_type": payload.source_type,
                "source_code": payload.source_code,
                "container_image_tag": container_image_tag,
                "status": status,
                "changelog": payload.changelog,
                "error_message": error_message,
            }))).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn rollback_version(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let ms_id = match id.parse::<i64>() {
        Ok(val) => val,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid microservice ID" }))).into_response(),
    };
    
    let version_id: Option<i64> = match &body["version_id"] {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) if s == "null" || s.is_empty() => None,
        serde_json::Value::String(s) => s.parse::<i64>().ok(),
        serde_json::Value::Number(n) => n.as_i64(),
        _ => None,
    };

    let update_query = "UPDATE microservices SET active_version_id = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?";
    let res = sqlx::query(update_query)
        .bind(version_id)
        .bind(ms_id)
        .execute(&state.db_proxy.pool)
        .await;

    match res {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

fn adapt_payload(input: &serde_json::Value) -> serde_json::Value {
    let mut payload_map = serde_json::Map::new();
    let mut nested_payloads = Vec::new();

    if let Some(obj) = input.as_object() {
        for (key, val) in obj {
            let json_val = if let Some(s_str) = val.as_str() {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s_str) {
                    let mut final_parsed = parsed.clone();
                    if let Some(inner_str) = parsed.as_str() {
                        if let Ok(inner_parsed) = serde_json::from_str::<serde_json::Value>(inner_str) {
                            if inner_parsed.is_object() {
                                final_parsed = inner_parsed;
                            }
                        }
                    }
                    if key == "payload" && final_parsed.is_object() {
                        nested_payloads.push(final_parsed.clone());
                    }
                    final_parsed
                } else if let Ok(num) = s_str.parse::<i64>() {
                    json!(num)
                } else if let Ok(f) = s_str.parse::<f64>() {
                    json!(f)
                } else if let Ok(b) = s_str.parse::<bool>() {
                    json!(b)
                } else {
                    val.clone()
                }
            } else {
                if key == "payload" && val.is_object() {
                    nested_payloads.push(val.clone());
                }
                val.clone()
            };
            payload_map.insert(key.clone(), json_val);
        }
    } else {
        return input.clone();
    }

    for nested in nested_payloads {
        if let Some(obj) = nested.as_object() {
            for (k, v) in obj {
                let final_val = if let Some(s) = v.as_str() {
                    if let Ok(num) = s.parse::<i64>() {
                        json!(num)
                    } else if let Ok(f) = s.parse::<f64>() {
                        json!(f)
                    } else if let Ok(b) = s.parse::<bool>() {
                        json!(b)
                    } else {
                        v.clone()
                    }
                } else {
                    v.clone()
                };
                payload_map.insert(k.clone(), final_val);
            }
        }
    }

    let mut final_val = serde_json::Value::Object(payload_map);
    if let Some(inner_payload) = final_val.get("payload") {
        if inner_payload.is_object() || inner_payload.is_array() {
            final_val = inner_payload.clone();
        }
    }
    final_val
}

#[derive(serde::Deserialize)]
struct TestVersionRequest {
    payload: serde_json::Value,
}

async fn test_version(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(version_id): Path<String>,
    Json(req): Json<TestVersionRequest>,
) -> impl IntoResponse {
    let v_id = match version_id.parse::<i64>() {
        Ok(val) => val,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid version ID" }))).into_response(),
    };

    let query = "SELECT microservice_id, container_image_tag FROM microservice_versions WHERE id = ?";
    let row = match sqlx::query(query)
        .bind(v_id)
        .fetch_optional(&state.db_proxy.pool)
        .await
    {
        Ok(Some(r)) => r,
        _ => return (StatusCode::NOT_FOUND, Json(json!({ "error": "Version not found" }))).into_response(),
    };

    let ms_id: i64 = row.get("microservice_id");
    let image_tag: Option<String> = row.get("container_image_tag");
    let image_tag = match image_tag {
        Some(tag) if !tag.is_empty() => tag,
        _ => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Image not compiled yet or build failed" }))).into_response(),
    };

    let adapted = adapt_payload(&req.payload);
    let env_vars = db::resolve_microservice_env(&state.db_proxy.pool, ms_id, &adapted)
        .await
        .unwrap_or_default();

    let container_name = format!("runner_ms_{}", ms_id);
    match state.docker_manager.run_container(&container_name, &image_tag, &adapted, Some(env_vars)).await {
        Ok((logs, stdout_val)) => {
            let mut is_panic = false;
            let mut panic_msg = String::new();
            if let Some(obj) = stdout_val.as_object() {
                if let Some(raw_out) = obj.get("raw_output").and_then(|r| r.as_str()) {
                    if raw_out.contains("panicked") {
                        is_panic = true;
                        panic_msg = raw_out.to_string();
                    }
                }
            }
            if is_panic {
                (StatusCode::OK, Json(json!({ "status": "error", "error": panic_msg }))).into_response()
            } else {
                (StatusCode::OK, Json(json!({ "status": "success", "logs": logs, "output": stdout_val }))).into_response()
            }
        }
        Err(e) => {
            (StatusCode::OK, Json(json!({ "status": "error", "error": e }))).into_response()
        }
    }
}

async fn get_version_container_status(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(version_id): Path<String>,
) -> impl IntoResponse {
    let v_id = match version_id.parse::<i64>() {
        Ok(val) => val,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({ "status": "unknown", "error": "Invalid version ID" }))).into_response(),
    };

    let query = "SELECT container_image_tag FROM microservice_versions WHERE id = ?";
    let row = match sqlx::query(query)
        .bind(v_id)
        .fetch_optional(&state.db_proxy.pool)
        .await
    {
        Ok(Some(r)) => r,
        _ => return (StatusCode::NOT_FOUND, Json(json!({ "status": "unknown", "error": "Version not found" }))).into_response(),
    };

    let image_tag: Option<String> = row.get("container_image_tag");
    let image_tag = match image_tag {
        Some(tag) if !tag.is_empty() => tag,
        _ => return (StatusCode::OK, Json(json!({ "status": "not_compiled" }))).into_response(),
    };

    let mut is_running = false;
    let list_options = Some(bollard::container::ListContainersOptions::<String> {
        all: false,
        ..Default::default()
    });

    if let Ok(containers) = state.docker_manager.docker.list_containers(list_options).await {
        for container in containers {
            if let Some(ref image) = container.image {
                if image.contains(&image_tag) || image_tag.contains(image) {
                    is_running = true;
                    break;
                }
            }
        }
    }

    let status_str = if is_running { "running" } else { "stopped" };
    (StatusCode::OK, Json(json!({ "status": status_str }))).into_response()
}

// --- Queues CRUD ---

async fn list_queues(axum::extract::State(state): axum::extract::State<AppState>) -> impl IntoResponse {
    let rows = sqlx::query("SELECT * FROM queues")
        .fetch_all(&state.db_proxy.pool)
        .await
        .unwrap_or_default();

    let mut queues = Vec::new();
    for row in rows {
        let id: i64 = row.get("id");
        let tags_str: String = row.get("tags");
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();

        queues.push(QueueDTO {
            id: Some(id.to_string()),
            stream_key: row.get("stream_key"),
            name: Some(row.get("stream_key")),
            consumer_group: row.get("consumer_group"),
            is_active: row.get("is_active"),
            tags,
            created_at: Some(chrono::Utc::now()),
        });
    }
    Json(queues)
}

async fn create_queue(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(payload): Json<QueueDTO>,
) -> impl IntoResponse {
    let tags_str = serde_json::to_string(&payload.tags).unwrap_or_else(|_| "[]".to_string());
    let query = r#"
        INSERT INTO queues (
            stream_key, consumer_group, is_active, tags
        ) VALUES (?, ?, ?, ?)
    "#;
    let res = sqlx::query(query)
        .bind(payload.stream_key.clone())
        .bind(payload.consumer_group.clone())
        .bind(payload.is_active)
        .bind(tags_str)
        .execute(&state.db_proxy.pool)
        .await;

    match res {
        Ok(r) => {
            let id = r.last_insert_rowid();
            let mut created = payload;
            created.id = Some(id.to_string());
            (StatusCode::CREATED, Json(created)).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn delete_queue(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Ok(id_int) = id.parse::<i64>() {
        let log_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM execution_logs WHERE queue_id = ?")
            .bind(id_int)
            .fetch_one(&state.db_proxy.pool)
            .await
            .unwrap_or((0,));
        if log_count.0 > 0 {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Cannot delete queue because it has execution logs." }))).into_response();
        }

        let _ = sqlx::query("DELETE FROM queues WHERE id = ?")
            .bind(id_int)
            .execute(&state.db_proxy.pool)
            .await;
    }
    StatusCode::NO_CONTENT.into_response()
}

// --- Bindings CRUD ---

async fn list_bindings(axum::extract::State(state): axum::extract::State<AppState>) -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT b.*, q.stream_key AS queue_name, m.name AS microservice_name \
         FROM bindings b \
         LEFT JOIN queues q ON b.queue_id = q.id \
         LEFT JOIN microservices m ON b.microservice_id = m.id"
    )
    .fetch_all(&state.db_proxy.pool)
    .await
    .unwrap_or_default();

    let mut bindings = Vec::new();
    for row in rows {
        let id: i64 = row.get("id");
        
        let queue_id: i64 = row.get("queue_id");
        let microservice_id: i64 = row.get("microservice_id");
        let target_version_id: Option<i64> = row.get("target_version_id");
        
        let queue_name: Option<String> = row.get("queue_name");
        let microservice_name: Option<String> = row.get("microservice_name");

        let queue = queue_name.map(|qname| QueueDTO {
            id: Some(queue_id.to_string()),
            stream_key: qname.clone(),
            name: Some(qname),
            consumer_group: "orchestrator_group".to_string(),
            is_active: true,
            tags: vec![],
            created_at: None,
        });

        let microservice = microservice_name.map(|mname| MicroserviceDTO {
            id: Some(microservice_id.to_string()),
            uuid: None,
            name: mname,
            language: "rust".to_string(),
            description: None,
            tags: vec![],
            active_version_id: None,
            active_version_tag: None,
            on_success_action: None,
            on_success_config: None,
            on_error_action: None,
            on_error_config: None,
            is_active: true,
            created_at: None,
            updated_at: None,
        });

        let on_success_config_str: String = row.get("on_success_config");
        let on_success_config = serde_json::from_str(&on_success_config_str).unwrap_or(serde_json::Value::Null);

        let on_error_config_str: String = row.get("on_error_config");
        let on_error_config = serde_json::from_str(&on_error_config_str).unwrap_or(serde_json::Value::Null);

        bindings.push(BindingDTO {
            id: Some(id.to_string()),
            queue_id: queue_id.to_string(),
            microservice_id: microservice_id.to_string(),
            queue,
            microservice,
            target_version_id: target_version_id.map(|v| v.to_string()),
            on_success_action: row.get("on_success_action"),
            on_success_config,
            on_error_action: row.get("on_error_action"),
            on_error_config,
            is_active: row.get("is_active"),
        });
    }
    Json(bindings)
}

async fn create_binding(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(payload): Json<BindingDTO>,
) -> impl IntoResponse {
    let q_id = payload.queue_id.parse::<i64>().unwrap_or(0);
    let ms_id = payload.microservice_id.parse::<i64>().unwrap_or(0);
    let v_id = payload.target_version_id.as_ref().and_then(|v| v.parse::<i64>().ok());

    let success_config_str = serde_json::to_string(&payload.on_success_config).unwrap_or_default();
    let error_config_str = serde_json::to_string(&payload.on_error_config).unwrap_or_default();

    let query = r#"
        INSERT INTO bindings (
            queue_id, microservice_id, target_version_id, on_success_action, 
            on_success_config, on_error_action, on_error_config, is_active
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
    "#;
    let res = sqlx::query(query)
        .bind(q_id)
        .bind(ms_id)
        .bind(v_id)
        .bind(payload.on_success_action.clone())
        .bind(success_config_str)
        .bind(payload.on_error_action.clone())
        .bind(error_config_str)
        .bind(payload.is_active)
        .execute(&state.db_proxy.pool)
        .await;

    match res {
        Ok(r) => {
            let id = r.last_insert_rowid();
            let mut created = payload;
            created.id = Some(id.to_string());
            (StatusCode::CREATED, Json(created)).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn delete_binding(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Ok(id_int) = id.parse::<i64>() {
        let _ = sqlx::query("DELETE FROM bindings WHERE id = ?")
            .bind(id_int)
            .execute(&state.db_proxy.pool)
            .await;
    }
    StatusCode::NO_CONTENT
}

// --- API Keys CRUD ---

async fn list_api_keys(
    _user: auth::AuthenticatedUser,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> impl IntoResponse {
    let rows = match sqlx::query("SELECT id, name, key_value, created_at FROM api_keys ORDER BY created_at DESC")
        .fetch_all(&state.db_proxy.pool)
        .await
    {
        Ok(r) => r,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    };

    let mut list = Vec::new();
    for r in rows {
        let created_at: Option<chrono::DateTime<chrono::Utc>> = r.try_get("created_at").ok();
        list.push(shared::ApiKeyDTO {
            id: Some(r.get::<i64, _>("id").to_string()),
            name: r.get("name"),
            key_value: Some(r.get("key_value")),
            created_at,
        });
    }

    (StatusCode::OK, Json(list)).into_response()
}

#[derive(serde::Deserialize)]
struct CreateApiKeyRequest {
    name: String,
}

async fn create_api_key(
    _user: auth::AuthenticatedUser,
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<CreateApiKeyRequest>,
) -> impl IntoResponse {
    if req.name.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Name cannot be empty" }))).into_response();
    }

    use rand::Rng;
    let random_bytes: [u8; 24] = rand::thread_rng().gen();
    let key_value = format!("dp_{}", hex::encode(random_bytes));

    let query = "INSERT INTO api_keys (name, key_value) VALUES (?, ?)";
    let res = sqlx::query(query)
        .bind(&req.name)
        .bind(&key_value)
        .execute(&state.db_proxy.pool)
        .await;

    match res {
        Ok(r) => {
            let id = r.last_insert_rowid();
            (StatusCode::CREATED, Json(shared::ApiKeyDTO {
                id: Some(id.to_string()),
                name: req.name,
                key_value: Some(key_value),
                created_at: Some(chrono::Utc::now()),
            })).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn delete_api_key(
    _user: auth::AuthenticatedUser,
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let id_int = match id.parse::<i64>() {
        Ok(val) => val,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    let _ = sqlx::query("DELETE FROM api_keys WHERE id = ?")
        .bind(id_int)
        .execute(&state.db_proxy.pool)
        .await;

    StatusCode::NO_CONTENT.into_response()
}

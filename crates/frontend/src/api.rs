use gloo_net::http::Request;
use shared::*;
use serde_json::Value;

const BASE_URL: &str = "http://localhost:4500/api/v1";

fn get_token() -> Option<String> {
    let window = web_sys::window()?;
    let storage = window.local_storage().ok()??;
    storage.get_item("auth_token").ok()?
}

pub fn check_response_status(status: u16) {
    if status == 401 {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                let _ = storage.remove_item("auth_token");
            }
            if let Ok(event) = web_sys::CustomEvent::new("auth_unauthorized") {
                let _ = window.dispatch_event(&event);
            }
        }
    }
}

pub async fn login(req: LoginRequest) -> Result<LoginResponse, String> {
    let url = format!("{}/auth/login", BASE_URL);
    let resp = Request::post(&url)
        .json(&req)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Login failed: Status {}", resp.status()));
    }

    let data: LoginResponse = resp.json().await.map_err(|e| e.to_string())?;
    
    // Save token to localStorage
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            let _ = storage.set_item("auth_token", &data.token);
        }
    }

    Ok(data)
}

pub async fn register(req: LoginRequest) -> Result<Value, String> {
    let url = format!("{}/auth/register", BASE_URL);
    let resp = Request::post(&url)
        .json(&req)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Registration failed: Status {}", resp.status()));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn get_system_health() -> Result<SystemHealthResponse, String> {
    let url = format!("{}/system/health", BASE_URL);
    let resp = Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Failed to load system health: Status {}", resp.status()));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn search_logs(query: LogFilterQuery) -> Result<LogSearchResponse, String> {
    let url = format!("{}/logs/search", BASE_URL);
    let resp = Request::post(&url)
        .json(&query)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Logs query failed: Status {}", resp.status()));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn delete_log(id: &str) -> Result<(), String> {
    let url = format!("{}/logs/{}", BASE_URL, id);
    let resp = Request::delete(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Delete log failed: Status {}", resp.status()));
    }
    Ok(())
}

pub async fn resend_log(id: &str) -> Result<(), String> {
    let url = format!("{}/logs/{}/resend", BASE_URL, id);
    let resp = Request::post(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Resend log failed: Status {}", resp.status()));
    }
    Ok(())
}

pub async fn get_services() -> Result<Vec<MicroserviceDTO>, String> {
    let url = format!("{}/services", BASE_URL);
    let resp = Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Failed to fetch services: Status {}", resp.status()));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn create_service(service: MicroserviceDTO) -> Result<MicroserviceDTO, String> {
    let url = format!("{}/services", BASE_URL);
    let resp = Request::post(&url)
        .json(&service)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Failed to create service: Status {}", resp.status()));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn update_service(id: &str, service: MicroserviceDTO) -> Result<MicroserviceDTO, String> {
    let url = format!("{}/services/{}", BASE_URL, id);
    let resp = Request::put(&url)
        .json(&service)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Failed to update service: Status {}", resp.status()));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn delete_service(id: &str) -> Result<(), String> {
    let url = format!("{}/services/{}", BASE_URL, id);
    let resp = Request::delete(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        let text = resp.text().await.unwrap_or_default();
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(msg) = val.get("error").and_then(|m| m.as_str()) {
                return Err(msg.to_string());
            }
        }
        return Err(format!("Failed to delete service: Status {}", resp.status()));
    }

    Ok(())
}

pub async fn create_version(service_id: &str, version: MicroserviceVersionDTO) -> Result<Value, String> {
    let url = format!("{}/services/{}/versions", BASE_URL, service_id);
    let resp = Request::post(&url)
        .json(&version)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        let err_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Failed to create version: Status {} - {}", resp.status(), err_text));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn get_queues() -> Result<Vec<QueueDTO>, String> {
    let url = format!("{}/queues", BASE_URL);
    let resp = Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Failed to fetch queues: Status {}", resp.status()));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn create_queue(queue: QueueDTO) -> Result<QueueDTO, String> {
    let url = format!("{}/queues", BASE_URL);
    let resp = Request::post(&url)
        .json(&queue)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Failed to create queue: Status {}", resp.status()));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn delete_queue(id: &str) -> Result<(), String> {
    let url = format!("{}/queues/{}", BASE_URL, id);
    let resp = Request::delete(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        let text = resp.text().await.unwrap_or_default();
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(msg) = val.get("error").and_then(|m| m.as_str()) {
                return Err(msg.to_string());
            }
        }
        return Err(format!("Failed to delete queue: Status {}", resp.status()));
    }

    Ok(())
}

pub async fn get_pools() -> Result<Vec<DbPoolDTO>, String> {
    let url = format!("{}/pools", BASE_URL);
    let resp = Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Failed to fetch pools: Status {}", resp.status()));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn create_pool(pool: DbPoolDTO) -> Result<DbPoolDTO, String> {
    let url = format!("{}/pools", BASE_URL);
    let resp = Request::post(&url)
        .json(&pool)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Failed to create pool: Status {}", resp.status()));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn update_pool(id: &str, pool: DbPoolDTO) -> Result<DbPoolDTO, String> {
    let url = format!("{}/pools/{}", BASE_URL, id);
    let resp = Request::put(&url)
        .json(&pool)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Failed to update pool: Status {}", resp.status()));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn delete_pool(id: &str) -> Result<(), String> {
    let url = format!("{}/pools/{}", BASE_URL, id);
    let resp = Request::delete(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Failed to delete pool: Status {}", resp.status()));
    }

    Ok(())
}

pub async fn test_pool_connection(id: &str) -> Result<String, String> {
    let url = format!("{}/pools/{}/test", BASE_URL, id);
    let resp = Request::post(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let text = resp.text().await.unwrap_or_default();
    if !resp.ok() {
        check_response_status(resp.status());
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(msg) = val.get("message").and_then(|m| m.as_str()) {
                return Err(msg.to_string());
            }
        }
        return Err(format!("Failed to test pool: {}", text));
    }

    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
        if let Some(msg) = val.get("message").and_then(|m| m.as_str()) {
            return Ok(msg.to_string());
        }
    }
    Ok("Connected successfully".to_string())
}

pub async fn test_pool_connection_payload(connection_url: &str) -> Result<String, String> {
    let url = format!("{}/pools/test", BASE_URL);
    let resp = Request::post(&url)
        .header("Authorization", &format!("Bearer {}", get_token().unwrap_or_default()))
        .json(&serde_json::json!({ "connection_url": connection_url }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let text = resp.text().await.unwrap_or_default();
    if !resp.ok() {
        check_response_status(resp.status());
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(msg) = val.get("message").and_then(|m| m.as_str()) {
                return Err(msg.to_string());
            }
        }
        return Err(format!("Failed to test pool: {}", text));
    }

    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
        if let Some(msg) = val.get("message").and_then(|m| m.as_str()) {
            return Ok(msg.to_string());
        }
    }
    Ok("Connected successfully".to_string())
}

pub async fn get_bindings() -> Result<Vec<BindingDTO>, String> {
    let url = format!("{}/bindings", BASE_URL);
    let resp = Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Failed to fetch bindings: Status {}", resp.status()));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn create_binding(binding: BindingDTO) -> Result<BindingDTO, String> {
    let url = format!("{}/bindings", BASE_URL);
    let resp = Request::post(&url)
        .json(&binding)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Failed to create binding: Status {}", resp.status()));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn delete_binding(id: &str) -> Result<(), String> {
    let url = format!("{}/bindings/{}", BASE_URL, id);
    let resp = Request::delete(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Failed to delete binding: Status {}", resp.status()));
    }

    Ok(())
}

pub async fn get_versions(service_id: &str) -> Result<Vec<MicroserviceVersionDTO>, String> {
    let url = format!("{}/services/{}/versions", BASE_URL, service_id);
    let resp = Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Failed to fetch versions: Status {}", resp.status()));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn rollback_version(service_id: &str, version_id: &str) -> Result<(), String> {
    let url = format!("{}/services/{}/rollback", BASE_URL, service_id);
    let resp = Request::post(&url)
        .json(&serde_json::json!({ "version_id": version_id }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        let err_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Failed to rollback version: Status {} - {}", resp.status(), err_text));
    }

    Ok(())
}

pub async fn get_version_container_status(version_id: &str) -> Result<String, String> {
    let url = format!("{}/versions/{}/status", BASE_URL, version_id);
    let resp = Request::get(&url)
        .header("Authorization", &format!("Bearer {}", get_token().unwrap_or_default()))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Failed to get version status: Status {}", resp.status()));
    }

    let val: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(val["status"].as_str().unwrap_or("unknown").to_string())
}

pub async fn test_version(version_id: &str, payload: serde_json::Value) -> Result<serde_json::Value, String> {
    let url = format!("{}/versions/{}/test", BASE_URL, version_id);
    let resp = Request::post(&url)
        .json(&serde_json::json!({ "payload": payload }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        let err_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Failed to test version: Status {} - {}", resp.status(), err_text));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn export_system_data() -> Result<serde_json::Value, String> {
    let url = format!("{}/system/export", BASE_URL);
    let resp = Request::get(&url)
        .header("Authorization", &format!("Bearer {}", get_token().unwrap_or_default()))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        let err_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Failed to export system data: Status {} - {}", resp.status(), err_text));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn import_system_data(payload: serde_json::Value) -> Result<(), String> {
    let url = format!("{}/system/import", BASE_URL);
    let resp = Request::post(&url)
        .header("Authorization", &format!("Bearer {}", get_token().unwrap_or_default()))
        .json(&payload)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        let err_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Failed to import system data: Status {} - {}", resp.status(), err_text));
    }

    Ok(())
}

pub async fn get_envs(ms_id: &str) -> Result<Vec<MicroserviceEnvDTO>, String> {
    let url = format!("{}/services/{}/envs", BASE_URL, ms_id);
    let resp = Request::get(&url)
        .header("Authorization", &format!("Bearer {}", get_token().unwrap_or_default()))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Failed to load envs: Status {}", resp.status()));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn create_env(ms_id: &str, payload: MicroserviceEnvDTO) -> Result<MicroserviceEnvDTO, String> {
    let url = format!("{}/services/{}/envs", BASE_URL, ms_id);
    let resp = Request::post(&url)
        .header("Authorization", &format!("Bearer {}", get_token().unwrap_or_default()))
        .json(&payload)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        let err_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Failed to create env: Status {} - {}", resp.status(), err_text));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn edit_env(ms_id: &str, env_id: &str, payload: MicroserviceEnvDTO) -> Result<MicroserviceEnvDTO, String> {
    let url = format!("{}/services/{}/envs/{}/edit", BASE_URL, ms_id, env_id);
    let resp = Request::post(&url)
        .header("Authorization", &format!("Bearer {}", get_token().unwrap_or_default()))
        .json(&payload)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        let err_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Failed to edit env: Status {} - {}", resp.status(), err_text));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn delete_env_by_id(ms_id: &str, env_id: &str) -> Result<(), String> {
    let url = format!("{}/services/{}/envs/{}", BASE_URL, ms_id, env_id);
    let resp = Request::delete(&url)
        .header("Authorization", &format!("Bearer {}", get_token().unwrap_or_default()))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        let err_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Failed to delete env: Status {} - {}", resp.status(), err_text));
    }

    Ok(())
}

pub async fn get_build_logs(ms_id: &str) -> Result<String, String> {
    let url = format!("{}/services/{}/build-logs", BASE_URL, ms_id);
    let resp = Request::get(&url)
        .header("Authorization", &format!("Bearer {}", get_token().unwrap_or_default()))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Failed to get build logs: Status {}", resp.status()));
    }

    let json_val: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(json_val["logs"].as_str().unwrap_or_default().to_string())
}

pub async fn get_api_keys() -> Result<Vec<ApiKeyDTO>, String> {
    let url = format!("{}/auth/keys", BASE_URL);
    let resp = Request::get(&url)
        .header("Authorization", &format!("Bearer {}", get_token().unwrap_or_default()))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Failed to load API keys: Status {}", resp.status()));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn create_api_key(name: &str) -> Result<ApiKeyDTO, String> {
    let url = format!("{}/auth/keys", BASE_URL);
    let resp = Request::post(&url)
        .header("Authorization", &format!("Bearer {}", get_token().unwrap_or_default()))
        .json(&serde_json::json!({ "name": name }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        let err_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Failed to create API key: Status {} - {}", resp.status(), err_text));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn delete_api_key(id: &str) -> Result<(), String> {
    let url = format!("{}/auth/keys/{}", BASE_URL, id);
    let resp = Request::delete(&url)
        .header("Authorization", &format!("Bearer {}", get_token().unwrap_or_default()))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        let err_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Failed to delete API key: Status {} - {}", resp.status(), err_text));
    }

    Ok(())
}

pub async fn get_schedules() -> Result<Vec<ScheduledJobDTO>, String> {
    let url = format!("{}/schedules", BASE_URL);
    let resp = Request::get(&url)
        .header("Authorization", &format!("Bearer {}", get_token().unwrap_or_default()))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        return Err(format!("Failed to load schedules: Status {}", resp.status()));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn schedule_job(ms_id: &str, payload: ScheduleJobRequest) -> Result<ScheduledJobDTO, String> {
    let url = format!("{}/services/{}/schedule", BASE_URL, ms_id);
    let resp = Request::post(&url)
        .header("Authorization", &format!("Bearer {}", get_token().unwrap_or_default()))
        .json(&payload)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        let err_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Failed to schedule job: Status {} - {}", resp.status(), err_text));
    }

    resp.json().await.map_err(|e| e.to_string())
}

pub async fn delete_schedule(id: &str) -> Result<(), String> {
    let url = format!("{}/schedules/{}", BASE_URL, id);
    let resp = Request::delete(&url)
        .header("Authorization", &format!("Bearer {}", get_token().unwrap_or_default()))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        check_response_status(resp.status());
        let err_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Failed to delete schedule: Status {} - {}", resp.status(), err_text));
    }

    Ok(())
}

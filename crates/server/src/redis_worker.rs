use std::sync::Arc;
use tokio::time::{sleep, Duration};
use redis::AsyncCommands;
use deadpool_redis::Pool;
use sqlx::{Row, SqlitePool};
use serde_json::json;
use chrono::Utc;
use shared::ExecutionLogDTO;
use crate::db::DbProxy;
use crate::docker_manager::DockerManager;

pub struct RedisWorkerManager {
    redis_pool: Pool,
    db_proxy: DbProxy,
    docker_manager: DockerManager,
}

impl RedisWorkerManager {
    pub fn new(redis_pool: Pool, db_proxy: DbProxy, docker_manager: DockerManager) -> Self {
        Self {
            redis_pool,
            db_proxy,
            docker_manager,
        }
    }

    pub fn start_worker_loop(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                if let Err(e) = self.process_queues().await {
                    eprintln!("Error in Redis Stream Worker loop: {}", e);
                }
                sleep(Duration::from_secs(2)).await;
            }
        });
    }

    async fn process_queues(&self) -> Result<(), String> {
        let pool = &self.db_proxy.pool;

        // 1. Test Redis Connection
        let mut conn = match self.redis_pool.get().await {
            Ok(c) => c,
            Err(e) => {
                println!("🔴 [Redis Worker] Redis Connection FAILED: could not get connection from pool: {}", e);
                return Err(e.to_string());
            }
        };
        let ping_res: Result<String, redis::RedisError> = redis::cmd("PING").query_async(&mut conn).await;
        if let Err(e) = ping_res {
            println!("🔴 [Redis Worker] Redis Connection FAILED: PING error: {}", e);
            return Err(e.to_string());
        }

        // Fetch active bindings from SQLite
        let query_str = r#"
            SELECT 
                b.id,
                b.queue_id,
                b.microservice_id,
                b.target_version_id,
                b.on_success_action,
                b.on_success_config,
                b.on_error_action,
                b.on_error_config,
                q.stream_key,
                q.consumer_group,
                m.name AS service_name,
                m.active_version_id AS service_active_version
            FROM bindings b
            JOIN queues q ON b.queue_id = q.id
            JOIN microservices m ON b.microservice_id = m.id
            WHERE b.is_active = 1
        "#;

        let rows = sqlx::query(query_str)
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?;

        if rows.is_empty() {
            println!("🟢 [Redis Worker] Redis Connection OK. No active bindings/queues are currently registered.");
        } else {
            let streams: Vec<String> = rows.iter().map(|r| r.get::<String, _>("stream_key")).collect();
            println!("🟢 [Redis Worker] Redis Connection OK. Awaiting messages on streams: {:?}", streams);
        }

        for row in rows {
            let binding_id: i64 = row.get("id");
            let stream_key: String = row.get("stream_key");
            let consumer_group: String = row.get("consumer_group");
            let queue_id: i64 = row.get("queue_id");
            let microservice_id: i64 = row.get("microservice_id");

            let target_version_id: Option<i64> = row.get("target_version_id");
            let active_version_id: Option<i64> = row.get("service_active_version");

            let on_success_action: Option<String> = row.get("on_success_action");
            let on_success_config: Option<String> = row.get("on_success_config");
            let on_error_action: Option<String> = row.get("on_error_action");
            let on_error_config: Option<String> = row.get("on_error_config");

            let version_id = target_version_id.or(active_version_id);
            if version_id.is_none() {
                println!("⚠️ [Redis Worker] Binding {} has no active version deployed. Skipping.", binding_id);
                continue;
            }
            let version_id = version_id.unwrap();

            if let Err(e) = self.process_stream_events(
                &stream_key,
                &consumer_group,
                queue_id,
                microservice_id,
                version_id,
                on_success_action.as_deref(),
                on_success_config.as_deref(),
                on_error_action.as_deref(),
                on_error_config.as_deref(),
            ).await {
                eprintln!("Failed to process stream {} for binding {}: {}", stream_key, binding_id, e);
            }
        }

        Ok(())
    }

    async fn process_stream_events(
        &self,
        stream_key: &str,
        consumer_group: &str,
        queue_id: i64,
        microservice_id: i64,
        version_id: i64,
        on_success_action: Option<&str>,
        on_success_config: Option<&str>,
        on_error_action: Option<&str>,
        on_error_config: Option<&str>,
    ) -> Result<(), String> {
        let mut conn = self.redis_pool.get().await.map_err(|e| e.to_string())?;

        // Ensure consumer group exists
        let _: redis::RedisResult<()> = conn.xgroup_create_mkstream(stream_key, consumer_group, "0").await;

        let consumer_name = "orchestrator_consumer_1";
        let opts = redis::streams::StreamReadOptions::default()
            .group(consumer_group, consumer_name)
            .count(1)
            .block(1000);

        let results = match conn.xread_options::<&str, &str, redis::streams::StreamReadReply>(&[stream_key], &[">"], &opts).await {
            Ok(res) => res,
            Err(e) => {
                if e.kind() == redis::ErrorKind::TypeError {
                    return Ok(()); // Empty stream read timeout
                }
                return Err(format!("XREADGROUP failed: {}", e));
            }
        };

        for stream in results.keys {
            for message in stream.ids {
                let msg_id = message.id;
                println!("📥 [Redis Worker] Message received from stream '{}': ID={}", stream_key, msg_id);

                // Parse payload from stream entry fields
                let mut payload_map = serde_json::Map::new();
                let mut nested_payloads = Vec::new();

                for (key, val) in message.map {
                    let str_val = match val {
                        redis::Value::Data(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                        redis::Value::Status(s) => s,
                        redis::Value::Int(i) => i.to_string(),
                        _ => format!("{:?}", val),
                    };

                    // Try parsing typed values
                    let json_val = if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&str_val) {
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
                    } else if let Ok(num) = str_val.parse::<i64>() {
                        json!(num)
                    } else if let Ok(f) = str_val.parse::<f64>() {
                        json!(f)
                    } else if let Ok(b) = str_val.parse::<bool>() {
                        json!(b)
                    } else {
                        json!(str_val)
                    };

                    payload_map.insert(key, json_val);
                }

                // Flatten nested payloads (e.g. "payload": { "a": 4, "b": 3 }) into the root map
                for nested in nested_payloads {
                    if let Some(obj) = nested.as_object() {
                        for (k, v) in obj {
                            // If it's a string representation of a number/bool, parse it
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

                let mut payload_input = serde_json::Value::Object(payload_map);
                if let Some(inner_payload) = payload_input.get("payload") {
                    if inner_payload.is_object() || inner_payload.is_array() {
                        payload_input = inner_payload.clone();
                    }
                }

                // Fetch container image info from SQLite
                let pool = &self.db_proxy.pool;
                let version_query = "SELECT container_image_tag FROM microservice_versions WHERE id = ?";
                let image_tag: Option<String> = sqlx::query(version_query)
                    .bind(version_id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| e.to_string())?
                    .and_then(|r| r.get("container_image_tag"));

                if let Some(img) = image_tag {
                    println!("🚀 [Redis Worker] Dispatching container execution for image '{}' with payload: {:?}", img, payload_input);
                    let start_time = Utc::now();

                    let env_vars = crate::db::resolve_microservice_env(pool, microservice_id, &payload_input)
                        .await
                        .unwrap_or_default();

                    // Dispatch execution to Docker with envs
                    let container_name = format!("runner_ms_{}", microservice_id);
                    let execution_result = self.docker_manager.run_container(&container_name, &img, &payload_input, Some(env_vars)).await;

                    let end_time = Utc::now();
                    let duration_ms = end_time.signed_duration_since(start_time).num_milliseconds();

                    let (status, payload_output, error_message) = match execution_result {
                        Ok((_, output)) => {
                            let mut is_panic = false;
                            let mut panic_msg = String::new();
                            if let Some(obj) = output.as_object() {
                                if let Some(raw_out) = obj.get("raw_output").and_then(|r| r.as_str()) {
                                    if raw_out.contains("panicked") {
                                        is_panic = true;
                                        panic_msg = raw_out.to_string();
                                    }
                                }
                            }
                            if is_panic {
                                ("error".to_string(), None, Some(panic_msg))
                            } else {
                                ("success".to_string(), Some(output), None)
                            }
                        }
                        Err(err) => ("error".to_string(), None, Some(err)),
                    };

                    // Persist log to execution_logs table
                    let payload_input_str = serde_json::to_string(&payload_input).unwrap_or_default();
                    let payload_output_str = payload_output.as_ref().map(|o| serde_json::to_string(&o).unwrap_or_default());
                    let created_at_now = Utc::now();

                    let insert_query = r#"
                        INSERT INTO execution_logs (
                            queue_id, microservice_id, version_id, stream_message_id, 
                            payload_input, payload_output, status, error_message, 
                            execution_time_ms, tags, created_at
                        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    "#;

                    let _ = sqlx::query(insert_query)
                        .bind(queue_id)
                        .bind(microservice_id)
                        .bind(version_id)
                        .bind(msg_id.clone())
                        .bind(payload_input_str)
                        .bind(payload_output_str)
                        .bind(status.clone())
                        .bind(error_message.clone())
                        .bind(duration_ms)
                        .bind("[]") // Default empty tag array
                        .bind(created_at_now)
                        .execute(pool)
                        .await;

                    // Route success or error actions
                    if status == "success" {
                        if let Some("queue") | Some("publish") = on_success_action {
                            if let Some(target_stream_raw) = on_success_config {
                                let target_stream = target_stream_raw.trim_matches('"');
                                if let Some(ref out_val) = payload_output {
                                    let out_str = serde_json::to_string(out_val).unwrap_or_default();
                                    println!("🔗 [Redis Worker] Success Action: Publishing output to stream '{}'", target_stream);
                                    let mut fields = vec![("payload".to_string(), out_str)];
                                    if let Some(obj) = out_val.as_object() {
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
                                    let _: redis::RedisResult<()> = conn.xadd(target_stream, "*", &fields_ref).await;
                                }
                            }
                        } else if let Some("key_event") = on_success_action {
                            if let Some(config_str) = on_success_config {
                                if let Some(ref out_val) = payload_output {
                                    if let Some((target_stream, payload_str)) = evaluate_key_event(out_val, config_str) {
                                        println!("🔗 [Redis Worker] Success Action (Key Event matched): Publishing to stream '{}'", target_stream);
                                        let mut fields = vec![("payload".to_string(), payload_str.clone())];
                                        if let Ok(parsed_val) = serde_json::from_str::<serde_json::Value>(&payload_str) {
                                            if let Some(obj) = parsed_val.as_object() {
                                                for (k, v) in obj {
                                                    let val_str = if v.is_string() {
                                                        v.as_str().unwrap().to_string()
                                                    } else {
                                                        v.to_string()
                                                    };
                                                    fields.push((k.clone(), val_str));
                                                }
                                            }
                                        }
                                        let fields_ref: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                                        let _: redis::RedisResult<()> = conn.xadd(target_stream, "*", &fields_ref).await;
                                    }
                                }
                            }
                        }
                    } else {
                        if let Some("queue") | Some("publish") = on_error_action {
                            if let Some(target_stream_raw) = on_error_config {
                                let target_stream = target_stream_raw.trim_matches('"');
                                let err_payload = json!({
                                    "error": error_message.as_ref().cloned().unwrap_or_default(),
                                    "input": payload_input
                                });
                                let err_str = serde_json::to_string(&err_payload).unwrap_or_default();
                                println!("🔗 [Redis Worker] Error Action: Publishing error info to stream '{}'", target_stream);
                                let mut fields = vec![("payload".to_string(), err_str)];
                                if let Some(obj) = err_payload.as_object() {
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
                                let _: redis::RedisResult<()> = conn.xadd(target_stream, "*", &fields_ref).await;
                            }
                        } else if let Some("key_event") = on_error_action {
                            if let Some(config_str) = on_error_config {
                                let err_payload = json!({
                                    "error": error_message.as_ref().cloned().unwrap_or_default(),
                                    "input": payload_input
                                });
                                if let Some((target_stream, payload_str)) = evaluate_key_event(&err_payload, config_str) {
                                    println!("🔗 [Redis Worker] Error Action (Key Event matched): Publishing to stream '{}'", target_stream);
                                    let mut fields = vec![("payload".to_string(), payload_str.clone())];
                                    if let Ok(parsed_val) = serde_json::from_str::<serde_json::Value>(&payload_str) {
                                        if let Some(obj) = parsed_val.as_object() {
                                            for (k, v) in obj {
                                                let val_str = if v.is_string() {
                                                    v.as_str().unwrap().to_string()
                                                } else {
                                                    v.to_string()
                                                };
                                                fields.push((k.clone(), val_str));
                                            }
                                        }
                                    }
                                    let fields_ref: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                                    let _: redis::RedisResult<()> = conn.xadd(target_stream, "*", &fields_ref).await;
                                }
                            }
                        }
                    }

                    println!("✅ [Redis Worker] Container finished in {}ms. Status: {}. ACK & XDEL stream message.", duration_ms, status);

                    // Acknowledge stream message
                    let _: redis::RedisResult<()> = conn.xack(stream_key, consumer_group, &[&msg_id]).await;
                    // Delete message from stream
                    let _: redis::RedisResult<()> = conn.xdel(stream_key, &[&msg_id]).await;
                } else {
                    println!("⚠️ [Redis Worker] No container image tag found for version: {}", version_id);
                }
            }
        }

        Ok(())
    }
}

fn evaluate_key_event(payload: &serde_json::Value, config_str: &str) -> Option<(String, String)> {
    let config: serde_json::Value = serde_json::from_str(config_str).ok()?;
    let key_path = config.get("key")?.as_str()?;
    let operator = config.get("operator")?.as_str()?;
    let val_str = config.get("value")?.as_str()?;
    let dest_queue = config.get("target_stream")
        .or_else(|| config.get("destination_queue"))?
        .as_str()?
        .to_string();

    let mut current = payload;
    for part in key_path.split('.') {
        if part.is_empty() { continue; }
        if let Some(next) = current.get(part) {
            current = next;
        } else {
            return None;
        }
    }

    let is_match = match current {
        serde_json::Value::String(s) => {
            match operator {
                "==" => s == val_str,
                "!=" => s != val_str,
                _ => {
                    if let (Ok(n1), Ok(n2)) = (s.parse::<f64>(), val_str.parse::<f64>()) {
                        match operator {
                            ">" => n1 > n2,
                            "<" => n1 < n2,
                            _ => false,
                        }
                    } else {
                        false
                    }
                }
            }
        }
        serde_json::Value::Number(n) => {
            if let Some(n1) = n.as_f64() {
                if let Ok(n2) = val_str.parse::<f64>() {
                    match operator {
                        "==" => n1 == n2,
                        "!=" => n1 != n2,
                        ">" => n1 > n2,
                        "<" => n1 < n2,
                        _ => false,
                    }
                } else {
                    false
                }
            } else {
                false
            }
        }
        serde_json::Value::Bool(b) => {
            if let Ok(b2) = val_str.parse::<bool>() {
                match operator {
                    "==" => *b == b2,
                    "!=" => *b != b2,
                    _ => false,
                }
            } else {
                false
            }
        }
        _ => false,
    };

    if is_match {
        Some((dest_queue, serde_json::to_string(payload).unwrap_or_default()))
    } else {
        None
    }
}

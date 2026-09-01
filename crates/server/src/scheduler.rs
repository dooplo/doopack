use std::sync::Arc;
use tokio::time::{sleep, Duration};
use sqlx::Row;
use chrono::Utc;
use crate::db::DbProxy;
use crate::docker_manager::DockerManager;

pub struct SchedulerManager {
    db_proxy: DbProxy,
    docker_manager: DockerManager,
}

impl SchedulerManager {
    pub fn new(db_proxy: DbProxy, docker_manager: DockerManager) -> Self {
        Self {
            db_proxy,
            docker_manager,
        }
    }

    pub fn start_scheduler_loop(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                if let Err(e) = self.process_scheduled_jobs().await {
                    eprintln!("Error in Scheduler loop: {}", e);
                }
                sleep(Duration::from_secs(5)).await;
            }
        });
    }

    async fn process_scheduled_jobs(&self) -> Result<(), String> {
        let pool = &self.db_proxy.pool;
        let now = Utc::now();

        // 1. Fetch pending scheduled jobs that are due
        let query = r#"
            SELECT id, microservice_id, payload, run_at, cron_expression 
            FROM scheduled_jobs 
            WHERE status = 'pending' AND run_at <= ?
        "#;
        
        let rows = sqlx::query(query)
            .bind(now)
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?;

        for r in rows {
            let job_id: i64 = r.get("id");
            let ms_id: i64 = r.get("microservice_id");
            let payload_str: String = r.get("payload");
            let cron_expression: Option<String> = r.get("cron_expression");
            let payload_input: serde_json::Value = serde_json::from_str(&payload_str).unwrap_or(serde_json::json!({}));

            println!("⏰ [Scheduler] Processing job ID {}, microservice ID {}", job_id, ms_id);

            // Fetch the active version of the microservice
            let version_query = r#"
                SELECT m.active_version_id, mv.container_image_tag
                FROM microservices m
                JOIN microservice_versions mv ON m.active_version_id = mv.id
                WHERE m.id = ? AND m.is_active = 1
            "#;

            let version_row = sqlx::query(version_query)
                .bind(ms_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;

            if let Some(vr) = version_row {
                let version_id: i64 = vr.get("active_version_id");
                let image_tag: Option<String> = vr.get("container_image_tag");

                if let Some(img) = image_tag {
                    if !img.is_empty() {
                        // Mark job as running to prevent double execution
                        let _ = sqlx::query("UPDATE scheduled_jobs SET status = 'running' WHERE id = ?")
                            .bind(job_id)
                            .execute(pool)
                            .await;

                        let start_time = Utc::now();
                        
                        // Resolve environment variables for the microservice
                        let adapted_payload = crate::adapt_payload(&payload_input);
                        let env_vars = crate::db::resolve_microservice_env(pool, ms_id, &adapted_payload)
                            .await
                            .unwrap_or_default();

                        // Execute container
                        let container_name = format!("runner_ms_{}", ms_id);
                        let execution_result = self.docker_manager.run_container(&container_name, &img, &adapted_payload, Some(env_vars)).await;
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
                                    ("failed".to_string(), None, Some(panic_msg))
                                } else {
                                    ("completed".to_string(), Some(output), None)
                                }
                            }
                            Err(err) => ("failed".to_string(), None, Some(err)),
                        };

                        // Update scheduled job status or reschedule if it's recurring (cron)
                        if let Some(ref cron_str) = cron_expression {
                            match parse_cron_and_next(cron_str) {
                                Ok(next_time) => {
                                    let _ = sqlx::query("UPDATE scheduled_jobs SET run_at = ?, status = 'pending' WHERE id = ?")
                                        .bind(next_time)
                                        .bind(job_id)
                                        .execute(pool)
                                        .await;
                                }
                                Err(e) => {
                                    let _ = sqlx::query("UPDATE scheduled_jobs SET status = 'failed' WHERE id = ?")
                                        .bind(job_id)
                                        .execute(pool)
                                        .await;
                                    eprintln!("⏰ [Scheduler] Cron parsing failed during rescheduling of job {}: {}", job_id, e);
                                }
                            }
                        } else {
                            let _ = sqlx::query("UPDATE scheduled_jobs SET status = ? WHERE id = ?")
                                .bind(&status)
                                .bind(job_id)
                                .execute(pool)
                                .await;
                        }

                        // Insert log into execution_logs table so it shows up in history (with queue_id = 0 and stream_message_id = "scheduled")
                        let payload_output_str = payload_output.as_ref().map(|o| serde_json::to_string(&o).unwrap_or_default());
                        let log_status = if status == "completed" { "success".to_string() } else { "error".to_string() };

                        let insert_query = r#"
                            INSERT INTO execution_logs (
                                queue_id, microservice_id, version_id, stream_message_id, 
                                payload_input, payload_output, status, error_message, 
                                execution_time_ms, tags, created_at
                            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                        "#;

                        let _ = sqlx::query(insert_query)
                            .bind(0i64) // Queue ID 0 is for scheduled
                            .bind(ms_id)
                            .bind(version_id)
                            .bind(format!("scheduled_{}", job_id))
                            .bind(payload_str)
                            .bind(payload_output_str)
                            .bind(log_status)
                            .bind(error_message)
                            .bind(duration_ms)
                            .bind("[]")
                            .bind(Utc::now())
                            .execute(pool)
                            .await;

                        println!("⏰ [Scheduler] Job ID {} finished with status: {}", job_id, status);
                        continue;
                    }
                }
            }

            // If we couldn't run it (no active version/image), mark as failed
            let _ = sqlx::query("UPDATE scheduled_jobs SET status = 'failed' WHERE id = ?")
                .bind(job_id)
                .execute(pool)
                .await;
        }

        Ok(())
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

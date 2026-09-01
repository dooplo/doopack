//! # Doopack SDK
//!
//! Official Rust client library for interacting with the **Doopack** event-driven platform and orchestrator.
//!
//! ## Features
//! - **Event Publishing**: Publish events directly into Redis Streams.
//! - **Event Scheduling**: Schedule jobs with delays, specific timestamps, or recurring Cron expressions.
//! - **Event & Execution Status**: Inspect execution logs, track real-time job status, and replay failed events.
//! - **Environment Variables CRUD**: Dynamically manage microservice configurations and environment variables.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use doopack_sdk::DoopackClient;
//! use serde_json::json;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 1. Initialize client
//!     let client = DoopackClient::new("http://localhost:4500")
//!         .with_token("your_jwt_token_or_api_key");
//!
//!     // 2. Publish an event to a stream
//!     let pub_res = client.publish("orders_created", &json!({
//!         "order_id": "12345",
//!         "amount": 99.90,
//!         "customer": "John Doe"
//!     })).await?;
//!     println!("Published message ID: {:?}", pub_res.message_id);
//!
//!     // 3. Schedule a job with a 60-second delay
//!     let scheduled = client.schedule_with_delay("orders-service", 60, &json!({
//!         "task": "send_order_confirmation_email",
//!         "order_id": "12345"
//!     })).await?;
//!     println!("Scheduled job ID: {:?}", scheduled.id);
//!
//!     // 4. Check event / execution log status
//!     if let Some(log_id) = scheduled.id {
//!         let log = client.get_event_status(&log_id).await?;
//!         println!("Execution status: {}", log.status);
//!     }
//!
//!     // 5. Manage environment variables
//!     let env = client.create_env(
//!         "orders-service",
//!         "production",
//!         &json!({ "SURREAL_NS": "prod", "SURREAL_DB": "ecommerce" }),
//!         true
//!     ).await?;
//!     println!("Created environment: {}", env.name);
//!
//!     Ok(())
//! }
//! ```

use std::env;
use std::time::Duration;
use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;

// Re-export common models from shared crate
pub use shared::{
    ApiKeyDTO, BindingDTO, DbPoolDTO, ExecutionLogDTO, LogFilterQuery, LogSearchResponse,
    MicroserviceDTO, MicroserviceEnvDTO, MicroserviceVersionDTO, PublishEventRequest,
    PublishEventResponse, QueueDTO, ScheduleJobRequest, ScheduledJobDTO, SystemHealth,
    SystemHealthResponse, UserDTO,
};

/// Errors returned by the Doopack SDK.
#[derive(Debug, Error)]
pub enum DoopackError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("API request failed with status {status}: {message}")]
    Api {
        status: u16,
        message: String,
    },

    #[error("Configuration or input error: {0}")]
    Config(String),

    #[error("Resource not found: {0}")]
    NotFound(String),
}

/// Builder for [`DoopackClient`].
#[derive(Debug, Clone)]
pub struct DoopackClientBuilder {
    endpoint: String,
    auth_token: Option<String>,
    api_key: Option<String>,
    timeout: Duration,
}

impl DoopackClientBuilder {
    /// Creates a new builder with the given Doopack API endpoint URL.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into().trim_end_matches('/').to_string(),
            auth_token: None,
            api_key: None,
            timeout: Duration::from_secs(30),
        }
    }

    /// Sets the JWT Bearer token for authentication.
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    /// Sets the API key for authentication (`x-api-key`).
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Sets the HTTP request timeout.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Builds the [`DoopackClient`].
    pub fn build(self) -> Result<DoopackClient, DoopackError> {
        let mut client_builder = reqwest::Client::builder()
            .timeout(self.timeout);

        let mut headers = HeaderMap::new();
        if let Some(ref token) = self.auth_token {
            let auth_val = if token.starts_with("Bearer ") {
                token.clone()
            } else {
                format!("Bearer {}", token)
            };
            if let Ok(val) = HeaderValue::from_str(&auth_val) {
                headers.insert(AUTHORIZATION, val);
            }
        }

        if let Some(ref key) = self.api_key {
            if let Ok(val) = HeaderValue::from_str(key) {
                headers.insert("x-api-key", val);
            }
        }

        client_builder = client_builder.default_headers(headers);
        let http_client = client_builder.build()?;

        Ok(DoopackClient {
            endpoint: self.endpoint,
            client: http_client,
            auth_token: self.auth_token,
            api_key: self.api_key,
        })
    }
}

/// Client for interacting with the Doopack platform API.
#[derive(Debug, Clone)]
pub struct DoopackClient {
    endpoint: String,
    client: reqwest::Client,
    auth_token: Option<String>,
    api_key: Option<String>,
}

impl DoopackClient {
    /// Creates a new `DoopackClient` pointing to `endpoint` (e.g. `http://localhost:4500`).
    pub fn new(endpoint: impl Into<String>) -> Self {
        DoopackClientBuilder::new(endpoint)
            .build()
            .unwrap_or_else(|_| Self {
                endpoint: "http://localhost:4500".to_string(),
                client: reqwest::Client::new(),
                auth_token: None,
                api_key: None,
            })
    }

    /// Creates a builder to customize timeout, headers, and credentials.
    pub fn builder(endpoint: impl Into<String>) -> DoopackClientBuilder {
        DoopackClientBuilder::new(endpoint)
    }

    /// Initializes a client from environment variables:
    /// - `DOOPACK_ENDPOINT` (defaults to `http://localhost:4500`)
    /// - `DOOPACK_TOKEN` (optional JWT token)
    /// - `DOOPACK_API_KEY` (optional API key)
    pub fn from_env() -> Result<Self, DoopackError> {
        let endpoint = env::var("DOOPACK_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:4500".to_string());
        
        let mut builder = DoopackClientBuilder::new(endpoint);
        if let Ok(tok) = env::var("DOOPACK_TOKEN") {
            builder = builder.token(tok);
        }
        if let Ok(key) = env::var("DOOPACK_API_KEY") {
            builder = builder.api_key(key);
        }
        builder.build()
    }

    /// Returns a new client with the specified Bearer token.
    pub fn with_token(self, token: impl Into<String>) -> Self {
        let tok = token.into();
        let mut builder = DoopackClientBuilder::new(&self.endpoint)
            .token(tok);
        if let Some(ref key) = self.api_key {
            builder = builder.api_key(key);
        }
        builder.build().unwrap_or(self)
    }

    /// Returns a new client with the specified API key.
    pub fn with_api_key(self, api_key: impl Into<String>) -> Self {
        let key = api_key.into();
        let mut builder = DoopackClientBuilder::new(&self.endpoint)
            .api_key(key);
        if let Some(ref tok) = self.auth_token {
            builder = builder.token(tok);
        }
        builder.build().unwrap_or(self)
    }

    // Helper for sending and deserializing JSON requests
    async fn request<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<T, DoopackError> {
        let url = format!("{}{}", self.endpoint, path);
        let mut req = self.client.request(method, &url);

        if let Some(json_body) = body {
            req = req.json(&json_body);
        }

        let resp = req.send().await?;
        let status = resp.status();

        if status.is_success() {
            let parsed = resp.json::<T>().await?;
            Ok(parsed)
        } else if status == reqwest::StatusCode::NOT_FOUND {
            let text = resp.text().await.unwrap_or_default();
            Err(DoopackError::NotFound(format!("{} - {}", url, text)))
        } else {
            let text = resp.text().await.unwrap_or_default();
            let err_msg = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(|s| s.to_string()))
                .unwrap_or(text);

            Err(DoopackError::Api {
                status: status.as_u16(),
                message: err_msg,
            })
        }
    }

    // =========================================================================
    // a) Event Publishing (Redis Streams)
    // =========================================================================

    /// Publishes an event to a specific Redis Stream.
    ///
    /// # Arguments
    /// * `stream_key` - Target Redis stream key name (e.g. `"orders_created"`).
    /// * `payload` - Any serializable data payload.
    ///
    /// # Example
    /// ```rust,no_run
    /// # use doopack_sdk::DoopackClient;
    /// # use serde_json::json;
    /// # async fn run(client: &DoopackClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let res = client.publish_event("orders_created", &json!({
    ///     "order_id": 1024,
    ///     "amount": 250.00
    /// })).await?;
    /// println!("Published message ID: {:?}", res.message_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn publish_event<T: Serialize>(
        &self,
        stream_key: &str,
        payload: &T,
    ) -> Result<PublishEventResponse, DoopackError> {
        let payload_json = serde_json::to_value(payload)?;
        let req_body = PublishEventRequest {
            stream_key: stream_key.to_string(),
            payload: payload_json,
        };

        self.request(
            reqwest::Method::POST,
            "/api/v1/events/publish",
            Some(serde_json::to_value(req_body)?),
        ).await
    }

    /// Shorthand alias for [`publish_event`].
    pub async fn publish<T: Serialize>(
        &self,
        stream_key: &str,
        payload: &T,
    ) -> Result<PublishEventResponse, DoopackError> {
        self.publish_event(stream_key, payload).await
    }

    // =========================================================================
    // b) Event Scheduling (Schedules)
    // =========================================================================

    /// Schedules a microservice execution using a full [`ScheduleJobRequest`].
    pub async fn schedule_job(
        &self,
        microservice_id: &str,
        request: &ScheduleJobRequest,
    ) -> Result<ScheduledJobDTO, DoopackError> {
        let path = format!("/api/v1/services/{}/schedule", microservice_id);
        self.request(
            reqwest::Method::POST,
            &path,
            Some(serde_json::to_value(request)?),
        ).await
    }

    /// Schedules a microservice execution with a delay in seconds.
    ///
    /// # Example
    /// ```rust,no_run
    /// # use doopack_sdk::DoopackClient;
    /// # use serde_json::json;
    /// # async fn run(client: &DoopackClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let job = client.schedule_with_delay(
    ///     "notification-service",
    ///     300, // run in 5 minutes
    ///     &json!({ "user_id": 42, "template": "welcome" })
    /// ).await?;
    /// println!("Job ID: {:?}", job.id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn schedule_with_delay<T: Serialize>(
        &self,
        microservice_id: &str,
        delay_seconds: i64,
        payload: &T,
    ) -> Result<ScheduledJobDTO, DoopackError> {
        let req = ScheduleJobRequest {
            run_at: None,
            delay_seconds: Some(delay_seconds),
            cron_expression: None,
            payload: serde_json::to_value(payload)?,
        };
        self.schedule_job(microservice_id, &req).await
    }

    /// Schedules a microservice execution at a specific UTC date and time.
    pub async fn schedule_at<T: Serialize>(
        &self,
        microservice_id: &str,
        run_at: DateTime<Utc>,
        payload: &T,
    ) -> Result<ScheduledJobDTO, DoopackError> {
        let req = ScheduleJobRequest {
            run_at: Some(run_at),
            delay_seconds: None,
            cron_expression: None,
            payload: serde_json::to_value(payload)?,
        };
        self.schedule_job(microservice_id, &req).await
    }

    /// Schedules a recurring microservice execution using a Cron expression.
    ///
    /// # Example
    /// ```rust,no_run
    /// # use doopack_sdk::DoopackClient;
    /// # use serde_json::json;
    /// # async fn run(client: &DoopackClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let job = client.schedule_cron(
    ///     "report-service",
    ///     "0 0 * * *", // Every midnight
    ///     &json!({ "type": "daily_summary" })
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn schedule_cron<T: Serialize>(
        &self,
        microservice_id: &str,
        cron_expression: &str,
        payload: &T,
    ) -> Result<ScheduledJobDTO, DoopackError> {
        let req = ScheduleJobRequest {
            run_at: None,
            delay_seconds: None,
            cron_expression: Some(cron_expression.to_string()),
            payload: serde_json::to_value(payload)?,
        };
        self.schedule_job(microservice_id, &req).await
    }

    /// Lists all active and scheduled jobs across the platform.
    pub async fn list_schedules(&self) -> Result<Vec<ScheduledJobDTO>, DoopackError> {
        self.request(reqwest::Method::GET, "/api/v1/schedules", None).await
    }

    /// Deletes or cancels a scheduled job by its ID.
    pub async fn delete_schedule(&self, schedule_id: &str) -> Result<(), DoopackError> {
        let path = format!("/api/v1/schedules/{}", schedule_id);
        let _: serde_json::Value = self.request(reqwest::Method::DELETE, &path, None).await?;
        Ok(())
    }

    // =========================================================================
    // c) Event & Execution Status / Logs
    // =========================================================================

    /// Retrieves execution status and output logs for an event by execution log ID.
    ///
    /// # Example
    /// ```rust,no_run
    /// # use doopack_sdk::DoopackClient;
    /// # async fn run(client: &DoopackClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let log = client.get_event_status("123").await?;
    /// println!("Status: {}, Time: {}ms", log.status, log.execution_time_ms);
    /// if let Some(output) = log.payload_output {
    ///     println!("Output: {}", output);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_event_status(&self, log_id: &str) -> Result<ExecutionLogDTO, DoopackError> {
        let path = format!("/api/v1/logs/{}", log_id);
        self.request(reqwest::Method::GET, &path, None).await
    }

    /// Shorthand alias for [`get_event_status`].
    pub async fn get_log(&self, log_id: &str) -> Result<ExecutionLogDTO, DoopackError> {
        self.get_event_status(log_id).await
    }

    /// Searches and filters execution logs by date, status, tags, queue, or search term.
    pub async fn search_logs(&self, query: &LogFilterQuery) -> Result<LogSearchResponse, DoopackError> {
        self.request(
            reqwest::Method::POST,
            "/api/v1/logs/search",
            Some(serde_json::to_value(query)?),
        ).await
    }

    /// Re-publishes/re-executes a previously processed event payload.
    pub async fn resend_log(&self, log_id: &str) -> Result<(), DoopackError> {
        let path = format!("/api/v1/logs/{}/resend", log_id);
        let _: serde_json::Value = self.request(reqwest::Method::POST, &path, None).await?;
        Ok(())
    }

    /// Deletes an execution log entry.
    pub async fn delete_log(&self, log_id: &str) -> Result<(), DoopackError> {
        let path = format!("/api/v1/logs/{}", log_id);
        let _: serde_json::Value = self.request(reqwest::Method::DELETE, &path, None).await?;
        Ok(())
    }

    // =========================================================================
    // d) CRUD of Environment Variables (Envs)
    // =========================================================================

    /// Lists all environment variable configurations for a microservice.
    pub async fn list_envs(&self, microservice_id: &str) -> Result<Vec<MicroserviceEnvDTO>, DoopackError> {
        let path = format!("/api/v1/services/{}/envs", microservice_id);
        self.request(reqwest::Method::GET, &path, None).await
    }

    /// Retrieves a specific environment configuration by ID.
    pub async fn get_env(
        &self,
        microservice_id: &str,
        env_id: &str,
    ) -> Result<MicroserviceEnvDTO, DoopackError> {
        let path = format!("/api/v1/services/{}/envs/{}", microservice_id, env_id);
        self.request(reqwest::Method::GET, &path, None).await
    }

    /// Creates a new environment variable set for a microservice.
    ///
    /// # Example
    /// ```rust,no_run
    /// # use doopack_sdk::DoopackClient;
    /// # use serde_json::json;
    /// # async fn run(client: &DoopackClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let env = client.create_env(
    ///     "orders-service",
    ///     "staging",
    ///     &json!({ "API_BASE_URL": "https://staging.api.com", "DEBUG": "true" }),
    ///     false
    /// ).await?;
    /// println!("Created env ID: {:?}", env.id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_env<T: Serialize>(
        &self,
        microservice_id: &str,
        name: &str,
        config: &T,
        is_default: bool,
    ) -> Result<MicroserviceEnvDTO, DoopackError> {
        let path = format!("/api/v1/services/{}/envs", microservice_id);
        let payload = MicroserviceEnvDTO {
            id: None,
            microservice_id: Some(microservice_id.to_string()),
            name: name.to_string(),
            config: serde_json::to_value(config)?,
            is_default,
        };

        self.request(
            reqwest::Method::POST,
            &path,
            Some(serde_json::to_value(payload)?),
        ).await
    }

    /// Updates an existing environment configuration.
    pub async fn update_env<T: Serialize>(
        &self,
        microservice_id: &str,
        env_id: &str,
        name: &str,
        config: &T,
        is_default: bool,
    ) -> Result<MicroserviceEnvDTO, DoopackError> {
        let path = format!("/api/v1/services/{}/envs/{}", microservice_id, env_id);
        let payload = MicroserviceEnvDTO {
            id: Some(env_id.to_string()),
            microservice_id: Some(microservice_id.to_string()),
            name: name.to_string(),
            config: serde_json::to_value(config)?,
            is_default,
        };

        self.request(
            reqwest::Method::PUT,
            &path,
            Some(serde_json::to_value(payload)?),
        ).await
    }

    /// Deletes an environment configuration for a microservice.
    pub async fn delete_env(
        &self,
        microservice_id: &str,
        env_id: &str,
    ) -> Result<(), DoopackError> {
        let path = format!("/api/v1/services/{}/envs/{}", microservice_id, env_id);
        let _: serde_json::Value = self.request(reqwest::Method::DELETE, &path, None).await?;
        Ok(())
    }

    // =========================================================================
    // System & General Platform Endpoints
    // =========================================================================

    /// Checks the health and system telemetry of the Doopack orchestrator.
    pub async fn system_health(&self) -> Result<SystemHealthResponse, DoopackError> {
        self.request(reqwest::Method::GET, "/api/v1/system/health", None).await
    }

    /// Lists all microservices registered on the platform.
    pub async fn list_services(&self) -> Result<Vec<MicroserviceDTO>, DoopackError> {
        self.request(reqwest::Method::GET, "/api/v1/services", None).await
    }

    /// Lists all Redis queues and stream keys configured in Doopack.
    pub async fn list_queues(&self) -> Result<Vec<QueueDTO>, DoopackError> {
        self.request(reqwest::Method::GET, "/api/v1/queues", None).await
    }

    /// Lists all event stream bindings connecting queues to microservices.
    pub async fn list_bindings(&self) -> Result<Vec<BindingDTO>, DoopackError> {
        self.request(reqwest::Method::GET, "/api/v1/bindings", None).await
    }
}

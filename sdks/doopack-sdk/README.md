# Doopack Rust SDK (`doopack-sdk`)

The official Rust client library for interacting with the **Doopack** event-driven microservices platform and orchestrator.

## Installation

Add `doopack-sdk` to your `Cargo.toml`:

```toml
[dependencies]
doopack-sdk = { path = "path/to/doopack/sdks/doopack-sdk" } # Or version "0.1.0"
serde_json = "1.0"
tokio = { version = "1.0", features = ["full"] }
```

---

## Capabilities

### 1. Initialize Client
```rust
use doopack_sdk::DoopackClient;

// Connect with endpoint URL and optional JWT token or API Key
let client = DoopackClient::new("http://localhost:4500")
    .with_token("your_jwt_auth_token");
```

---

### a) Publish an Event (Redis Streams)
Publish events directly to any Redis stream configured in Doopack:

```rust
use serde_json::json;

let response = client.publish("orders_created", &json!({
    "order_id": "ORD-98213",
    "customer_email": "client@example.com",
    "amount": 149.99,
    "currency": "BRL"
})).await?;

println!("Event published! Message ID: {:?}", response.message_id);
```

---

### b) Schedule an Event (Delay / Timestamp / Cron)
Schedule future executions of microservices:

```rust
// Schedule with a 60-second delay
let job = client.schedule_with_delay("email-service", 60, &json!({
    "template": "welcome_email",
    "user_id": "user_42"
})).await?;

// Schedule with a recurring Cron expression (e.g. daily at 9:00 AM)
let cron_job = client.schedule_cron("report-service", "0 9 * * *", &json!({
    "report": "daily_revenue"
})).await?;

// List all scheduled jobs
let schedules = client.list_schedules().await?;

// Cancel a scheduled job
client.delete_schedule(&job.id.unwrap()).await?;
```

---

### c) Check Event & Execution Status (Logs & Monitoring)
Track the processing lifecycle, inspect input/output payloads, and replay failed events:

```rust
// Query event execution status by log ID
let log = client.get_event_status("105").await?;
println!("Execution status: {}", log.status); // "success" | "error" | "timeout" | "panic"
println!("Duration: {} ms", log.execution_time_ms);
println!("Output: {:?}", log.payload_output);

// Search logs with filters
use doopack_sdk::LogFilterQuery;

let results = client.search_logs(&LogFilterQuery {
    microservice_id: Some("orders-service".to_string()),
    queue_id: None,
    status: Some("error".to_string()),
    tags: None,
    start_date: None,
    end_date: None,
    min_duration_ms: None,
    max_duration_ms: None,
    search_term: None,
    page: 1,
    limit: 20,
}).await?;

// Re-send / retry a failed event
client.resend_log("105").await?;
```

---

### d) Manage Microservice Environment Variables (CRUD)
Dynamically configure environments (e.g., staging, production):

```rust
// List environments
let envs = client.list_envs("orders-service").await?;

// Create new environment configuration
let new_env = client.create_env(
    "orders-service",
    "production",
    &json!({
        "DB_POOL_SURREAL": "wss://cloud.surrealdb.com/rpc",
        "SURREAL_NS": "prod",
        "SURREAL_DB": "ecommerce"
    }),
    true // is_default
).await?;

// Update environment
let updated_env = client.update_env(
    "orders-service",
    &new_env.id.unwrap(),
    "production",
    &json!({ "SURREAL_NS": "prod_v2" }),
    true
).await?;

// Delete environment
client.delete_env("orders-service", &updated_env.id.unwrap()).await?;
```

---

## Complete Example

See `examples/usage.rs` for a runnable end-to-end example.

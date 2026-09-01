use doopack_sdk::DoopackClient;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Doopack SDK Example ===");

    // 1. Initialize client
    let client = DoopackClient::new("http://localhost:4500")
        .with_token("test-token");

    // 2. Publish an event
    println!("\n1. Publishing event to 'orders_created'...");
    match client.publish("orders_created", &json!({
        "order_id": "ORD-1001",
        "customer": "weliton",
        "amount": 199.90
    })).await {
        Ok(res) => println!("   -> Success! Message ID: {:?}", res.message_id),
        Err(e) => println!("   -> Error publishing: {}", e),
    }

    // 3. Schedule a job with a 30-second delay
    println!("\n2. Scheduling job for 'email-service' with 30s delay...");
    match client.schedule_with_delay("email-service", 30, &json!({
        "order_id": "ORD-1001",
        "template": "invoice_email"
    })).await {
        Ok(job) => println!("   -> Job scheduled! ID: {:?}", job.id),
        Err(e) => println!("   -> Error scheduling: {}", e),
    }

    // 4. Check event status
    println!("\n3. Querying event execution status...");
    match client.get_event_status("1").await {
        Ok(log) => println!("   -> Status: {}, Execution Time: {}ms", log.status, log.execution_time_ms),
        Err(e) => println!("   -> Status check (expected if ID 1 not found): {}", e),
    }

    // 5. CRUD of Environment Variables
    println!("\n4. Managing Environment Variables...");
    match client.create_env(
        "email-service",
        "production",
        &json!({
            "SMTP_HOST": "smtp.mailgun.org",
            "SMTP_PORT": "587"
        }),
        true
    ).await {
        Ok(env) => {
            println!("   -> Created Env: {} (ID: {:?})", env.name, env.id);
            if let Some(ref env_id) = env.id {
                let _ = client.delete_env("email-service", env_id).await;
                println!("   -> Cleaned up created env.");
            }
        }
        Err(e) => println!("   -> Error managing envs: {}", e),
    }

    println!("\n=== Done! ===");
    Ok(())
}

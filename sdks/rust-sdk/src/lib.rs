use std::env;
use serde::Serialize;
use serde_json::Value;

pub use surrealdb;
pub use surrealdb::types::RecordId;
pub use surrealdb::types::SurrealValue;

/// Retrieves the input payload passed to the microservice by the orchestrator.
/// This reads the `PAYLOAD_INPUT` environment variable and parses it as JSON.
pub fn get_input() -> Result<Value, String> {
    let input_str = env::var("PAYLOAD_INPUT")
        .map_err(|e| format!("PAYLOAD_INPUT environment variable not found: {}", e))?;
    serde_json::from_str(&input_str)
        .map_err(|e| format!("Failed to parse PAYLOAD_INPUT JSON: {}", e))
}

/// Sends the output of the microservice execution back to the orchestrator.
/// This prints the serialized output to stdout, which is captured by the orchestrator.
pub fn send_output<T: Serialize>(output: &T) {
    if let Ok(json_str) = serde_json::to_string(output) {
        println!("{}", json_str);
    } else {
        println!("{{ \"error\": \"Failed to serialize output\" }}");
    }
}

/// Retrieves the connection URL of a database pool configured in the orchestrator.
pub fn get_db_pool(name: &str) -> Result<String, String> {
    let env_var_name = format!("DB_POOL_{}", name.to_uppercase().replace('-', "_"));
    env::var(&env_var_name)
        .map_err(|e| format!("Database pool '{}' not found in environment: {}", name, e))
}

/// Connects to SurrealDB using the pool credentials configured in the orchestrator,
/// automatically selecting the namespace and database.
pub async fn connect_surreal(name: &str) -> Result<surrealdb::Surreal<surrealdb::engine::any::Any>, String> {
    let prefix = format!("DB_POOL_{}", name.to_uppercase().replace('-', "_"));
    let mut url = env::var(&prefix)
        .map_err(|e| format!("Database pool '{}' not found in environment: {}", name, e))?;
    
    // Self-healing: if url starts with wss:// or https:// and has no port, append :443
    if url.starts_with("wss://") || url.starts_with("https://") {
        if let Some(idx) = url.find("://") {
            let host_part = &url[idx + 3..];
            if !host_part.contains(':') {
                url = format!("{}:443", url);
            }
        }
    }
    
    let db = surrealdb::engine::any::connect(&url).await
        .map_err(|e| format!("Failed to connect to SurrealDB at '{}': {}", url, e))?;
    
    let ns_key = format!("{}_NS", prefix);
    let db_key = format!("{}_DB", prefix);
    let user_key = format!("{}_USER", prefix);
    let pass_key = format!("{}_PASS", prefix);
    
    if let (Ok(user), Ok(pass)) = (env::var(&user_key), env::var(&pass_key)) {
        let ns = env::var(&ns_key).unwrap_or_default();
        let db_name = env::var(&db_key).unwrap_or_default();
        
        // Try Root sign-in first, fallback to Database sign-in if it fails
        let root_creds = surrealdb::opt::auth::Root {
            username: user.clone(),
            password: pass.clone(),
        };
        
        if let Err(_) = db.signin(root_creds).await {
            let db_creds = surrealdb::opt::auth::Database {
                namespace: ns,
                database: db_name,
                username: user,
                password: pass,
            };
            db.signin(db_creds).await
                .map_err(|e| format!("Failed to sign in to SurrealDB: {}", e))?;
        }
    }
    
    if let (Ok(ns), Ok(database)) = (env::var(&ns_key), env::var(&db_key)) {
        db.use_ns(&ns).use_db(&database).await
            .map_err(|e| format!("Failed to select namespace '{}' or database '{}': {}", ns, database, e))?;
    }
    
    Ok(db)
}

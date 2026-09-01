use sqlx::{sqlite::SqliteConnectOptions, SqlitePool, Row};
use std::str::FromStr;

#[derive(Clone)]
pub struct DbProxy {
    pub pool: SqlitePool,
}

impl DbProxy {
    pub async fn new() -> Result<Self, sqlx::Error> {
        let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://orchestrator.db".to_string());

        let options = SqliteConnectOptions::from_str(&db_url)?
            .create_if_missing(true);

        let pool = SqlitePool::connect_with(options).await?;

        // Initialize schema
        Self::init_schema(&pool).await?;

        Ok(Self { pool })
    }

    async fn init_schema(pool: &SqlitePool) -> Result<(), sqlx::Error> {
        // Users Table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                email TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )"
        ).execute(pool).await?;

        // DB Pools Table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS db_pools (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT UNIQUE NOT NULL,
                engine TEXT NOT NULL,
                connection_url TEXT NOT NULL,
                auth_namespace TEXT,
                auth_database TEXT,
                auth_username TEXT,
                auth_password TEXT,
                max_connections INTEGER DEFAULT 10,
                tags TEXT,
                is_active BOOLEAN DEFAULT 1,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )"
        ).execute(pool).await?;

        // Microservices Table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS microservices (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                uuid TEXT UNIQUE,
                name TEXT UNIQUE NOT NULL,
                language TEXT NOT NULL,
                description TEXT,
                tags TEXT,
                active_version_id INTEGER,
                on_success_action TEXT DEFAULT 'end',
                on_success_config TEXT,
                on_error_action TEXT DEFAULT 'end',
                on_error_config TEXT,
                is_active BOOLEAN DEFAULT 1,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )"
        ).execute(pool).await?;

        let _ = sqlx::query("ALTER TABLE db_pools ADD COLUMN is_active BOOLEAN DEFAULT 1").execute(pool).await;
        let _ = sqlx::query("ALTER TABLE microservices ADD COLUMN is_active BOOLEAN DEFAULT 1").execute(pool).await;
        let _ = sqlx::query("ALTER TABLE microservices ADD COLUMN on_success_action TEXT DEFAULT 'end'").execute(pool).await;
        let _ = sqlx::query("ALTER TABLE microservices ADD COLUMN on_success_config TEXT").execute(pool).await;
        let _ = sqlx::query("ALTER TABLE microservices ADD COLUMN on_error_action TEXT DEFAULT 'end'").execute(pool).await;
        let _ = sqlx::query("ALTER TABLE microservices ADD COLUMN on_error_config TEXT").execute(pool).await;
        let _ = sqlx::query("ALTER TABLE microservices ADD COLUMN uuid TEXT").execute(pool).await;
        let _ = sqlx::query("ALTER TABLE microservice_versions ADD COLUMN error_message TEXT").execute(pool).await;

        // Populate UUIDs for existing microservices that do not have one
        if let Ok(rows) = sqlx::query("SELECT id FROM microservices WHERE uuid IS NULL OR uuid = ''").fetch_all(pool).await {
            for r in rows {
                use rand::Rng;
                let ms_id: i64 = r.get("id");
                let mut rng = rand::thread_rng();
                let bytes: [u8; 16] = rng.gen();
                let generated_uuid = format!(
                    "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                    bytes[0], bytes[1], bytes[2], bytes[3],
                    bytes[4], bytes[5],
                    bytes[6], bytes[7],
                    bytes[8], bytes[9],
                    bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
                );
                let _ = sqlx::query("UPDATE microservices SET uuid = ? WHERE id = ?")
                    .bind(generated_uuid)
                    .bind(ms_id)
                    .execute(pool)
                    .await;
            }
        }

        // Microservice Versions Table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS microservice_versions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                microservice_id INTEGER NOT NULL,
                version_number INTEGER NOT NULL,
                version_tag TEXT NOT NULL,
                source_type TEXT NOT NULL,
                source_code TEXT NOT NULL,
                container_image_tag TEXT,
                container_id TEXT,
                status TEXT DEFAULT 'draft',
                changelog TEXT,
                error_message TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(microservice_id, version_number)
            )"
        ).execute(pool).await?;

        // Queues Table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS queues (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                stream_key TEXT UNIQUE NOT NULL,
                consumer_group TEXT DEFAULT 'orchestrator_group',
                is_active BOOLEAN DEFAULT 1,
                tags TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )"
        ).execute(pool).await?;

        // Bindings Table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS bindings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                queue_id INTEGER NOT NULL,
                microservice_id INTEGER NOT NULL,
                target_version_id INTEGER,
                on_success_action TEXT,
                on_success_config TEXT,
                on_error_action TEXT,
                on_error_config TEXT,
                is_active BOOLEAN DEFAULT 1,
                UNIQUE(queue_id, microservice_id)
            )"
        ).execute(pool).await?;

        // Execution Logs Table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS execution_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                queue_id INTEGER NOT NULL,
                microservice_id INTEGER NOT NULL,
                version_id INTEGER,
                stream_message_id TEXT NOT NULL,
                payload_input TEXT NOT NULL,
                payload_output TEXT,
                status TEXT NOT NULL,
                error_message TEXT,
                execution_time_ms INTEGER NOT NULL,
                tags TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )"
        ).execute(pool).await?;

        // Microservice Envs Table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS microservice_envs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                microservice_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                config TEXT NOT NULL,
                is_default BOOLEAN DEFAULT 0,
                UNIQUE(microservice_id, name)
            )"
        ).execute(pool).await?;

        // API Keys Table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS api_keys (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                key_value TEXT NOT NULL UNIQUE,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )"
        ).execute(pool).await?;

        // Scheduled Jobs Table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS scheduled_jobs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                microservice_id INTEGER NOT NULL,
                payload TEXT NOT NULL,
                run_at DATETIME NOT NULL,
                status TEXT DEFAULT 'pending',
                cron_expression TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )"
        ).execute(pool).await?;

        let _ = sqlx::query("ALTER TABLE scheduled_jobs ADD COLUMN cron_expression TEXT").execute(pool).await;

        // Seed default user if no users exist
        let count: i32 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(pool)
            .await?;
        if count == 0 {
            println!("🌱 Database created/initialized for the first time. Seeding default user weliton@dooplo.com");
            let password_hash = crate::auth::hash_password("Stowe@283").map_err(|e| sqlx::Error::Protocol(e))?;
            sqlx::query("INSERT INTO users (email, password_hash) VALUES (?, ?)")
                .bind("weliton@dooplo.com")
                .bind(password_hash)
                .execute(pool)
                .await?;
        }

        Ok(())
    }
}

pub async fn resolve_microservice_env(
    pool: &sqlx::SqlitePool,
    microservice_id: i64,
    payload_input: &serde_json::Value,
) -> Result<std::collections::HashMap<String, String>, String> {
    use sqlx::Row;
    let env_name_opt = payload_input
        .get("doopack")
        .and_then(|d| d.get("env"))
        .and_then(|e| e.as_str());

    let row = match env_name_opt {
        Some(name) => {
            sqlx::query("SELECT config FROM microservice_envs WHERE microservice_id = ? AND name = ?")
                .bind(microservice_id)
                .bind(name)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?
        }
        None => {
            sqlx::query("SELECT config FROM microservice_envs WHERE microservice_id = ? AND is_default = 1")
                .bind(microservice_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?
        }
    };

    let config_str = match row {
        Some(r) => r.get::<String, _>("config"),
        None => {
            let fallback_row = sqlx::query("SELECT config FROM microservice_envs WHERE microservice_id = ? LIMIT 1")
                .bind(microservice_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
            match fallback_row {
                Some(r) => r.get::<String, _>("config"),
                None => "{}".to_string(),
            }
        }
    };

    let mut config_map: std::collections::HashMap<String, String> = serde_json::from_str(&config_str)
        .unwrap_or_default();

    // Query active DB Pools and inject them as environment variables
    if let Ok(db_rows) = sqlx::query("SELECT name, connection_url, auth_namespace, auth_database, auth_username, auth_password FROM db_pools WHERE is_active = 1")
        .fetch_all(pool)
        .await
    {
        for r in db_rows {
            let name: String = r.get("name");
            let prefix = format!("DB_POOL_{}", name.to_uppercase().replace('-', "_"));
            
            config_map.insert(prefix.clone(), r.get::<String, _>("connection_url"));
            if let Ok(ns) = r.try_get::<String, _>("auth_namespace") {
                if !ns.is_empty() {
                    config_map.insert(format!("{}_NS", prefix), ns);
                }
            }
            if let Ok(db) = r.try_get::<String, _>("auth_database") {
                if !db.is_empty() {
                    config_map.insert(format!("{}_DB", prefix), db);
                }
            }
            if let Ok(user) = r.try_get::<String, _>("auth_username") {
                if !user.is_empty() {
                    config_map.insert(format!("{}_USER", prefix), user);
                }
            }
            if let Ok(pass) = r.try_get::<String, _>("auth_password") {
                if !pass.is_empty() {
                    config_map.insert(format!("{}_PASS", prefix), pass);
                }
            }
        }
    }

    Ok(config_map)
}

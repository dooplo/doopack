use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use chrono::{Utc, Duration};

const JWT_SECRET: &[u8] = b"super-secret-orchestrator-key-change-this-in-production";

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user_id or email
    pub exp: i64,
}

pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| e.to_string())?
        .to_string();
    Ok(password_hash)
}

pub fn verify_password(hash: &str, password: &str) -> bool {
    if let Ok(parsed_hash) = PasswordHash::new(hash) {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok()
    } else {
        false
    }
}

pub fn generate_token(user_id: &str) -> Result<String, String> {
    let expiration = Utc::now()
        .checked_add_signed(Duration::minutes(20))
        .expect("valid timestamp")
        .timestamp();

    let claims = Claims {
        sub: user_id.to_owned(),
        exp: expiration,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(JWT_SECRET),
    )
    .map_err(|e| e.to_string())
}

pub fn verify_token(token: &str) -> Result<Claims, String> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(JWT_SECRET),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_hashing_and_verification() {
        let password = "my-secure-password";
        let hash = hash_password(password).expect("hashing should work");
        assert!(verify_password(&hash, password));
        assert!(!verify_password(&hash, "wrong-password"));
    }

    #[test]
    fn test_jwt_generation_and_validation() {
        let user_id = "user_123";
        let token = generate_token(user_id).expect("token generation should work");
        let claims = verify_token(&token).expect("verification should succeed");
        assert_eq!(claims.sub, user_id);
    }
}

pub struct AuthenticatedUser {
    pub user_id: String,
}

impl<S> axum::extract::FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
    sqlx::SqlitePool: axum::extract::FromRef<S>,
{
    type Rejection = (axum::http::StatusCode, axum::Json<serde_json::Value>);

    async fn from_request_parts(parts: &mut axum::http::request::Parts, state: &S) -> Result<Self, Self::Rejection> {
        use axum::extract::FromRef;
        use axum::http::StatusCode;
        let pool = sqlx::SqlitePool::from_ref(state);

        // 1. Check for X-API-Key header
        if let Some(api_key_header) = parts.headers.get("X-API-Key").or_else(|| parts.headers.get("x-api-key")) {
            if let Ok(api_key_str) = api_key_header.to_str() {
                let query = "SELECT id FROM api_keys WHERE key_value = ? LIMIT 1";
                let row = sqlx::query(query)
                    .bind(api_key_str)
                    .fetch_optional(&pool)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({ "error": e.to_string() }))))?;

                if row.is_some() {
                    return Ok(AuthenticatedUser {
                        user_id: "api_key_client".to_string(),
                    });
                }
            }
        }

        // 2. Check for Authorization Bearer header
        if let Some(auth_header) = parts.headers.get("Authorization").or_else(|| parts.headers.get("authorization")) {
            if let Ok(auth_str) = auth_header.to_str() {
                if auth_str.starts_with("Bearer ") {
                    let token = &auth_str["Bearer ".len()..];
                    if let Ok(claims) = verify_token(token) {
                        return Ok(AuthenticatedUser {
                            user_id: claims.sub,
                        });
                    }
                    let query = "SELECT id FROM api_keys WHERE key_value = ? LIMIT 1";
                    let row = sqlx::query(query)
                        .bind(token)
                        .fetch_optional(&pool)
                        .await
                        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({ "error": e.to_string() }))))?;

                    if row.is_some() {
                        return Ok(AuthenticatedUser {
                            user_id: "api_key_client".to_string(),
                        });
                    }
                }
            }
        }

        Err((StatusCode::UNAUTHORIZED, axum::Json(serde_json::json!({ "error": "Unauthorized: Invalid or missing API key or JWT token" }))))
    }
}

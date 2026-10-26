use axum::{
    extract::{State, Json, Path},
    http::{header::HeaderMap, StatusCode},
    routing::{get, post},
    Router,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use sha2::Sha256;
use hmac::{Hmac, Mac};
use once_cell::sync::Lazy;

use crate::database::DbPool;
use sqlx::Row;

type HmacSha256 = Hmac<Sha256>;

// JWT 配置
static JWT_SECRET: Lazy<String> = Lazy::new(|| {
    std::env::var("JWT_SECRET_KEY").unwrap_or_else(|_| {
        use rand::Rng;
        let secret: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();
        secret
    })
});

static JWT_ACCESS_TOKEN_EXPIRE_HOURS: Lazy<i64> = Lazy::new(|| {
    std::env::var("JWT_ACCESS_TOKEN_EXPIRE_HOURS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(24)
});

// ============== Models ==============

#[derive(Debug, Clone, Serialize)]
pub struct IdentityInitResponse {
    pub identity_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginRequest {
    pub login_id: String,
    pub password_md5: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub user: UserInfo,
    pub identity_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserInfo {
    pub id: String,
    pub login_id: Option<String>,
    pub role: String,
    pub display_name: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RefreshTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CurrentUserResponse {
    pub user: Option<UserInfo>,
    pub identity_id: String,
    pub is_authenticated: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateProfileRequest {
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsersListResponse {
    pub users: Vec<UserInfoAdmin>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserInfoAdmin {
    pub id: String,
    pub login_id: Option<String>,
    pub role: String,
    pub display_name: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateUserRequest {
    pub login_id: String,
    pub password: String,
    pub role: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateUserRequest {
    pub role: Option<String>,
    pub display_name: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResetPasswordRequest {
    pub new_password: String,
}

// Database row types
#[derive(Debug, Clone, sqlx::FromRow)]
struct UserRow {
    id: String,
    login_id: Option<String>,
    role: String,
    display_name: Option<String>,
    password_hash: Option<String>,
    password_salt: Option<String>,
    password_hash_iterations: Option<i32>,
    is_active: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct UserIdentityRow {
    id: String,
    user_id: Option<String>,
}

// ============== State ==============

#[derive(Clone)]
pub struct AuthState {
    pub db_pool: DbPool,
}

// ============== JWT ==============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,        // user_id
    pub role: String,
    pub identity_id: String,
    pub exp: i64,           // expiration timestamp
    iat: i64,           // issued at
    #[serde(rename = "type")]
    pub token_type: String, // "access" or "refresh"
}

fn create_access_token(user_id: &str, role: &str, identity_id: &str) -> (String, i64) {
    let now = Utc::now();
    let expires_in = *JWT_ACCESS_TOKEN_EXPIRE_HOURS * 3600;
    let exp = now + Duration::seconds(expires_in);
    
    let claims = JwtClaims {
        sub: user_id.to_string(),
        role: role.to_string(),
        identity_id: identity_id.to_string(),
        exp: exp.timestamp(),
        iat: now.timestamp(),
        token_type: "access".to_string(),
    };
    
    let token = encode_jwt(&claims);
    (token, expires_in)
}

fn encode_jwt(claims: &JwtClaims) -> String {
    let header = json!({"alg": "HS256", "typ": "JWT"});
    let header_b64 = BASE64.encode(header.to_string().as_bytes());
    let claims_b64 = BASE64.encode(serde_json::to_string(claims).unwrap().as_bytes());
    
    let unsigned = format!("{}.{}", header_b64, claims_b64);
    
    let mut mac = HmacSha256::new_from_slice(JWT_SECRET.as_bytes()).unwrap();
    mac.update(unsigned.as_bytes());
    let signature = BASE64.encode(mac.finalize().into_bytes());
    
    format!("{}.{}", unsigned, signature)
}

pub fn decode_jwt(token: &str) -> Option<JwtClaims> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    
    // Verify signature
    let unsigned = format!("{}.{}", parts[0], parts[1]);
    let mut mac = HmacSha256::new_from_slice(JWT_SECRET.as_bytes()).unwrap();
    mac.update(unsigned.as_bytes());
    
    let expected_sig = BASE64.encode(mac.finalize().into_bytes());
    if parts[2] != expected_sig {
        return None;
    }
    
    // Decode claims
    let claims_json = BASE64.decode(parts[1]).ok()?;
    let claims: JwtClaims = serde_json::from_slice(&claims_json).ok()?;
    
    // Check expiration
    let now = Utc::now().timestamp();
    if claims.exp < now {
        return None;
    }
    
    Some(claims)
}

// ============== Password Verification ==============

fn verify_password(password_md5: &str, password_hash: &str, password_salt: &str, iterations: i32) -> bool {
    let salt = match BASE64.decode(password_salt) {
        Ok(s) => s,
        Err(_) => return false,
    };
    
    // PBKDF2-HMAC-SHA256
    let mut derived = [0u8; 32];
    pbkdf2::pbkdf2::<Hmac<Sha256>>(
        password_md5.as_bytes(),
        &salt,
        iterations as u32,
        &mut derived,
    ).unwrap();
    
    let computed_hash = BASE64.encode(derived);
    computed_hash == password_hash
}

pub(crate) fn hash_password(password_md5: &str) -> (String, String, i32) {
    use rand::RngCore;
    
    let iterations = 200_000i32;
    let mut salt = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut salt);
    
    let mut derived = [0u8; 32];
    pbkdf2::pbkdf2::<Hmac<Sha256>>(
        password_md5.as_bytes(),
        &salt,
        iterations as u32,
        &mut derived,
    ).unwrap();
    
    let password_hash = BASE64.encode(derived);
    let password_salt = BASE64.encode(salt);
    
    (password_hash, password_salt, iterations)
}

// ============== Helpers ==============

async fn get_setting_bool(pool: &sqlx::SqlitePool, key: &str, default: bool) -> bool {
    let result = sqlx::query("SELECT value_json FROM system_settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await;
    match result {
        Ok(Some(row)) => {
            let value_json: String = row.get("value_json");
            serde_json::from_str(&value_json).unwrap_or(default)
        }
        _ => default,
    }
}

fn extract_identity_id(headers: &HeaderMap) -> Option<String> {
    headers
        .get("X-Identity-Id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

pub fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

// ============== Handlers ==============

async fn init_identity(
    State(state): State<AuthState>,
) -> Result<Json<IdentityInitResponse>, (StatusCode, Json<Value>)> {
    // Check allow_anonymous setting
    let allow_anonymous = get_setting_bool(&state.db_pool, "allow_anonymous", true).await;
    if !allow_anonymous {
        return Err((StatusCode::FORBIDDEN, Json(json!({"detail": "匿名使用已被禁用，请先登录"}))));
    }

    let identity_id = crate::database::gen_prefix_random_id("ui", 16);
    sqlx::query(
        r#"
        INSERT INTO user_identities (id, created_at, updated_at)
        VALUES (?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        "#,
    )
    .bind(&identity_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create identity: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Failed to create identity"})))
    })?;
    
    // Get the created identity to return its id
    let identity: UserIdentityRow = sqlx::query_as(
        "SELECT id, user_id FROM user_identities WHERE id = ?",
    )
    .bind(&identity_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to get created identity: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Failed to get created identity"})))
    })?;
    
    tracing::info!("Created new identity: {}", identity.id);
    
    Ok(Json(IdentityInitResponse { identity_id: identity.id }))
}

async fn login(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<Value>)> {
    // Find user
    let user: UserRow = sqlx::query_as(
        r#"
        SELECT id, login_id, role, display_name, password_hash, password_salt, password_hash_iterations, is_active
        FROM users
        WHERE login_id = ?
        "#,
    )
    .bind(&request.login_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Database error"})))
    })?
    .ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"detail": "用户名或密码错误"})))
    })?;
    
    // Verify password
    let password_hash = user.password_hash.as_ref().ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"detail": "用户名或密码错误"})))
    })?;
    let password_salt = user.password_salt.as_ref().ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"detail": "用户名或密码错误"})))
    })?;
    let iterations = user.password_hash_iterations.unwrap_or(200_000);
    
    if !verify_password(&request.password_md5, password_hash, password_salt, iterations) {
        return Err((StatusCode::UNAUTHORIZED, Json(json!({"detail": "用户名或密码错误"}))));
    }

    // Check if account is frozen
    if !user.is_active {
        return Err((StatusCode::FORBIDDEN, Json(json!({"detail": "账号已被冻结，请联系管理员"}))));
    }
    
    // Handle identity with anti-hijacking
    let identity_id = if let Some(id) = extract_identity_id(&headers) {
        // Check if identity exists
        let existing: Option<UserIdentityRow> = sqlx::query_as(
            "SELECT id, user_id FROM user_identities WHERE id = ?",
        )
        .bind(&id)
        .fetch_optional(&state.db_pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Database error"})))
        })?;

        if let Some(row) = existing {
            match row.user_id.as_deref() {
                None => {
                    // Identity not claimed → link to current user
                    sqlx::query("UPDATE user_identities SET user_id = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                        .bind(&user.id)
                        .bind(&id)
                        .execute(&state.db_pool)
                        .await
                        .map_err(|e| {
                            tracing::error!("Failed to link identity: {}", e);
                            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Failed to link identity"})))
                        })?;
                    tracing::info!("Linked identity {} to user {}", id, user.id);
                    id
                }
                Some(uid) if uid == user.id => {
                    // Already linked to current user → no-op
                    tracing::info!("Identity {} already linked to user {}", id, user.id);
                    id
                }
                _ => {
                    // Identity claimed by another user → don't hijack, create new
                    tracing::info!("Identity {} already claimed, creating new identity for user {}", id, user.id);
                    let identity_id = crate::database::gen_prefix_random_id("ui", 16);
                    sqlx::query(
                        "INSERT INTO user_identities (id, user_id, created_at, updated_at) VALUES (?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
                    )
                    .bind(&identity_id)
                    .bind(&user.id)
                    .execute(&state.db_pool)
                    .await
                    .map_err(|e| {
                        tracing::error!("Failed to create identity: {}", e);
                        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Failed to create identity"})))
                    })?;

                    // Get the created identity
                    let identity: UserIdentityRow = sqlx::query_as(
                        "SELECT id, user_id FROM user_identities WHERE id = ?",
                    )
                    .bind(&identity_id)
                    .fetch_one(&state.db_pool)
                    .await
                    .map_err(|e| {
                        tracing::error!("Failed to get created identity: {}", e);
                        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Failed to get created identity"})))
                    })?;
                    identity.id
                }
            }
        } else {
            // Identity doesn't exist → create new
            let identity_id = crate::database::gen_prefix_random_id("ui", 16);
            sqlx::query(
                "INSERT INTO user_identities (id, user_id, created_at, updated_at) VALUES (?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
            )
            .bind(&identity_id)
            .bind(&user.id)
            .execute(&state.db_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to create identity: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Failed to create identity"})))
            })?;

            let identity: UserIdentityRow = sqlx::query_as(
                "SELECT id, user_id FROM user_identities WHERE id = ?",
            )
            .bind(&identity_id)
            .fetch_one(&state.db_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get created identity: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Failed to get created identity"})))
            })?;

            tracing::info!("Created new identity {} for user {}", identity.id, user.id);
            identity.id
        }
    } else {
        // No identity provided, create new
        let identity_id = crate::database::gen_prefix_random_id("ui", 16);
        sqlx::query(
            "INSERT INTO user_identities (id, user_id, created_at, updated_at) VALUES (?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .bind(&identity_id)
        .bind(&user.id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create identity: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Failed to create identity"})))
        })?;
        
        // Get the created identity
        let identity: UserIdentityRow = sqlx::query_as(
            "SELECT id, user_id FROM user_identities WHERE id = ?",
        )
        .bind(&identity_id)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get created identity: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Failed to get created identity"})))
        })?;
        
        tracing::info!("Created new identity {} for user {}", identity.id, user.id);
        identity.id
    };
    
    // Generate token
    let (access_token, expires_in) = create_access_token(&user.id, &user.role, &identity_id);
    
    tracing::info!("User {} logged in successfully", request.login_id);
    
    Ok(Json(LoginResponse {
        access_token,
        token_type: "bearer".to_string(),
        expires_in,
        user: UserInfo {
            id: user.id,
            login_id: user.login_id,
            role: user.role,
            display_name: user.display_name,
            is_active: user.is_active,
        },
        identity_id,
    }))
}

async fn logout(
    headers: HeaderMap,
) -> Json<Value> {
    if let Some(token) = extract_bearer_token(&headers) {
        if let Some(claims) = decode_jwt(&token) {
            tracing::info!("User {} logged out", claims.sub);
        }
    }
    Json(json!({"message": "Logged out successfully"}))
}

async fn refresh_token(
    State(state): State<AuthState>,
    Json(request): Json<RefreshTokenRequest>,
) -> Result<Json<RefreshTokenResponse>, (StatusCode, Json<Value>)> {
    let claims = decode_jwt(&request.refresh_token).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"detail": "Invalid or expired refresh token"})))
    })?;
    
    if claims.token_type != "refresh" {
        return Err((StatusCode::UNAUTHORIZED, Json(json!({"detail": "Invalid token type"}))));
    }
    
    // Get user
    let user: UserRow = sqlx::query_as(
        "SELECT id, login_id, role, display_name, password_hash, password_salt, password_hash_iterations, is_active FROM users WHERE id = ?",
    )
    .bind(&claims.sub)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Database error"})))
    })?
    .ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"detail": "User not found"})))
    })?;
    
    // Find identity
    let identity: Option<UserIdentityRow> = sqlx::query_as(
        "SELECT id, user_id FROM user_identities WHERE user_id = ? LIMIT 1",
    )
    .bind(&user.id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Database error"})))
    })?;
    
    let identity_id = identity.map(|i| i.id).unwrap_or_default();
    
    let (access_token, expires_in) = create_access_token(&user.id, &user.role, &identity_id);
    
    Ok(Json(RefreshTokenResponse {
        access_token,
        token_type: "bearer".to_string(),
        expires_in,
    }))
}

async fn get_me(
    State(state): State<AuthState>,
    headers: HeaderMap,
) -> Json<CurrentUserResponse> {
    let identity_id = extract_identity_id(&headers).unwrap_or_default();
    
    let (user, is_authenticated) = if let Some(token) = extract_bearer_token(&headers) {
        if let Some(claims) = decode_jwt(&token) {
            let user: Option<UserRow> = sqlx::query_as(
                "SELECT id, login_id, role, display_name, password_hash, password_salt, password_hash_iterations, is_active FROM users WHERE id = ?",
            )
            .bind(&claims.sub)
            .fetch_optional(&state.db_pool)
            .await
            .ok()
            .flatten();
            
            if let Some(u) = user {
                (Some(UserInfo {
                    id: u.id,
                    login_id: u.login_id,
                    role: u.role,
                    display_name: u.display_name,
                    is_active: u.is_active,
                }), true)
            } else {
                (None, false)
            }
        } else {
            (None, false)
        }
    } else {
        (None, false)
    };
    
    Json(CurrentUserResponse {
        user,
        identity_id,
        is_authenticated,
    })
}

async fn update_profile(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(request): Json<UpdateProfileRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let token = extract_bearer_token(&headers).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"detail": "Not authenticated"})))
    })?;
    
    let claims = decode_jwt(&token).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"detail": "Invalid token"})))
    })?;
    
    if let Some(display_name) = request.display_name {
        sqlx::query("UPDATE users SET display_name = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(&display_name)
            .bind(&claims.sub)
            .execute(&state.db_pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update profile: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Failed to update profile"})))
            })?;
    }
    
    // Get updated user
    let user: UserRow = sqlx::query_as(
        "SELECT id, login_id, role, display_name, password_hash, password_salt, password_hash_iterations, is_active FROM users WHERE id = ?",
    )
    .bind(&claims.sub)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Database error"})))
    })?;
    
    tracing::info!("User {} updated profile", claims.sub);
    
    Ok(Json(json!({
        "message": "Profile updated successfully",
        "user": {
            "id": user.id,
            "login_id": user.login_id,
            "role": user.role,
            "display_name": user.display_name,
            "is_active": user.is_active,
        }
    })))
}

async fn change_password(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(request): Json<ChangePasswordRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let token = extract_bearer_token(&headers).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"detail": "Not authenticated"})))
    })?;
    
    let claims = decode_jwt(&token).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"detail": "Invalid token"})))
    })?;
    
    // Get user
    let user: UserRow = sqlx::query_as(
        "SELECT id, login_id, role, display_name, password_hash, password_salt, password_hash_iterations, is_active FROM users WHERE id = ?",
    )
    .bind(&claims.sub)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Database error"})))
    })?;
    
    // Verify current password
    let password_hash = user.password_hash.as_ref().ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(json!({"detail": "User has no password"})))
    })?;
    let password_salt = user.password_salt.as_ref().ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(json!({"detail": "User has no password"})))
    })?;
    let iterations = user.password_hash_iterations.unwrap_or(200_000);
    
    if !verify_password(&request.current_password, password_hash, password_salt, iterations) {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"detail": "当前密码不正确"}))));
    }
    
    // Hash new password
    let (new_hash, new_salt, new_iterations) = hash_password(&request.new_password);
    
    // Update password
    sqlx::query("UPDATE users SET password_hash = ?, password_salt = ?, password_hash_iterations = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(&new_hash)
        .bind(&new_salt)
        .bind(new_iterations)
        .bind(&claims.sub)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update password: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Failed to update password"})))
        })?;
    
    tracing::info!("User {} changed password", claims.sub);
    
    Ok(Json(json!({"message": "Password changed successfully"})))
}

async fn list_users(
    State(state): State<AuthState>,
    headers: HeaderMap,
) -> Result<Json<UsersListResponse>, (StatusCode, Json<Value>)> {
    let token = extract_bearer_token(&headers).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"detail": "Not authenticated"})))
    })?;
    
    let claims = decode_jwt(&token).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"detail": "Invalid token"})))
    })?;
    
    // Check admin role
    if claims.role != "admin" {
        return Err((StatusCode::FORBIDDEN, Json(json!({"detail": "Admin access required"}))));
    }
    
    let users: Vec<UserRow> = sqlx::query_as(
        "SELECT id, login_id, role, display_name, password_hash, password_salt, password_hash_iterations, is_active FROM users",
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Database error"})))
    })?;
    
    Ok(Json(UsersListResponse {
        users: users.into_iter().map(|u| UserInfoAdmin {
            id: u.id,
            login_id: u.login_id,
            role: u.role,
            display_name: u.display_name,
            is_active: u.is_active,
        }).collect(),
    }))
}

async fn create_user(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Json(request): Json<CreateUserRequest>,
) -> Result<Json<UserInfoAdmin>, (StatusCode, Json<Value>)> {
    let token = extract_bearer_token(&headers).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"detail": "Not authenticated"})))
    })?;
    
    let claims = decode_jwt(&token).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"detail": "Invalid token"})))
    })?;
    
    if claims.role != "admin" {
        return Err((StatusCode::FORBIDDEN, Json(json!({"detail": "Admin access required"}))));
    }
    
    // Check if login_id exists
    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE login_id = ?")
        .bind(&request.login_id)
        .fetch_optional(&state.db_pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Database error"})))
        })?;
    
    if existing.is_some() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"detail": "用户名已存在"}))));
    }
    
    let (password_hash, password_salt, iterations) = hash_password(&request.password);
    let role = request.role.unwrap_or_else(|| "user".to_string());
    let display_name = request.display_name.unwrap_or_else(|| request.login_id.clone());
    let user_id = crate::database::gen_prefix_random_id("us", 8);

    sqlx::query(
        "INSERT INTO users (id, login_id, role, display_name, password_hash, password_salt, password_hash_iterations, is_active) VALUES (?, ?, ?, ?, ?, ?, ?, 1)"
    )
    .bind(&user_id)
    .bind(&request.login_id)
    .bind(&role)
    .bind(&display_name)
    .bind(&password_hash)
    .bind(&password_salt)
    .bind(iterations)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create user: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Failed to create user"})))
    })?;
    
    // Get the created user to return
    let created_user: UserRow = sqlx::query_as(
        "SELECT id, login_id, role, display_name, password_hash, password_salt, password_hash_iterations, is_active FROM users WHERE login_id = ?",
    )
    .bind(&request.login_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to get created user: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Failed to get created user"})))
    })?;
    
    tracing::info!("Admin {} created user {}", claims.sub, request.login_id);
    
    Ok(Json(UserInfoAdmin {
        id: created_user.id,
        login_id: created_user.login_id,
        role: created_user.role,
        display_name: created_user.display_name,
        is_active: created_user.is_active,
    }))
}

async fn update_user(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Json(request): Json<UpdateUserRequest>,
) -> Result<Json<UserInfoAdmin>, (StatusCode, Json<Value>)> {
    let token = extract_bearer_token(&headers).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"detail": "Not authenticated"})))
    })?;
    
    let claims = decode_jwt(&token).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"detail": "Invalid token"})))
    })?;
    
    if claims.role != "admin" {
        return Err((StatusCode::FORBIDDEN, Json(json!({"detail": "Admin access required"}))));
    }
    
    // Get target user
    let user: UserRow = sqlx::query_as(
        "SELECT id, login_id, role, display_name, password_hash, password_salt, password_hash_iterations, is_active FROM users WHERE id = ?",
    )
    .bind(&user_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Database error"})))
    })?
    .ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({"detail": "用户不存在"})))
    })?;
    
    // Cannot change own role
    if user_id == claims.sub && request.role.is_some() && request.role.as_ref() != Some(&user.role) {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"detail": "不能修改自己的角色"}))));
    }
    
    let new_role = request.role.unwrap_or(user.role);
    let new_display_name = request.display_name.or(user.display_name);
    let new_is_active = request.is_active.unwrap_or(user.is_active);
    
    sqlx::query("UPDATE users SET role = ?, display_name = ?, is_active = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(&new_role)
        .bind(&new_display_name)
        .bind(new_is_active)
        .bind(&user_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update user: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Failed to update user"})))
        })?;
    
    tracing::info!("Admin {} updated user {}", claims.sub, user_id);
    
    Ok(Json(UserInfoAdmin {
        id: user_id,
        login_id: user.login_id,
        role: new_role,
        display_name: new_display_name,
        is_active: new_is_active,
    }))
}

async fn reset_user_password(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Json(request): Json<ResetPasswordRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let token = extract_bearer_token(&headers).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"detail": "Not authenticated"})))
    })?;
    
    let claims = decode_jwt(&token).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"detail": "Invalid token"})))
    })?;
    
    if claims.role != "admin" {
        return Err((StatusCode::FORBIDDEN, Json(json!({"detail": "Admin access required"}))));
    }
    
    // Check user exists
    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE id = ?")
        .bind(&user_id)
        .fetch_optional(&state.db_pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Database error"})))
        })?;
    
    if existing.is_none() {
        return Err((StatusCode::NOT_FOUND, Json(json!({"detail": "用户不存在"}))));
    }
    
    let (password_hash, password_salt, iterations) = hash_password(&request.new_password);
    
    sqlx::query("UPDATE users SET password_hash = ?, password_salt = ?, password_hash_iterations = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(&password_hash)
        .bind(&password_salt)
        .bind(iterations)
        .bind(&user_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to reset password: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Failed to reset password"})))
        })?;
    
    tracing::info!("Admin {} reset password for user {}", claims.sub, user_id);
    
    Ok(Json(json!({"message": "密码重置成功"})))
}

async fn delete_user(
    State(state): State<AuthState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let token = extract_bearer_token(&headers).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"detail": "Not authenticated"})))
    })?;
    
    let claims = decode_jwt(&token).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"detail": "Invalid token"})))
    })?;
    
    if claims.role != "admin" {
        return Err((StatusCode::FORBIDDEN, Json(json!({"detail": "Admin access required"}))));
    }
    
    if user_id == claims.sub {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"detail": "不能删除自己"}))));
    }
    
    // Check user exists
    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE id = ?")
        .bind(&user_id)
        .fetch_optional(&state.db_pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Database error"})))
        })?;
    
    if existing.is_none() {
        return Err((StatusCode::NOT_FOUND, Json(json!({"detail": "用户不存在"}))));
    }
    
    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(&user_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete user: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"detail": "Failed to delete user"})))
        })?;
    
    tracing::info!("Admin {} deleted user {}", claims.sub, user_id);
    
    Ok(Json(json!({"message": "用户已删除"})))
}

// ============== Router ==============

/// User info for external use
#[derive(Debug, Clone)]
pub struct ExtractedUser {
    pub id: String,
    pub role: String,
    pub login_id: Option<String>,
    pub display_name: Option<String>,
}

/// Extract user from Authorization header token
pub async fn extract_user_from_token(
    headers: &HeaderMap,
    db_pool: &DbPool,
) -> Result<ExtractedUser, String> {
    let token = extract_bearer_token(headers)
        .ok_or_else(|| "No authorization token".to_string())?;
    
    let claims = decode_jwt(&token)
        .ok_or_else(|| "Invalid or expired token".to_string())?;
    
    let user: UserRow = sqlx::query_as(
        "SELECT id, login_id, role, display_name, password_hash, password_salt, password_hash_iterations, is_active FROM users WHERE id = ?",
    )
    .bind(&claims.sub)
    .fetch_optional(db_pool)
    .await
    .map_err(|e| format!("Database error: {}", e))?
    .ok_or_else(|| "User not found".to_string())?;
    
    Ok(ExtractedUser {
        id: user.id,
        role: user.role,
        login_id: user.login_id,
        display_name: user.display_name,
    })
}

pub fn auth_router() -> Router<AuthState> {
    Router::new()
        .route("/identity/init", get(init_identity))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/refresh", post(refresh_token))
        .route("/me", get(get_me))
        .route("/profile", axum::routing::put(update_profile))
        .route("/change-password", post(change_password))
        .route("/users", get(list_users).post(create_user))
        .route("/users/{user_id}", axum::routing::put(update_user).delete(delete_user))
        .route("/users/{user_id}/reset-password", post(reset_user_password))
}

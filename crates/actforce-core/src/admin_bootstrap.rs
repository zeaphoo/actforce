//! Admin user bootstrap — creates the initial `admin` account on first run.
//! Mirrors agentlite/src/agentlite/admin_bootstrap.py::ensure_admin_user.

use rand::distributions::Alphanumeric;
use rand::Rng;
use sqlx::SqlitePool;
use tracing::info;

use crate::auth::hash_password;
use crate::config::CONFIG;
use crate::database::gen_prefix_random_id;

/// Ensure an admin user exists. Returns true if a new admin user was created.
pub async fn ensure_admin_user(pool: &SqlitePool) -> anyhow::Result<bool> {
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT login_id FROM users WHERE login_id = ? LIMIT 1",
    )
    .bind("admin")
    .fetch_optional(pool)
    .await?;

    if exists.is_some() {
        return Ok(false);
    }

    // Random password (~22 chars, comparable to agentlite's secrets.token_urlsafe(16))
    let password: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(22)
        .map(char::from)
        .collect();

    // Storage scheme is pbkdf2_sha256(md5(password)) — same as the login flow
    let md5_hex = format!("{:x}", md5::compute(password.as_bytes()));
    let (password_hash, password_salt, iterations) = hash_password(&md5_hex);

    let id = gen_prefix_random_id("us", 8);

    sqlx::query(
        "INSERT INTO users (id, login_id, role, display_name, password_hash, password_salt, password_hash_algo, password_hash_iterations, is_active, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, 'pbkdf2_sha256', ?, 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
    )
    .bind(&id)
    .bind("admin")
    .bind("admin")
    .bind("Admin")
    .bind(&password_hash)
    .bind(&password_salt)
    .bind(iterations)
    .execute(pool)
    .await?;

    // Write the password to data_root/admin_password.txt (chmod 600)
    let password_file = CONFIG.data_root.join("admin_password.txt");
    if let Some(parent) = password_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&password_file, format!("{password}\n"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&password_file, std::fs::Permissions::from_mode(0o600));
    }

    info!(
        "Created initial admin user; password written to {}",
        password_file.display()
    );
    Ok(true)
}

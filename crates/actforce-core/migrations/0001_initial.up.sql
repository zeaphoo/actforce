-- Initial schema migration
-- Matches Python alembic/versions/0001_initial.py

-- Users table
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    login_id TEXT UNIQUE,
    role TEXT NOT NULL DEFAULT 'guest',
    display_name TEXT,
    password_hash TEXT,
    password_salt TEXT,
    password_hash_algo TEXT,
    password_hash_iterations INTEGER,
    is_active INTEGER NOT NULL DEFAULT 1,
    extra_json TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS ix_users_id ON users(id);
CREATE UNIQUE INDEX IF NOT EXISTS ix_users_login_id ON users(login_id);

-- User identities table
CREATE TABLE IF NOT EXISTS user_identities (
    id TEXT PRIMARY KEY,
    user_id TEXT,
    identifier TEXT,
    source TEXT,
    extra_json TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS ix_user_identities_id ON user_identities(id);
CREATE INDEX IF NOT EXISTS ix_user_identities_user_id ON user_identities(user_id);
CREATE INDEX IF NOT EXISTS ix_user_identities_identifier ON user_identities(identifier);

-- Conversations table
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    title TEXT,
    owner_identity_id TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS ix_conversations_id ON conversations(id);
CREATE INDEX IF NOT EXISTS ix_conversations_owner_identity_id ON conversations(owner_identity_id);

-- Messages table
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    content_json TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS ix_messages_id ON messages(id);
CREATE INDEX IF NOT EXISTS ix_messages_conversation_id ON messages(conversation_id);

-- Skills table
CREATE TABLE IF NOT EXISTS skills (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    content TEXT,
    program TEXT,
    extra_json TEXT,
    created_by TEXT,
    updated_by TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS ix_skills_id ON skills(id);
CREATE INDEX IF NOT EXISTS ix_skills_name ON skills(name);
CREATE INDEX IF NOT EXISTS ix_skills_created_by ON skills(created_by);
CREATE INDEX IF NOT EXISTS ix_skills_updated_by ON skills(updated_by);

-- System settings table
CREATE TABLE IF NOT EXISTS system_settings (
    key TEXT PRIMARY KEY,
    value_json TEXT,
    created_by TEXT,
    updated_by TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS ix_system_settings_created_by ON system_settings(created_by);
CREATE INDEX IF NOT EXISTS ix_system_settings_updated_by ON system_settings(updated_by);

-- Quick prompts table
CREATE TABLE IF NOT EXISTS quick_prompts (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT,
    prompt TEXT NOT NULL,
    icon TEXT,
    category TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    created_by TEXT,
    updated_by TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS ix_quick_prompts_id ON quick_prompts(id);
CREATE INDEX IF NOT EXISTS ix_quick_prompts_created_by ON quick_prompts(created_by);
CREATE INDEX IF NOT EXISTS ix_quick_prompts_updated_by ON quick_prompts(updated_by);

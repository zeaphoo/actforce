-- Rollback initial schema migration
-- Drops all tables created in up.sql in reverse order

-- Drop indexes first (optional, as DROP TABLE will handle them)
DROP INDEX IF EXISTS ix_quick_prompts_updated_by;
DROP INDEX IF EXISTS ix_quick_prompts_created_by;
DROP INDEX IF EXISTS ix_quick_prompts_id;

DROP INDEX IF EXISTS ix_system_settings_updated_by;
DROP INDEX IF EXISTS ix_system_settings_created_by;

DROP INDEX IF EXISTS ix_skills_updated_by;
DROP INDEX IF EXISTS ix_skills_created_by;
DROP INDEX IF EXISTS ix_skills_name;
DROP INDEX IF EXISTS ix_skills_id;

DROP INDEX IF EXISTS ix_messages_conversation_id;
DROP INDEX IF EXISTS ix_messages_id;

DROP INDEX IF EXISTS ix_conversations_owner_identity_id;
DROP INDEX IF EXISTS ix_conversations_id;

DROP INDEX IF EXISTS ix_user_identities_identifier;
DROP INDEX IF EXISTS ix_user_identities_user_id;
DROP INDEX IF EXISTS ix_user_identities_id;

DROP INDEX IF EXISTS ix_users_login_id;
DROP INDEX IF EXISTS ix_users_id;

-- Drop tables in reverse dependency order
DROP TABLE IF EXISTS quick_prompts;
DROP TABLE IF EXISTS system_settings;
DROP TABLE IF EXISTS skills;
DROP TABLE IF EXISTS messages;
DROP TABLE IF EXISTS conversations;
DROP TABLE IF EXISTS user_identities;
DROP TABLE IF EXISTS users;

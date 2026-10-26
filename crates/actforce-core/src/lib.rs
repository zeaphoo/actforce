#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_mut)]

mod admin_bootstrap;
mod app_config;
mod attachment_handler;
mod auth;
mod auto_start;
mod aupage;
mod builtin_knowledge;
mod builtin_skills;
mod chat;
mod cli;
mod config;
mod database;
mod disks;
mod file_ops;
mod file_processing;
mod inference;
mod markdown_export;
mod models;
mod process;
mod quick_prompts;
mod skills;
mod system_metrics;

pub mod app;

pub use app::{run, run_with_factory, AppFactory, AppState, CommunityAppFactory};
pub use auth::{extract_user_from_token, ExtractedUser};
pub use builtin_skills::{
    get_builtin_skills, load_skill_from_json_content, BuiltinSkill,
};
pub use database::DbPool;

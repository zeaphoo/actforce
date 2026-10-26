/// CLI 模块 - 集中管理所有命令行子命令
/// 
/// 结构:
///     actforce
///     ├── db          数据库管理
///     │   ├── migrate
///     │   ├── status
///     │   ├── backup
///     │   ├── schema
///     │   └── tables
///     └── convert-skills  技能转换工具

pub mod db;
pub mod convert_skills;

use clap::{Parser, Subcommand};

/// ActForce CLI - 轻量级 AI 智能体平台
#[derive(Parser)]
#[command(
    name = "actforce",
    author,
    version,
    about = "ActForce - 轻量级 AI 智能体平台",
    long_about = None
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 数据库管理命令
    Db(db::DbArgs),
    
    /// 转换 Markdown skill 文件为 JSON 格式
    ConvertSkills(convert_skills::ConvertSkillsArgs),
}

impl Commands {
    /// 执行命令
    pub async fn execute(self) -> anyhow::Result<()> {
        match self {
            Commands::Db(args) => db::execute(args).await,
            Commands::ConvertSkills(args) => convert_skills::execute(args).await,
        }
    }
}

impl Cli {
    /// 检查是否有子命令
    pub fn has_command(&self) -> bool {
        self.command.is_some()
    }
}

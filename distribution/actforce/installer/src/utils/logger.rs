use colored::*;
use env_logger::Builder;
use log::{Level, LevelFilter};
use std::io::Write;

pub fn init(verbose: bool) {
    let mut builder = Builder::new();
    
    // Check if RUST_LOG environment variable is set
    if std::env::var("RUST_LOG").is_ok() {
        // If set, env_logger will use the value from RUST_LOG
        builder.parse_env("RUST_LOG");
    } else {
        // If not set, use Debug level if verbose is true, otherwise use Info level
        let level = if verbose {
            LevelFilter::Debug
        } else {
            LevelFilter::Info
        };
        builder.filter(None, level);
    }
    
    builder
        .format(|buf, record| {
            let level_str = match record.level() {
                Level::Error => "ERROR".red().bold(),
                Level::Warn => "WARN".yellow().bold(),
                Level::Info => "INFO".green().bold(),
                Level::Debug => "DEBUG".blue().bold(),
                Level::Trace => "TRACE".normal(),
            };

            writeln!(buf, "[{}] {}", level_str, record.args())
        })
        .init();
}

pub fn log_success(message: &str) {
    println!("{} {}", "[SUCCESS]".green().bold(), message);
}

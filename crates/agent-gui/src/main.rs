//! agent-gui CLI entry point.
//!
//! All subcommands mirror the Python agent_gui CLI exactly.

use agent_gui::{clipboard, keyboard, mouse, screen, shell, window};
use clap::{Parser, Subcommand};
use serde_json::json;
use std::path::PathBuf;
use std::process;

fn print_json(value: &serde_json::Value) {
    println!("{}", value);
}

#[derive(Parser)]
#[command(name = "agent-gui", about = "GUI automation CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Single mouse click at (x, y)
    MouseClick {
        x: i32,
        y: i32,
        #[arg(long, default_value = "left")]
        button: String,
    },
    /// Move mouse cursor to (x, y)
    MouseMove { x: i32, y: i32 },
    /// Get current mouse cursor position
    MousePos,
    /// Press and release a key
    KeyPress { key: String },
    /// Type a string of text
    Type { text: String },
    /// Press a hotkey combination (comma-separated keys, e.g. ctrl,c)
    Hotkey { keys: String },
    /// Take a screenshot (prints base64 to stdout, or saves to --out)
    Screenshot {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Get screen size (width x height)
    ScreenSize,
    /// Get the active window info
    ActiveWindow,
    /// Activate a window by handle
    WindowActivate { handle: String },
    /// Get clipboard text
    ClipboardGet,
    /// Set clipboard text
    ClipboardSet { text: String },
    /// Run a shell command
    Shell {
        command: String,
        #[arg(long, default_value_t = 30)]
        timeout: u64,
    },
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Commands::MouseClick { x, y, button } => {
            mouse::click(x, y, &button)?;
        }
        Commands::MouseMove { x, y } => {
            mouse::move_to(x, y)?;
        }
        Commands::MousePos => {
            let (x, y) = mouse::position()?;
            print_json(&json!({"x": x, "y": y}));
        }
        Commands::KeyPress { key } => {
            keyboard::press_key(&key)?;
        }
        Commands::Type { text } => {
            keyboard::type_text(&text)?;
        }
        Commands::Hotkey { keys } => {
            let parts: Vec<&str> = keys.split(',').filter(|s| !s.is_empty()).collect();
            keyboard::hotkey(&parts)?;
        }
        Commands::Screenshot { out } => {
            match out {
                Some(path) => {
                    screen::screenshot_to_file(&path)?;
                }
                None => {
                    let b64 = screen::screenshot_b64()?;
                    println!("{}", b64);
                }
            }
        }
        Commands::ScreenSize => {
            let (w, h) = screen::screen_size()?;
            print_json(&json!({"width": w, "height": h}));
        }
        Commands::ActiveWindow => {
            let title = window::get_active_window_title()?;
            let handle = window::get_active_window_handle()?;
            print_json(&json!({"title": title, "handle": handle}));
        }
        Commands::WindowActivate { handle } => {
            let ok = window::activate_window(&handle)?;
            process::exit(if ok { 0 } else { 1 });
        }
        Commands::ClipboardGet => {
            let text = clipboard::get()?;
            print!("{}", text);
        }
        Commands::ClipboardSet { text } => {
            clipboard::set(&text)?;
        }
        Commands::Shell { command, timeout } => {
            let res = shell::run(&command, timeout)?;
            let code = res.returncode;
            print_json(&json!({
                "stdout": res.stdout,
                "stderr": res.stderr,
                "returncode": res.returncode
            }));
            if code != 0 {
                process::exit(code);
            }
        }
    }

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e:#}");
        process::exit(1);
    }
}

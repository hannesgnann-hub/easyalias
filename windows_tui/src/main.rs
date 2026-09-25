//! EasyAlias TUI for Windows: the EasyAlias alias and automation manager as a terminal app.
//!
//! The data layer (everything except `tui/`, `preview` and `suggestions`) is the
//! same logic the desktop app uses, minus Tauri. Both read and write
//! ~/.easyalias, so the TUI and the desktop app can be used side by side.

use chrono::{DateTime, Datelike, Local, Offset, TimeZone, Timelike};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

mod models;
mod paths;
mod clock;
mod cmd_scripts;
mod user_path;
mod legacy_import;
mod aliases;
mod automations;
mod session;
mod timed;
mod sun;
mod schtasks;
mod settings;
mod help;
mod platform;
mod preview;
mod suggestions;
mod tui;

use models::*;
use paths::*;
use clock::*;
use cmd_scripts::*;
use user_path::*;
use legacy_import::*;
use aliases::*;
use automations::*;
use session::*;
use timed::*;
use sun::*;
use schtasks::*;
use settings::*;
use preview::*;

const HELP: &str = "\
EasyAlias TUI - manage shell aliases and automations in the terminal

USAGE:
    easyalias-tui              Open the interactive interface
    easyalias-tui --help       Show this help
    easyalias-tui --version    Show the version

Data lives in ~/.easyalias and is shared with the EasyAlias desktop app.
";

fn main() {
    let args: Vec<String> = env::args().collect();

    // launchd invokes this same executable to fire a timed automation - handle
    // that before touching the terminal, then exit.
    if args.len() >= 3 && args[1] == "--run-timed-automation" {
        let exit_code = match run_timed_automation_headless(&args[2]) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("Timed automation {} failed: {}", args[2], error);
                1
            }
        };
        std::process::exit(exit_code);
    }
    if args.len() >= 2 && args[1] == "--check-sun-timed-automations" {
        let exit_code = match check_sun_timed_automations() {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("Sunrise/sunset timed automations check failed: {}", error);
                1
            }
        };
        std::process::exit(exit_code);
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print!("{}", HELP);
        return;
    }
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("easyalias-tui {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if let Some(unknown) = args.get(1) {
        eprintln!("Unknown argument: {}\n\n{}", unknown, HELP);
        std::process::exit(2);
    }

    if let Err(error) = tui::run() {
        eprintln!("easyalias-tui: {}", error);
        std::process::exit(1);
    }
}

#[cfg(test)]
pub(crate) mod tests;

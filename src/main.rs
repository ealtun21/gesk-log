mod mlog;
mod slog;
mod tlog;
mod utils;

use mlog::mlog_main;
use slog::slog_main;
use tlog::tlog_main;

use crossterm::style::Stylize;
use enum_display_derive::{self, Display};
use inquire::{InquireError, Select};
use std::fmt::Display;

#[derive(Debug, Display)]
enum GeskMode {
    SLog,
    TLog,
    MLog,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gesk_mode = loop {
        match Select::new(
            "Please select logging mode:",
            vec![GeskMode::SLog, GeskMode::TLog, GeskMode::MLog],
        )
        .prompt()
        {
            Ok(mode) => break mode,
            Err(InquireError::OperationInterrupted) => return Ok(()),
            Err(_) => eprintln!("{}", "Please select an option.".red().slow_blink()),
        }
    };

    match gesk_mode {
        GeskMode::SLog => slog_main(true),
        GeskMode::TLog => tlog_main(),
        GeskMode::MLog => mlog_main(),
    }
}

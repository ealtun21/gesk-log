use chrono::Timelike;
use chrono::{Datelike, Local};

pub fn generate_timestamp() -> String {
    let now = Local::now();

    format!(
        "{RESET}[{GREEN}{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}{RESET}] ",
        now.year(),
        now.month(),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        now.timestamp_subsec_millis(),
        RESET = "\x1b[0m",
        GREEN = "\x1b[32m",
    )
}

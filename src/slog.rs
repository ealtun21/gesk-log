use crossterm::style::Stylize;
use inquire::validator::Validation;
use inquire::CustomType;
use inquire::{InquireError, Select};
use serialport::available_ports;
use std::fs::{create_dir_all, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::utils::generate_timestamp;

pub fn slog_main(init: bool) -> Result<(), Box<dyn std::error::Error>> {
    let options = available_ports().expect("Failed to detect ports");
    if options.is_empty() {
        if init {
            eprintln!("Waiting for serial interfaces...");
        }
        thread::sleep(Duration::from_millis(100));
        return slog_main(false);
    }

    let port_path = match Select::new(
        "Select the port to read from:",
        options.clone().into_iter().map(|o| o.port_name).collect(),
    )
    .prompt()
    {
        Ok(k) => k,
        Err(InquireError::OperationInterrupted) => return Ok(()),
        Err(_) => return slog_main(true), // Restarts to check for more iterfaces.
    };
    let split_char_result: Option<String> = loop {
        match CustomType::new("Select the split char:")
            .with_help_message("esc for default")
            .with_validator(|a: &String| {
                if let Some(escaped_char) = process_escape_sequence(a) {
                    if escaped_char.is_ascii() {
                        return Ok(Validation::Valid);
                    }
                }

                if a.len() == 1 && a.chars().next().unwrap().is_ascii() {
                    Ok(Validation::Valid)
                } else {
                    Ok(Validation::Invalid(inquire::validator::ErrorMessage::from(
                        "Split char must be ascii or a valid escape sequence".to_owned(),
                    )))
                }
            })
            .prompt_skippable()
        {
            Ok(k) => break k,
            Err(InquireError::OperationInterrupted) => return Ok(()),
            Err(_) => {
                eprintln!("{}", "Please type a correct value".red().slow_blink());
                continue;
            }
        }
    };

    let split_char: char = if let Some(k) = split_char_result {
        if k.len() == 1 {
            k.chars().next().unwrap()
        } else {
            process_escape_sequence(&k).unwrap()
        }
    } else {
        '\n'
    };

    let baud = loop {
        match CustomType::new("What is the baud rate?:")
            .with_error_message("Please type a valid number")
            .with_help_message("esc for default")
            .prompt_skippable()
        {
            Ok(ans) => break ans,
            Err(InquireError::OperationInterrupted) => return Ok(()),
            Err(_) => eprintln!("{}", "Please type a correct value".red().slow_blink()),
        }
    }
    .unwrap_or(115200);

    let output: Option<String> = loop {
        match CustomType::new("What is the output file name?:")
            .with_error_message("Please type a valid file name")
            .with_help_message(
                "esc to skip outputing to a file",
            )
            .prompt_skippable()
        {
            Ok(ans) => break ans,
            Err(InquireError::OperationInterrupted) => return Ok(()),
            Err(_) => eprintln!("{}", "Please type a correct value".red().slow_blink()),
        }
    };

    let port = serialport::new(&port_path, baud)
        .timeout(Duration::from_millis(10))
        .open();

    match port {
        Ok(mut port) => {
            let mut serial_buf: Vec<u8> = vec![0; 1000];
            println!("Receiving data on {} at {} baud:", &port_path, baud);
            let mut accumulated_data = Vec::new();

            loop {
                match port.read(serial_buf.as_mut_slice()) {
                    Ok(t) => {
                        accumulated_data.extend_from_slice(&serial_buf[..t]);

                        // Split the accumulated data by newlines
                        while let Some(pos) =
                            accumulated_data.iter().position(|&x| x == split_char as u8)
                        {
                            let line = accumulated_data.drain(..=pos).collect::<Vec<u8>>();
                            let timestamp = generate_timestamp().into_bytes();

                            let mut data = Vec::with_capacity(timestamp.len() + line.len());
                            data.extend_from_slice(&timestamp);
                            data.extend_from_slice(&line);

                            if let Ok(string) = std::str::from_utf8(&data) {
                                print!("{}", string);
                            } else {
                                eprintln!("Bytes are not valid UTF-8");
                            }
                            if let Some(ref file) = &output {
                                if !Path::new("slog").exists() {
                                    create_dir_all("slog").expect("Unable to create dir");
                                }

                                let mut file = match OpenOptions::new()
                                    .write(true)
                                    .append(true)
                                    .create(true)
                                    .open(format!("slog/{file}"))
                                {
                                    Ok(file) => file,
                                    Err(e) => {
                                        eprintln!(
                                            "Failed to open \"{}\". Error: {}",
                                            output.as_ref().unwrap().as_str(),
                                            e
                                        );
                                        ::std::process::exit(1);
                                    }
                                };
                                file.write_all(&data).unwrap();
                                file.flush().unwrap();
                            }
                        }
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
                    Err(ref e) if e.kind() == io::ErrorKind::BrokenPipe => return slog_main(true), // Restart
                    Err(e) => eprintln!("{e:?}"),
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to open \"{}\". Error: {}", &port_path, e);
            ::std::process::exit(1);
        }
    }
}

fn process_escape_sequence(s: &str) -> Option<char> {
    match s {
        "\'" => Some('\''),
        "\\\"" => Some('\"'),
        "\\\\" => Some('\\'),
        "\\n" => Some('\n'),
        "\\r" => Some('\r'),
        "\\t" => Some('\t'),
        "\\b" => Some('\u{0008}'), // backspace
        "\\f" => Some('\u{000C}'), // form feed
        "\\v" => Some('\u{000B}'), // vertical tab
        "\\0" => Some('\u{0000}'), // null character
        _ if s.starts_with("\\x") => {
            let hex_part = &s[2..];
            u8::from_str_radix(hex_part, 16)
                .ok()
                .map(|byte| byte as char)
        }
        _ => None,
    }
}

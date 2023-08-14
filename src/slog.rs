use crossterm::style::Stylize;
use inquire::CustomType;
use inquire::{InquireError, Select};
use serialport::available_ports;
use std::fs::OpenOptions;
use std::io::Write;
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

    let port_path = loop {
        match Select::new(
            "Select the port to read from",
            options.clone().into_iter().map(|o| o.port_name).collect(),
        )
        .prompt()
        {
            Ok(k) => break k,
            Err(InquireError::OperationInterrupted) => return Ok(()),
            Err(_) => return slog_main(false), // Restarts to check for more iterfaces.
        }
    };

    let baud = loop {
        match CustomType::new("What is the baud rate?: ")
            .with_error_message("Please type a valid number")
            .with_help_message("Type the baud rate you want to use")
            .prompt()
        {
            Ok(ans) => break ans,
            Err(InquireError::OperationInterrupted) => return Ok(()),
            Err(_) => eprintln!("{}", "Please type a correct value".red().slow_blink()),
        }
    };

    let output: Option<String> = loop {
        match CustomType::new("What is the output file name?: ")
            .with_error_message("Please type a valid file name")
            .with_help_message(
                "You may also give a path if you wish, esc to skip outputing to a file",
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
                        while let Some(pos) = accumulated_data.iter().position(|&x| x == b'\n') {
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
                                let mut file = match OpenOptions::new()
                                    .write(true)
                                    .append(true)
                                    .create(true)
                                    .open(file)
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

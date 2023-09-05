#![allow(dead_code)]

use std::{
    fs::{create_dir_all, OpenOptions},
    io::{self, Write},
    path::Path,
    thread,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Result};
use crossterm::style::Stylize;
use inquire::{CustomType, InquireError, Select};
use serialport::available_ports;

use crate::utils::generate_timestamp;

pub fn tlog_main(init: bool) -> Result<(), Box<dyn std::error::Error>> {
    let options = available_ports().expect("Failed to detect ports");
    if options.is_empty() {
        if init {
            eprintln!("Waiting for serial interfaces...");
        }
        thread::sleep(Duration::from_millis(100));
        return tlog_main(false);
    }

    let port_path = match Select::new(
        "Select the port to read from:",
        options
            .clone()
            .into_iter()
            .map(|o| {
                if o.port_name.starts_with("/sys/class/") {
                    o.port_name.replace("/sys/class", "/dev/")
                } else {
                    o.port_name
                }
            })
            .collect(),
    )
    .prompt()
    {
        Ok(k) => k,
        Err(InquireError::OperationInterrupted) => return Ok(()),
        Err(_) => return tlog_main(true), // Restarts to check for more iterfaces.
    };

    let baud = loop {
        match CustomType::new("What is the baud rate:")
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

    let time_out = loop {
        match CustomType::new("What is the timeout in seconds:")
            .with_error_message("Please type a valid number")
            .with_help_message("esc for default")
            .prompt_skippable()
        {
            Ok(ans) => break ans,
            Err(InquireError::OperationInterrupted) => return Ok(()),
            Err(_) => eprintln!("{}", "Please type a correct value".red().slow_blink()),
        }
    }
    .unwrap_or(5);

    let output: Option<String> = loop {
        match CustomType::new("What is the output file name?:")
            .with_error_message("Please type a valid file name")
            .with_help_message("esc to skip outputing to a file")
            .prompt_skippable()
        {
            Ok(ans) => break ans,
            Err(InquireError::OperationInterrupted) => return Ok(()),
            Err(_) => eprintln!("{}", "Please type a correct value".red().slow_blink()),
        }
    };

    match serialport::new(&port_path, baud).open() {
        Ok(mut port) => {
            let mut serial_buf = [0; 1];
            let mut accumulated_data = Vec::new();

            // Introduce the timestamp variable
            let mut last_packet_detected: Option<Instant> = None;
            let time_out_duration: Duration = Duration::from_secs(time_out);

            loop {
                match port.read_exact(&mut serial_buf) {
                    Ok(_) => {
                        accumulated_data.extend_from_slice(&serial_buf[..]);

                        while let Some(start_pos) = accumulated_data.iter().position(|&x| x == 0x1A)
                        {
                            // Check if we have at least the first 5 bytes (0x1A, len1, len2, 0x1, type)
                            if accumulated_data.len() > start_pos + 4 {
                                // Update the timestamp every time we detect the start of a packet
                                last_packet_detected = Some(Instant::now());

                                let len_bytes = [
                                    accumulated_data[start_pos + 1],
                                    accumulated_data[start_pos + 2],
                                ];
                                let payload_len = u16::from_be_bytes(len_bytes) as usize;

                                // Ensure we have all bytes of the message
                                if accumulated_data.len() >= start_pos + payload_len + 5 {
                                    let data_packet = accumulated_data
                                        .drain(start_pos..start_pos + payload_len + 5)
                                        .collect::<Vec<u8>>();
                                    match TLog::from_be_bytes(data_packet) {
                                        Ok(tlog) => {
                                            let timestamp = generate_timestamp().into_bytes();

                                            let colored_message = match tlog.payload_type {
                                                PayloadType::Debug => {
                                                    format!("\x1b[0m\x1b[36m[Debug]\x1b[0m ")
                                                } // Cyan color for Debug
                                                PayloadType::Warning => {
                                                    format!("\x1b[0m\x1b[33m[Warning]\x1b[0m ")
                                                } // Yellow color for Warning
                                                PayloadType::Error => {
                                                    format!("\x1b[0m\x1b[31m[Error]\x1b[0m ")
                                                } // Red color for Error
                                                PayloadType::Unknown => {
                                                    format!("\x1b[0m\x1b[37m[Unknown]\x1b[0m ")
                                                } // White color for Unknown
                                            };

                                            // Less resizing when using with_capacity
                                            let mut data = Vec::with_capacity(
                                                timestamp.len()
                                                    + tlog.payload.len()
                                                    + 1
                                                    + colored_message.len(),
                                            );

                                            data.extend_from_slice(&timestamp);
                                            data.extend_from_slice(&colored_message.as_bytes());
                                            data.extend_from_slice(&tlog.payload.as_bytes());
                                            data.extend_from_slice(String::from("\n").as_bytes());

                                            if let Ok(string) = std::str::from_utf8(&data) {
                                                print!("{}", string);
                                            } else {
                                                eprintln!("Bytes are not valid UTF-8");
                                            }
                                            if let Some(ref file) = &output {
                                                if !Path::new("tlog").exists() {
                                                    create_dir_all("tlog")
                                                        .expect("Unable to create dir");
                                                }

                                                let mut file = match OpenOptions::new()
                                                    .write(true)
                                                    .append(true)
                                                    .create(true)
                                                    .open(format!("tlog/{file}.txt"))
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
                                        Err(e) => eprintln!("Error parsing TLog: {}", e),
                                    }
                                } else {
                                    break; // Wait for more data
                                }
                            } else {
                                break; // Wait for more data
                            }
                        }

                        // Check for timeout
                        if let Some(timestamp) = last_packet_detected {
                            if timestamp.elapsed() > time_out_duration {
                                // Drain accumulated_data up to the detected start position
                                if let Some(start_pos) =
                                    accumulated_data.iter().position(|&x| x == 0x1A)
                                {
                                    accumulated_data.drain(..=start_pos);
                                    last_packet_detected = None; // Reset timestamp
                                }
                            }
                        }
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
                    Err(ref e) if e.kind() == io::ErrorKind::BrokenPipe => return tlog_main(true), // Restart
                    Err(e) => eprintln!("{:?}", e),
                }

                // Here, you can handle the accumulated tlogs if necessary.
            }
        }
        Err(e) => {
            eprintln!("Failed to open \"{}\". Error: {}", port_path, e);
            ::std::process::exit(1);
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum PayloadType {
    Debug = 0,
    Warning = 1,
    Error = 2,
    Unknown,
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct TLog {
    payload_type: PayloadType,
    payload: String,
}

impl TLog {
    pub fn new(payload: String, payload_type: PayloadType) -> Self {
        Self {
            payload,
            payload_type,
        }
    }

    pub fn to_packet(&self) -> Result<Vec<u8>> {
        if self.payload.len() > u16::MAX.into() {
            return Err(anyhow!("Too large packet!"));
        }
        let p_type: u8 = match self.payload_type {
            PayloadType::Debug => 0x0,
            PayloadType::Warning => 0x1,
            PayloadType::Error => 0x2,
            PayloadType::Unknown => return Err(anyhow!("Invalid payload type!")),
        };

        let payload_len = (self.payload.len() as u16).to_be_bytes();
        Ok([
            vec![0x1A, payload_len[0], payload_len[1], p_type, 0x1],
            self.payload.as_bytes().to_vec(),
        ]
        .concat())
    }

    pub fn from_be_bytes(data_packet: Vec<u8>) -> Result<Self> {
        // Check initial conditions
        if data_packet.is_empty() {
            return Err(anyhow!("Input is empty"));
        }
        if data_packet[0] != 0x1A || data_packet[4] != 0x1 {
            eprintln!(
                "Invalid packet: {:?}",
                data_packet
                    .iter()
                    .map(|byte| format!("{:02x}", byte))
                    .collect::<Vec<String>>()
                    .join(" ")
            );
            return Err(anyhow!(
                "Invalid or unsupported input format, Byte[0]:{:X}, Byte[3]:{:X}",
                data_packet[0],
                data_packet[3]
            ));
        }

        let payload_len = u16::from_be_bytes([data_packet[1], data_packet[2]]) as usize;

        // Ensure packet size consistency
        if data_packet.len() != payload_len + 5 {
            return Err(anyhow!("Inconsistent data packet length"));
        }

        let payload_type = match data_packet[3] {
            0 => PayloadType::Debug,
            1 => PayloadType::Warning,
            2 => PayloadType::Error,
            _ => PayloadType::Unknown,
        };

        let payload = std::str::from_utf8(&data_packet[5..(payload_len + 5)])
            .map_err(|e| anyhow!("UTF8 conversion error: {}", e))?
            .to_owned();

        Ok(Self {
            payload_type,
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ascii() {
        let tlog = TLog::new(
            "I want to some very nice food!".to_owned(),
            PayloadType::Debug,
        );
        let packet = tlog.to_packet().unwrap();
        let out_log = TLog::from_be_bytes(packet).unwrap();

        assert_eq!(tlog, out_log);
    }

    #[test]
    fn test_utf8() {
        let tlog = TLog::new(
            "素早い茶色のキツネが怠惰な犬を飛び越える".to_owned(),
            PayloadType::Debug,
        );
        let packet = tlog.to_packet().unwrap();
        let out_log = TLog::from_be_bytes(packet).unwrap();

        assert_eq!(tlog, out_log);
    }

    #[test]
    fn test_utf8_icon() {
        let tlog = TLog::new("".to_owned(), PayloadType::Debug);
        let packet = tlog.to_packet().unwrap();
        let out_log = TLog::from_be_bytes(packet).unwrap();

        assert_eq!(tlog, out_log);
    }
}

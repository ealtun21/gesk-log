use crossterm::style::Stylize;
use inquire::{
    validator::Validation, Confirm, CustomType, InquireError, Password, PasswordDisplayMode,
};
use rumqttc::{
    AsyncClient, ConnectReturnCode, Event, EventLoop, MqttOptions, Packet, Publish, QoS,
    SubscribeReasonCode,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{create_dir_all, File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
    time::Duration,
};

use serde_json::value::RawValue;

use crate::utils::generate_timestamp;

#[derive(Debug, Serialize, Deserialize)]
struct PartialArgsFromFile {
    /// Domain name or IP address of the broker
    broker: Box<RawValue>,

    /// Port on which the broker is expected to listen for incoming connections
    port: Box<RawValue>,

    /// Topics to be monitored
    topics: Box<RawValue>,

    /// Identifier for the device connecting to the broker
    id: Box<RawValue>,

    /// Duration in seconds to wait before pinging the broker if there's no other communication
    keep_alive: Box<RawValue>,

    /// Number of concurrent in flight messages
    inflight: Box<RawValue>,

    /// Credentials for logging in: username followed by password
    auth: Box<RawValue>,

    /// Clean Session
    clean_session: Box<RawValue>,
}

impl PartialArgsFromFile {
    fn new() -> Option<Self> {
        let file = File::open("mlog_config.json").ok();
        let mut buffer = Vec::new();

        if let Some(mut file) = file {
            let __ = file.read_to_end(&mut buffer);

            return serde_json::from_slice(&buffer).ok();
        }

        None
    }
}

/// MQTT Logger
#[derive(Debug, Serialize, Deserialize)]
struct Args {
    /// Domain name or IP address of the broker
    broker: String,

    /// Port on which the broker is expected to listen for incoming connections
    port: u16,

    /// Topics to be monitored
    topics: Vec<String>,

    /// Identifier for the device connecting to the broker
    id: String,

    /// Duration in seconds to wait before pinging the broker if there's no other communication
    keep_alive: u64,

    /// Number of concurrent in flight messages
    inflight: Option<u16>,

    /// Credentials for logging in: username followed by password
    auth: Vec<String>,

    /// Clean Session
    clean_session: bool,
}

fn clean_str(s: &str) -> String {
    let reserved_chars = "!*'();:@&=+$,/?#[]";
    s.chars()
        .filter(|&c| !reserved_chars.contains(c) && c != '\\' && c != '"')
        .collect()
}

impl Args {
    fn parse() -> Self {
        let args_from_file = PartialArgsFromFile::new();

        let mut amount_of_changes = 0;

        let broker = match args_from_file.as_ref().map(|a| clean_str(a.broker.get())) {
            Some(val) if !val.is_empty() => val,
            _ => {
                amount_of_changes += 1;
                get_broker()
            }
        };

        let port = match args_from_file
            .as_ref()
            .map(|a| a.port.get())
            .and_then(|s| s.parse::<u16>().ok())
        {
            Some(val) => val,
            _ => {
                amount_of_changes += 1;
                get_port()
            }
        };

        let id = match args_from_file.as_ref().map(|a| clean_str(a.id.get())) {
            Some(val) if !val.is_empty() => val.to_string(),
            _ => {
                amount_of_changes += 1;
                get_id()
            }
        };

        let topics = match args_from_file
            .as_ref()
            .map(|a| a.topics.get())
            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        {
            Some(val) if !val.is_empty() => val,
            _ => {
                amount_of_changes += 1;
                get_topics()
            }
        };

        let keep_alive = match args_from_file
            .as_ref()
            .map(|a| a.keep_alive.get())
            .and_then(|s| s.parse::<u64>().ok())
        {
            Some(val) => val,
            _ => {
                amount_of_changes += 1;
                get_keep_alieve()
            }
        };

        let inflight = args_from_file
            .as_ref()
            .and_then(|a| {
                let str_val = a.inflight.get();
                if str_val == "null" {
                    return None;
                }
                str_val.parse::<u16>().ok()
            })
            .or_else(|| {
                if args_from_file.as_ref().map(|a| a.inflight.get()) == Some("null") {
                    None
                } else {
                    {
                        amount_of_changes += 1;
                        get_inflight()
                    }
                }
            });

        let auth = match args_from_file
            .as_ref()
            .map(|a| a.auth.get())
            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        {
            Some(val) => val,
            _ => {
                amount_of_changes += 1;
                get_credentials()
            }
        };

        let clean_session = match args_from_file
            .as_ref()
            .map(|a| a.clean_session.get())
            .and_then(|s| s.parse::<bool>().ok())
        {
            Some(val) => val,
            _ => {
                amount_of_changes += 1;
                get_clean_session()
            }
        };

        let mut res = Self {
            broker,
            port,
            id,
            topics,
            keep_alive,
            inflight,
            auth,
            clean_session,
        };

        if let Ok(json_ver) = serde_json::to_string_pretty(&res) {
            if amount_of_changes != 0 {
                save_to_json(json_ver);
            }
        }

        if res.id == "random" {
            res.id = format!("mlog-{}", rand::random::<u16>())
        };

        println!("Loaded State: \n{:#?}", &res);

        res
    }
}

fn save_to_json(json_ver: String) {
    let write = loop {
        match Confirm::new("Do you want to save this as a config file?")
            .with_default(true)
            .prompt()
        {
            Ok(ans) => break ans,
            Err(InquireError::OperationInterrupted) => ::std::process::exit(1),
            Err(_) => eprintln!("{}", "Please type a correct value".red().slow_blink()),
        }
    };

    if write {
        let file = match OpenOptions::new()
            .truncate(true)
            .write(true)
            .create(true)
            .open("mlog_config.json")
        {
            Ok(f) => Some(f),
            Err(e) => {
                eprintln!("Failed to create/open file for config {e}");
                None
            }
        };

        if let Some(mut file) = file {
            file.write_all(json_ver.as_bytes()).unwrap();
            file.flush().unwrap();
        }
    }
}

fn get_clean_session() -> bool {
    loop {
        match Confirm::new("Do you want a clean session?")
            .with_default(false)
            .prompt()
        {
            Ok(ans) => break ans,
            Err(InquireError::OperationInterrupted) => ::std::process::exit(1),
            Err(_) => eprintln!("{}", "Please type a correct value".red().slow_blink()),
        }
    }
}

fn get_inflight() -> Option<u16> {
    loop {
        match CustomType::new("Enter the number of concurrent inflight messages:")
            .with_help_message("esc for default")
            .prompt_skippable()
        {
            Ok(ans) => break ans,
            Err(InquireError::OperationInterrupted) => ::std::process::exit(1),
            Err(_) => eprintln!("{}", "Please type a correct value".red().slow_blink()),
        }
    }
}

fn get_keep_alieve() -> u64 {
    loop {
        match CustomType::new("Duration in seconds to wait before pinging the broker if there's no other communication:")
            .with_help_message("esc for default")
            .prompt_skippable()
        {
            Ok(ans) => break ans,
            Err(InquireError::OperationInterrupted) => ::std::process::exit(1),
            Err(_) => eprintln!("{}", "Please type a correct value".red().slow_blink()),
        }
    }.unwrap_or(5)
}

fn get_id() -> String {
    match loop {
        match CustomType::new("Identifier for the device connecting to the broker:")
            .with_help_message("esc for default")
            .prompt_skippable()
        {
            Ok(ans) => break ans,
            Err(InquireError::OperationInterrupted) => ::std::process::exit(1),
            Err(_) => eprintln!("{}", "Please type a correct value".red().slow_blink()),
        }
    } {
        Some(ans) => ans,
        None => "random".to_string(),
    }
}

fn get_port() -> u16 {
    loop {
        match CustomType::new(
            "Enter the port on which the broker is expected to listen for incoming connections:",
        )
        .with_error_message("Please type a valid port")
        .prompt()
        {
            Ok(ans) => break ans,
            Err(InquireError::OperationInterrupted) => ::std::process::exit(1),
            Err(_) => eprintln!("{}", "Please type a correct value".red().slow_blink()),
        }
    }
}

fn get_broker() -> String {
    let validator = |input: &String| {
        if input.trim().is_empty() {
            Ok(Validation::Invalid("Input must not be empty".into()))
        } else {
            Ok(Validation::Valid)
        }
    };

    loop {
        match CustomType::new("Enter the domain name or IP address of the broker:")
            .with_error_message("Please type a domain name or IP address")
            .with_validator(validator)
            .prompt()
        {
            Ok(ans) => break ans,
            Err(InquireError::OperationInterrupted) => ::std::process::exit(1),
            Err(_) => eprintln!("{}", "Please type a correct value".red().slow_blink()),
        }
    }
    .trim()
    .to_owned()
}

fn get_topics() -> Vec<String> {
    loop {
        let mut topics: Vec<String> = Vec::new();
        'asking_loop: loop {
            if let Some(ans) = loop {
                match CustomType::new("Add a topic:")
                    .with_help_message("esc to stop")
                    .prompt_skippable()
                {
                    Ok(ans) => break ans,
                    Err(InquireError::OperationInterrupted) => ::std::process::exit(1),
                    Err(_) => eprintln!("{}", "Please type a correct value".red().slow_blink()),
                }
            } {
                topics.push(ans);
            } else {
                break 'asking_loop;
            }
        }

        if topics.is_empty() {
            eprintln!("{}", "Topics can not be none".red().slow_blink());
            continue;
        }

        break topics;
    }
}

fn get_credentials() -> Vec<String> {
    let mut credentials: Vec<String> = Vec::new();

    // Prompt for the username
    let username = loop {
        match CustomType::new("Enter your username:")
            .with_help_message("esc to skip")
            .prompt_skippable()
        {
            Ok(ans) => break ans,
            Err(InquireError::OperationInterrupted) => return vec![], // return empty vector if interrupted
            Err(_) => eprintln!("{}", "Please type a correct value".red().slow_blink()),
        }
    };

    if let Some(username) = username {
        credentials.push(username);

        let password = loop {
            match Password::new("Password:")
                .with_display_toggle_enabled()
                .with_display_mode(PasswordDisplayMode::Masked)
                .with_custom_confirmation_message("Password (confirm):")
                .with_custom_confirmation_error_message("The passwords don't match.")
                .with_formatter(&|_| String::from("Input received"))
                .prompt()
            {
                Ok(ans) => break ans,
                Err(InquireError::OperationInterrupted) => return vec![],
                Err(_) => {
                    eprintln!("{}", "Please type a correct value".red().slow_blink());
                    continue;
                }
            };
        };

        credentials.push(password);
    };

    credentials
}

pub async fn mlog_main() -> std::io::Result<()> {
    let args = Args::parse();

    let mqttoptions = configure_mqtt(&args);

    let (client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

    let mut files = initialize_files_and_subscriptions(&client, &args.topics).await;

    process_events(&mut eventloop, &mut files).await
}

fn configure_mqtt(args: &Args) -> MqttOptions {
    let mut mqttoptions = MqttOptions::new(&args.id, &args.broker, args.port);

    if !args.auth.is_empty() {
        mqttoptions.set_credentials(args.auth[0].clone(), args.auth[1].clone());
    }
    if let Some(inflight) = args.inflight {
        mqttoptions.set_inflight(inflight);
    }
    mqttoptions.set_clean_session(args.clean_session);
    mqttoptions.set_keep_alive(Duration::from_secs(args.keep_alive));

    mqttoptions
}

async fn initialize_files_and_subscriptions(
    client: &AsyncClient,
    topics: &[String],
) -> HashMap<String, File> {
    let mut files = HashMap::new();
    for topic in topics {
        if client.subscribe(topic, QoS::ExactlyOnce).await.is_err() {
            eprintln!("Failed to subscribe to {topic}");
        }

        if !Path::new("mlog").exists() {
            create_dir_all("mlog").expect("Unable to create dir");
        }
        files.insert(
            topic.clone(),
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(format!("mlog/{topic}.txt"))
                .expect("Unable to create files"),
        );
    }
    files
}

async fn process_events(
    eventloop: &mut EventLoop,
    files: &mut HashMap<String, File>,
) -> std::io::Result<()> {
    println!("Waiting for events...");
    loop {
        match eventloop.poll().await {
            Ok(notification) => match notification {
                Event::Incoming(p) => match p {
                    Packet::Publish(p) => {
                        let timestamp = generate_timestamp().into_bytes();
                        write_to_file(&timestamp, &p, files);
                        write_to_stdout(&timestamp, &p);
                    }
                    Packet::SubAck(s) => {
                        for code in s.return_codes {
                            if code == SubscribeReasonCode::Failure {
                                eprintln!("Got a subscribe fail packet!");
                            }
                        }
                    }
                    Packet::ConnAck(c) if c.code == ConnectReturnCode::Success => {
                        println!("Connection established");
                    }
                    Packet::Disconnect => println!("Got disconnect"),
                    _ => (),
                },
                Event::Outgoing(_) => (),
            },
            Err(e) => {
                eprintln!("{e}");
                break;
            }
        }
    }

    Ok(())
}

fn write_to_file(timestamp: &Vec<u8>, data: &Publish, files: &HashMap<String, File>) {
    let mut res = Vec::with_capacity(data.payload.len() + timestamp.len());

    res.extend_from_slice(timestamp);
    res.extend_from_slice(&data.payload);
    res.extend_from_slice("\n".as_bytes());

    match files.get(data.topic.as_str()) {
        Some(mut file) => {
            file.write_all(&res).unwrap();
            file.flush().unwrap();
        }
        None => eprintln!(
            "Got packet from topic {}, but that topic file was not created!",
            data.topic
        ),
    };
}

fn write_to_stdout(timestamp: &Vec<u8>, data: &Publish) {
    let mut res = Vec::with_capacity(data.payload.len() + timestamp.len());

    res.extend_from_slice(timestamp);
    res.extend_from_slice(
        format!(
            "{RESET}[{BLUE}{}{RESET}] ",
            data.topic.as_str(),
            RESET = "\x1b[0m",
            BLUE = "\x1b[34m"
        )
        .as_bytes(),
    );
    res.extend_from_slice(&data.payload);
    res.extend_from_slice("\n".as_bytes());

    io::stdout().write_all(&res).unwrap();
    ::std::io::stdout().flush().unwrap();
}

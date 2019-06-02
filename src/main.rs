use std::{
    fs,
    io::{stdin, stdout, BufRead, Write},
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::Duration,
};

use clap::{App, Arg, ArgMatches, SubCommand};
use colored::Colorize;
use coral::*;
use notify::{watcher, DebouncedEvent, RecursiveMode, Result, Watcher};
use pad::{Alignment, PadStr};
use toml::Value;

#[derive(Clone, Copy)]
struct Params {
    watch: bool,
    debug: bool,
    color: bool,
    checker: Checker,
}

impl Params {
    fn new(watch: bool, matches: &ArgMatches) -> Params {
        Params {
            watch,
            debug: matches.is_present("debug"),
            color: !matches.is_present("nocolor"),
            checker: if matches.is_present("clippy") {
                Checker::Clippy
            } else if matches.is_present("build") {
                Checker::Build
            } else {
                Checker::Check
            },
        }
    }
}

fn run(params: Params) -> Vec<Entry> {
    print::headers(params.color);
    let entries: Vec<_> = Analyzer::with_args(params.checker, &[])
        .unwrap()
        .debug(params.debug)
        .color(params.color)
        .inspect(|entry| {
            if entry.is_artifact() {
                print!(
                    "{}\r",
                    format!("compiling {}", entry.package_id)
                        .pad_to_width_with_alignment(terminal_width(), Alignment::Left)
                );
                let _ = stdout().flush();
            }
        })
        .filter(|entry| entry.report().is_some())
        .enumerate()
        .inspect(|(i, entry)| print::entry(*i, entry))
        .map(|(_, entry)| entry)
        .collect();
    if entries.is_empty() {
        let no_problems =
            "No problems".pad_to_width_with_alignment(terminal_width(), Alignment::Left);
        let no_problems = if params.color {
            no_problems.bright_green().to_string()
        } else {
            no_problems
        };
        println!("{}", no_problems);
    } else {
        let errors = entries.iter().cloned().filter(Entry::is_error).count();
        let warnings = entries.iter().cloned().filter(Entry::is_warning).count();
        let warnings_text = format!("warning{}", if warnings == 1 { "" } else { "s" });
        let errors_text = format!("error{}", if errors == 1 { "" } else { "s" });
        let (warnings_text, errors_text) = if params.color {
            (
                warnings_text.bright_yellow().to_string(),
                errors_text.bright_red().to_string(),
            )
        } else {
            (warnings_text, errors_text)
        };
        let problem_count = format!("{} {}, {} {}", errors, errors_text, warnings, warnings_text)
            .pad_to_width_with_alignment(terminal_width(), Alignment::Left);
        println!("{}", problem_count);
    }
    if params.watch {
        print::prompt();
    }
    entries
}

macro_rules! init_command {
    ($command:expr) => {
        $command
            .arg(
                Arg::with_name("nocolor")
                    .help("Disable colored output")
                    .short("n")
                    .long("nocolor"),
            )
            .arg(
                Arg::with_name("build")
                    .help("Check with cargo build")
                    .short("b")
                    .long("build"),
            )
            .arg(
                Arg::with_name("clippy")
                    .help("Check with clippy")
                    .short("c")
                    .long("clippy"),
            )
            .arg(
                Arg::with_name("debug")
                    .help("Output generated json to the standard output and a file")
                    .short("d")
                    .long("debug"),
            )
    };
}

fn top_app<'a, 'b>() -> App<'a, 'b> {
    init_command!(App::new("coral")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Compact Rust compiler messages"))
    .subcommand(init_command!(SubCommand::with_name("watch")
        .alias("w")
        .alias("reef")
        .about("watch for changes to files and recompile if necessary")))
}

fn command_exits(command: &str) -> bool {
    match command.trim() {
        "quit" | "exit" | "q" => true,
        _ => false,
    }
}

fn commands() -> (JoinHandle<()>, Receiver<String>) {
    let (send, recv) = mpsc::channel();
    let handle = thread::spawn(move || {
        for command in stdin().lock().lines().filter_map(std::result::Result::ok) {
            if !command.trim().is_empty() {
                let _ = send.send(command.clone());
            }
            if command_exits(&command) {
                return;
            }
        }
    });
    (handle, recv)
}

static COMMAND_HELP: &str = r#"
Commands:
    <index>      expand the message at the index
    fix <index>  apply the compiler-suggested fix, if there is one
    quit         quit watching
    help         display this message
"#;

fn main() -> Result<()> {
    let app = top_app();
    let matches = app.get_matches();
    match matches.subcommand() {
        // watch subcommand
        ("watch", Some(matches)) => {
            let params = Params::new(true, matches);
            let mut entries = run(params);
            let (handle, command_rx) = commands();
            let (event_tx, event_rx) = mpsc::channel();
            let mut watcher = watcher(event_tx, Duration::from_secs(2))?;
            // watch src
            if PathBuf::from("src").exists() {
                watcher.watch("src", RecursiveMode::Recursive)?;
            }
            // watch other stuff in the workspace
            if let Ok(bytes) = fs::read("Cargo.toml") {
                if let Ok(Value::Table(manifest)) = toml::from_slice::<Value>(&bytes) {
                    if let Some(Value::Table(workspace)) = manifest.get("workspace") {
                        if let Some(Value::Array(members)) = workspace.get("members") {
                            for member in members.iter().filter_map(Value::as_str) {
                                watcher.watch(member, RecursiveMode::Recursive)?;
                            }
                        }
                    }
                }
            }
            // watch loop
            loop {
                // get watch events
                if let Ok(event) = event_rx.try_recv() {
                    if let DebouncedEvent::Write(_) = event {
                        entries = run(params);
                    }
                }
                // get commands
                if let Ok(command) = command_rx.try_recv() {
                    match command.trim() {
                        "help" => println!("{}", COMMAND_HELP),
                        command if command.starts_with("fix ") => {
                            let res = if let Some(index_str) = command.split_whitespace().nth(1) {
                                if let Ok(i) = index_str.parse::<usize>() {
                                    if i < entries.len() {
                                        if let Some(span) = entries[i]
                                            .message
                                            .as_ref()
                                            .and_then(Message::replacement_span)
                                        {
                                            match span.clone().replace_in_file() {
                                                Ok(()) => Ok(()),
                                                Err(e) => Err(format!("Error: {}", e)),
                                            }
                                        } else {
                                            Err("No replacement available".into())
                                        }
                                    } else {
                                        Err("Invalid index".into())
                                    }
                                } else {
                                    Err("Index must be a number".into())
                                }
                            } else {
                                Err("Fix which index?".into())
                            };
                            match res {
                                Ok(_) => {
                                    println!("Fixed, recompiling...");
                                }
                                Err(message) => {
                                    println!("{}", message);
                                    print::prompt();
                                }
                            }
                        }
                        command if command_exits(command) => break,
                        command => {
                            if let Ok(i) = command.parse::<usize>() {
                                if let Some(entry) = entries.get(i) {
                                    if let Some(rendered) = entry.rendered() {
                                        println!("{}", rendered);
                                    } else {
                                        println!("No render available");
                                    }
                                } else {
                                    println!("Invalid index");
                                }
                            } else {
                                println!("Unknown command: {:?}\n{}", command, COMMAND_HELP);
                            }
                            print::prompt();
                        }
                    }
                }
                // sleep to reduce cpu time
                thread::sleep(Duration::from_millis(100));
            }
            handle.join().unwrap();
        }
        // no subcommand
        _ => {
            run(Params::new(false, &matches));
        }
    }
    Ok(())
}

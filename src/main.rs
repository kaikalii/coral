use std::{
    fs,
    io::{stdin, stdout, BufRead, Write},
    path::PathBuf,
    rc::Rc,
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

#[derive(Clone)]
struct Params {
    watch: bool,
    debug: bool,
    color: bool,
    checker: Checker,
    args: Rc<Vec<String>>,
}

impl Params {
    fn new(watch: bool, matches: &ArgMatches) -> Params {
        let mut args = Vec::new();
        if matches.is_present("all") {
            args.push("--all".into());
        }
        if let Some(values) = matches.values_of("package") {
            for value in values {
                args.push("--package".into());
                args.push(value.into());
            }
        }
        if let Some(values) = matches.values_of("exclude") {
            for value in values {
                args.push("--exclude".into());
                args.push(value.into());
            }
        }
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
            args: Rc::new(args),
        }
    }
}

fn run(params: Params) -> Vec<Entry> {
    let mut printed_headers = false;
    println!();
    println!();
    let entries: Vec<_> = Analyzer::with_args(params.checker, &params.args)
        .unwrap()
        .debug(params.debug)
        .color(params.color)
        .inspect(|entry| {
            if entry.is_artifact() {
                let mut line = format!("compiled {}", entry.package_id)
                    .pad_to_width_with_alignment(terminal_width(), Alignment::Left);
                line.truncate(terminal_width());
                print!("{}\r", line);
                let _ = stdout().flush();
            }
        })
        .filter(|entry| entry.report().is_some())
        .enumerate()
        .inspect(|(i, entry)| {
            if !printed_headers {
                print::headers(params.color);
                printed_headers = true;
            }
            print::entry(*i, entry);
        })
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
        let mut problem_count = if errors > 0 {
            format!("{} {}", errors, errors_text)
        } else {
            String::new()
        };
        if warnings > 0 {
            if errors > 0 {
                problem_count.push_str(", ");
            }
            problem_count.push_str(&format!("{} {}", warnings, warnings_text));
        }
        let problem_count =
            problem_count.pad_to_width_with_alignment(terminal_width(), Alignment::Left);
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
            .arg(
                Arg::with_name("all")
                    .help("Check all packages in the workspace")
                    .long("all"),
            )
            .arg(
                Arg::with_name("package")
                    .help("Package(s) to check")
                    .short("p")
                    .long("package")
                    .takes_value(true)
                    .multiple(true),
            )
            .arg(
                Arg::with_name("exclude")
                    .help("Exclude packages from the check")
                    .long("exclude")
                    .takes_value(true)
                    .multiple(true),
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
        // Watch subcommand
        ("watch", Some(matches)) => {
            let params = Params::new(true, matches);
            let mut entries = run(params.clone());
            let (handle, command_rx) = commands();
            let (event_tx, event_rx) = mpsc::channel();
            let mut watcher = watcher(event_tx, Duration::from_secs(2))?;
            // Watch src
            if PathBuf::from("src").exists() {
                watcher.watch("src", RecursiveMode::Recursive)?;
            }
            // Watch other stuff in the workspace
            if let Ok(bytes) = fs::read("Cargo.toml") {
                // Watch Cargo.toml
                watcher.watch("Cargo.toml", RecursiveMode::Recursive)?;
                // Read manifest
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
            // Watch loop
            loop {
                // Get watch events
                let mut got_event = false;
                while let Ok(event) = event_rx.try_recv() {
                    if let DebouncedEvent::Write(_) = event {
                        got_event = true;
                    }
                }
                if got_event {
                    entries = run(params.clone());
                }
                // Get commands
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
                // Sleep to reduce cpu time
                thread::sleep(Duration::from_millis(100));
            }
            handle.join().unwrap();
        }
        // No subcommand
        _ => {
            run(Params::new(false, &matches));
        }
    }
    Ok(())
}

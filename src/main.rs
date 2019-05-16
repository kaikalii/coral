use std::{
    io::{stdin, stdout, BufRead, Write},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::Duration,
};

use clap::{App, Arg, ArgMatches, SubCommand};
use coral::*;
use notify::{watcher, DebouncedEvent, RecursiveMode, Result, Watcher};
use pad::{Alignment, PadStr};

#[derive(Clone, Copy)]
struct Params {
    debug: bool,
    color: bool,
    checker: Checker,
}

fn params(matches: &ArgMatches) -> Params {
    Params {
        debug: matches.is_present("debug"),
        color: !matches.is_present("nocolor"),
        checker: if matches.is_present("clippy") {
            Checker::Clippy
        } else {
            Checker::Check
        },
    }
}

fn run(params: Params) -> Vec<Entry> {
    println!("\n    {}", Message::report_headers(params.color));
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
        .inspect(|(i, entry)| {
            println!(
                "{} {}",
                i.to_string()
                    .pad_to_width_with_alignment(3, Alignment::Right),
                entry.report_width(terminal_width() - 4).unwrap()
            )
        })
        .map(|(_, entry)| entry)
        .collect();
    print!(
        "{}\r",
        (0..terminal_width()).map(|_| ' ').collect::<String>()
    );
    let _ = stdout().flush();
    entries
}

macro_rules! init_command {
    ($command:expr) => {
        $command
            .arg(
                Arg::with_name("nocolor")
                    .help("Disable colored output")
                    .long("nocolor"),
            )
            .arg(
                Arg::with_name("clippy")
                    .help("Check with clippy")
                    .long("clippy"),
            )
            .arg(
                Arg::with_name("debug")
                    .help("Output generated json to the standard output and a file")
                    .long("debug"),
            )
    };
}

fn top_app<'a, 'b>() -> App<'a, 'b> {
    init_command!(App::new("coral")).subcommand(init_command!(SubCommand::with_name("watch")
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

fn main() -> Result<()> {
    let a = 5;
    let app = top_app();
    let matches = app.get_matches();
    match matches.subcommand() {
        ("watch", Some(matches)) => {
            let params = params(matches);
            let mut entries = run(params);
            let (handle, command_rx) = commands();
            let (event_tx, event_rx) = mpsc::channel();
            let mut watcher = watcher(event_tx, Duration::from_secs(2))?;
            watcher.watch("./src", RecursiveMode::Recursive)?;
            'watch_loop: loop {
                if let Ok(event) = event_rx.try_recv() {
                    match event {
                        DebouncedEvent::Write(_) | DebouncedEvent::Create(_) => {
                            entries = run(params);
                        }
                        _ => {}
                    }
                }
                while let Ok(command) = command_rx.try_recv() {
                    match command.trim() {
                        command if command_exits(command) => break 'watch_loop,
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
                                println!("Unknown command: {:?}", command)
                            }
                        }
                    }
                }
            }
            handle.join().unwrap();
        }
        _ => {
            run(params(&matches));
        }
    }
    Ok(())
}

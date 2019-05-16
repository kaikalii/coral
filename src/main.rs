use std::{
    io::{stdin, BufRead},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::Duration,
};

use clap::{App, Arg, ArgMatches, SubCommand};
use coral::*;
use notify::{watcher, DebouncedEvent, RecursiveMode, Result, Watcher};

#[derive(Clone, Copy)]
struct Params {
    color: bool,
    checker: Checker,
}

fn params(matches: &ArgMatches) -> Params {
    Params {
        color: !matches.is_present("nocolor"),
        checker: if matches.is_present("clippy") {
            Checker::Clippy
        } else {
            Checker::Check
        },
    }
}

fn run(params: Params) -> Vec<Entry> {
    println!("{}", Message::report_headers(params.color));
    Analyzer::with_args(params.checker, &[])
        .unwrap()
        .color(params.color)
        .filter(Entry::is_message)
        .inspect(|entry| {
            if let Some(report) = entry.report() {
                println!("{}", report)
            }
        })
        .filter(|entry| entry.report().is_some())
        .collect()
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
    };
}

fn top_app<'a, 'b>() -> App<'a, 'b> {
    init_command!(App::new("coral")).subcommand(init_command!(SubCommand::with_name("watch")
        .about("watch for changes to files and recompile if necessary")))
}

fn commands() -> (JoinHandle<()>, Receiver<String>) {
    let (send, recv) = mpsc::channel();
    let handle = thread::spawn(move || {
        for command in stdin().lock().lines().filter_map(std::result::Result::ok) {
            if !command.trim().is_empty() {
                let _ = send.send(command.clone());
            }
            if command.trim() == "quit" {
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
                        "quit" => break 'watch_loop,
                        s => println!("Unknown command: {:?}", s),
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

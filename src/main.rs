use std::{sync::mpsc, time::Duration};

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

fn run(params: Params) {
    println!("{}", Message::report_headers(params.color));
    for entry in Analyzer::with_args(params.checker, &[])
        .unwrap()
        .color(params.color)
        .filter(Entry::is_message)
    {
        if let Some(report) = entry.report() {
            println!("{}", report)
        }
    }
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

fn main() -> Result<()> {
    let a = 5;
    let app = top_app();
    let matches = app.get_matches();
    match matches.subcommand() {
        ("watch", Some(matches)) => {
            let params = params(matches);
            run(params);
            let (send, recv) = mpsc::channel();
            let mut watcher = watcher(send, Duration::from_secs(2))?;
            watcher.watch("./src", RecursiveMode::Recursive)?;
            while let Ok(event) = recv.recv() {
                match event {
                    DebouncedEvent::Write(_) | DebouncedEvent::Create(_) => run(params),
                    _ => {}
                }
            }
        }
        _ => {
            run(params(&matches));
        }
    }
    Ok(())
}

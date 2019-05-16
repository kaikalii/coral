use std::{sync::mpsc, time::Duration};

use clap::{App, Arg, SubCommand};
use coral::*;
use notify::{watcher, DebouncedEvent, RecursiveMode, Result, Watcher};

fn run(color: bool) {
    println!("{}", Message::report_headers(color));
    for entry in Analyzer::new()
        .unwrap()
        .color(color)
        .filter(Entry::is_message)
    {
        if let Some(report) = entry.report() {
            println!("{}", report)
        }
    }
}

fn top_app<'a, 'b>() -> App<'a, 'b> {
    App::new("coral")
        .arg(
            Arg::with_name("nocolor")
                .help("Disable colored output")
                .long("nocolor"),
        )
        .subcommand(
            SubCommand::with_name("watch")
                .about("watch for changes to files and recompile if necessary")
                .arg(
                    Arg::with_name("nocolor")
                        .help("Disable colored output")
                        .long("nocolor"),
                ),
        )
}

fn main() -> Result<()> {
    let a = 5;
    let app = top_app();
    let matches = app.get_matches();
    match matches.subcommand() {
        ("watch", Some(matches)) => {
            let color = !matches.is_present("nocolor");
            let (send, recv) = mpsc::channel();
            run(color);
            let mut watcher = watcher(send, Duration::from_secs(2))?;
            watcher.watch("./src", RecursiveMode::Recursive)?;
            while let Ok(event) = recv.recv() {
                match event {
                    DebouncedEvent::Write(_) | DebouncedEvent::Create(_) => run(color),
                    _ => {}
                }
            }
        }
        _ => {
            let color = !matches.is_present("nocolor");
            run(color);
        }
    }
    Ok(())
}

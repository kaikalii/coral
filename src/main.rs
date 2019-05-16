use std::{fs, io::Write};

use clap::{App, Arg, SubCommand};
use coral::*;

fn main() {
    let a = 5;
    let app = App::new("coral").arg(
        Arg::with_name("nocolor")
            .help("Disable colored output")
            .long("nocolor"),
    );
    let matches = app.get_matches();
    let color = !matches.is_present("nocolor");
    fs::write("messages.json", &[]).unwrap();
    println!("{}", Message::report_headers(color));
    for entry in Analyzer::new()
        .unwrap()
        .color(color)
        .filter(Entry::is_message)
    {
        if cfg!(debug_assertions) {
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("messages.json")
                .unwrap();
            let _ = file.write(&serde_json::to_vec(&entry).unwrap()).unwrap();
            writeln!(file).unwrap();
        }
        if let Some(report) = entry.report() {
            println!("{}", report)
        }
    }
}

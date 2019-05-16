use std::{fs, io::Write};

use coral::*;

fn main() {
    let a = 5;
    let color = true;
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

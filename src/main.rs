use std::{fs, io::Write};

use coral::*;

fn main() {
    fs::write("messages.json", &[]).unwrap();
    for entry in Analyzer::new().unwrap().filter(Entry::is_message) {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("messages.json")
            .unwrap();
        file.write(&serde_json::to_vec(&entry).unwrap()).unwrap();
        writeln!(file, "").unwrap();
    }
}

/*!
functions for printing `Entry`s
*/

use std::io::{stdout, Write};

use pad::{Alignment, PadStr};

use crate::{terminal_width, Entry, Message};

/// Print a CLI prompt arrow
pub fn prompt() {
    print!(
        "{}\r> ",
        "".pad_to_width_with_alignment(terminal_width(), Alignment::Left)
    );
    let _ = stdout().flush();
}

/// Print an `Entry` with an assigned index
pub fn entry(index: usize, entry: &Entry) {
    if let Some(ref msg) = entry.message {
        for msg in msg.unroll() {
            message(index, entry.color, msg);
        }
    }
}

/// Print a `Message` with an assigned index
pub fn message(index: usize, color: bool, message: &Message) {
    if let Some(report) = message.report(color, terminal_width() - 4) {
        println!(
            "{} {}",
            index
                .to_string()
                .pad_to_width_with_alignment(3, Alignment::Right),
            report
        )
    }
}

/// Print `Entry` column headers
pub fn headers(color: bool) {
    println!("    {}", Message::report_headers(color));
}

/// Print a compact list of `Entry`s
pub fn entries<'a, I>(color: bool, entries: I)
where
    I: IntoIterator<Item = &'a Entry>,
{
    headers(color);
    for (i, entry) in entries.into_iter().enumerate() {
        self::entry(i, entry)
    }
    prompt();
}

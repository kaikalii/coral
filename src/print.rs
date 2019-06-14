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
    println!(
        "{} {}",
        index
            .to_string()
            .pad_to_width_with_alignment(3, Alignment::Right),
        entry.report_width(terminal_width() - 4).unwrap()
    )
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

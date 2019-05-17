#![deny(missing_docs)]

/*!
This crate parses the output of `cargo check --message-format json` into transparent data structures.

The main entrypoint for running cargo and parsing output is the [`Analyzer`](struct.Analyzer.html) struct.
*/

use std::{
    collections::VecDeque,
    error,
    fmt::{self, Debug, Display, Formatter},
    fs,
    io::{self, Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    result,
};

use colored::Colorize;
use pad::{Alignment, PadStr};
use serde_derive::{Deserialize, Serialize};

/// Error type used by coral
#[derive(Debug)]
pub enum Error {
    /// An error running cargo
    Cargo,
    /// An IO error
    IO(io::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        use Error::*;
        match self {
            Cargo => write!(f, "Unable to run cargo"),
            IO(e) => write!(f, "{}", e),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::IO(e)
    }
}

impl error::Error for Error {}

/// Result type used by coral
pub type Result<T> = result::Result<T, Error>;

/// Get the width of the terminal
pub fn terminal_width() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(100)
}

const LEVEL_COLUMN_WIDTH: usize = 7;
const FILE_COLUMN_WIDTH: usize = 18;
const LINE_COLUMN_WIDTH: usize = 8;
const ELIPSES_COLUMN_WIDTH: usize = 3;

fn message_column_width(terminal_width: usize) -> usize {
    terminal_width - LEVEL_COLUMN_WIDTH - FILE_COLUMN_WIDTH - LINE_COLUMN_WIDTH - 6
}

fn ensure_color() {
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();
}

/// A way of checking a project
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum Checker {
    /// Check with `cargo check`
    Check,
    /// Check with `cargo clippy`
    Clippy,
}

impl Default for Checker {
    fn default() -> Self {
        Checker::Check
    }
}

/// The main entrypoint for running cargo and parsing output
pub struct Analyzer {
    child: Child,
    buffer: VecDeque<u8>,
    debug: bool,
    color: bool,
}

impl Analyzer {
    /// Create a new `Analyzer` that uses `cargo check`
    pub fn new() -> Result<Analyzer> {
        Analyzer::with_args(Checker::Check, &[])
    }
    /// Create a new `Analyzer` that uses `cargo clippy`
    pub fn clippy() -> Result<Analyzer> {
        Analyzer::with_args(Checker::Clippy, &[])
    }
    /// Create a new `Analyzer` that uses the given checker and argments
    pub fn with_args(checker: Checker, args: &[&str]) -> Result<Analyzer> {
        ensure_color();
        Ok(Analyzer {
            child: Command::new("cargo")
                .args(&[
                    &format!("{:?}", checker).to_lowercase(),
                    "--message-format",
                    "json",
                ])
                .args(args)
                .stdin(Stdio::null())
                .stderr(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .map_err(|_| Error::Cargo)?,
            buffer: VecDeque::new(),
            debug: false,
            color: true,
        })
    }
    /// Set whether to enable debug mode. Default is `false`
    pub fn debug(self, debug: bool) -> Self {
        Analyzer { debug, ..self }
    }
    /// Set whether to enable console coloring. Default is `true`
    pub fn color(self, color: bool) -> Self {
        Analyzer { color, ..self }
    }
    fn add_to_buffer(&mut self) {
        const BUFFER_LEN: usize = 100;
        let mut buffer = [0u8; BUFFER_LEN];
        while !self.buffer.contains(&b'\n') {
            if let Ok(len) = self.child.stdout.as_mut().unwrap().read(&mut buffer) {
                if len == 0 {
                    break;
                } else {
                    self.buffer.extend(&buffer[..len]);
                }
            } else {
                break;
            }
        }
    }
}

impl Iterator for Analyzer {
    type Item = Entry;
    fn next(&mut self) -> Option<Self::Item> {
        colored::control::set_override(true);
        self.add_to_buffer();
        let mut entry_buffer = Vec::new();
        while let Some(byte) = self.buffer.pop_front().filter(|&b| b != b'\n') {
            entry_buffer.push(byte);
        }
        let res = if entry_buffer.is_empty() {
            None
        } else {
            if self.debug {
                println!("\t{}\n", String::from_utf8_lossy(&entry_buffer));
                let mut file = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("coral.json")
                    .unwrap();
                let _ = file.write(&entry_buffer).unwrap();
                writeln!(file).unwrap();
            }
            let mut entry: Entry = serde_json::from_slice(&entry_buffer).unwrap();
            entry.color = self.color;
            Some(entry)
        };
        if res.is_none() {
            self.child.wait().unwrap();
        }
        res
    }
}

impl Debug for Analyzer {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "Analyzer")
    }
}

fn default_color_setting() -> bool {
    true
}

/// A top-level entry output by cargo
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[allow(missing_docs)]
pub struct Entry {
    pub reason: Reason,
    pub package_id: String,
    pub target: Option<Target>,
    pub message: Option<Message>,
    pub profile: Option<Profile>,
    pub features: Option<Vec<String>>,
    pub filenames: Option<Vec<PathBuf>>,
    pub executable: Option<PathBuf>,
    pub fresh: Option<bool>,
    #[serde(default = "default_color_setting")]
    color: bool,
}

impl Entry {
    /// Check if the `Entry` is a compiler message
    pub fn is_message(&self) -> bool {
        self.reason == Reason::CompilerMessage
    }
    /// Check if the `Entry` is a compiler artifact
    pub fn is_artifact(&self) -> bool {
        self.reason == Reason::CompilerArtifact
    }
    /// Get an error, warning, or info report from the `Entry`
    pub fn report(&self) -> Option<String> {
        self.report_width(terminal_width())
    }
    /// Same as [`Entry::report`](struct.Entry.html#method.report) but uses a custom terminal width
    pub fn report_width(&self, terminal_width: usize) -> Option<String> {
        self.message
            .as_ref()
            .and_then(|m| m.report(self.color, terminal_width))
    }
    /// Get the `Entry`'s render if it had one
    pub fn rendered(&self) -> Option<&str> {
        self.message
            .as_ref()
            .and_then(|m| m.rendered.as_ref().map(String::as_str))
    }
}

/// A reason output by cargo
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum Reason {
    CompilerArtifact,
    CompilerMessage,
    BuildScriptExecuted,
}

/// Target information output by cargo
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[allow(missing_docs)]
pub struct Target {
    pub kind: Vec<TargetKind>,
    pub crate_types: Vec<CrateType>,
    pub name: String,
    pub src_path: PathBuf,
    pub edition: String,
}

/// The kind of a [`Target`](struct.Target.html) output by cargo
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum TargetKind {
    Lib,
    Bin,
    CustomBuild,
    ProcMacro,
}

/// A message output by cargo
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[allow(missing_docs)]
pub struct Message {
    pub message: String,
    pub code: Option<Code>,
    pub level: Level,
    pub spans: Option<Vec<Span>>,
    pub children: Option<Vec<Message>>,
    pub rendered: Option<String>,
}

impl Message {
    /// Get a string containing the column headers for reports
    pub fn report_headers(color: bool) -> String {
        ensure_color();
        colored::control::set_override(color);
        let level = "Level"
            .pad_to_width_with_alignment(LEVEL_COLUMN_WIDTH, Alignment::Right)
            .bright_white();
        let file = "File"
            .pad_to_width_with_alignment(FILE_COLUMN_WIDTH, Alignment::Right)
            .bright_white();
        let line = "Line"
            .pad_to_width_with_alignment(LINE_COLUMN_WIDTH, Alignment::Left)
            .bright_white();
        let message = "Message".bright_white();
        let res = format!("{} {}    {} {}", level, file, line, message);
        colored::control::unset_override();
        res
    }
    /// Get the message as a compact report
    pub fn report(&self, color: bool, terminal_width: usize) -> Option<String> {
        if let (Some(span), true) = (
            self.spans.as_ref().and_then(|v| v.last()),
            self.level.is_some(),
        ) {
            colored::control::set_override(color);
            let level = self.level.format();
            let file = span.file_name_string();
            let file = if file.len() <= FILE_COLUMN_WIDTH {
                file
            } else {
                format!("...{}", &file[(file.len() - FILE_COLUMN_WIDTH + 3)..])
            }
            .pad_to_width_with_alignment(FILE_COLUMN_WIDTH, Alignment::Right)
            .bright_cyan();
            let (line, column) = span.line();
            let line = format!("{}:{}", line, column)
                .pad_to_width_with_alignment(LINE_COLUMN_WIDTH, Alignment::Left)
                .bright_cyan();
            let message_column_width = message_column_width(terminal_width);
            let message = if self.message.len() <= message_column_width {
                self.message[..(message_column_width.min(self.message.len()))].to_string()
            } else {
                format!(
                    "{}...",
                    &self.message
                        [..((message_column_width - ELIPSES_COLUMN_WIDTH).min(self.message.len()))]
                )
            }
            .pad_to_width_with_alignment(message_column_width, Alignment::Left)
            .white();
            let res = Some(format!("{} {} at {} {}", level, file, line, message));
            colored::control::unset_override();
            res
        } else {
            None
        }
    }
}

/// A code output by cargo
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[allow(missing_docs)]
pub struct Code {
    pub code: String,
    pub explanation: Option<String>,
}

/// A message severity level output by cargo
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum Level {
    #[serde(rename = "")]
    None,
    Note,
    Help,
    Warning,
    Error,
}

impl Level {
    /// Check if the level is not `Level::None`
    pub fn is_some(self) -> bool {
        !self.is_none()
    }
    /// Check if the level is `Level::None`
    pub fn is_none(self) -> bool {
        self == Level::None
    }
    fn format(self) -> String {
        let pad = |s: &str| s.pad_to_width_with_alignment(LEVEL_COLUMN_WIDTH, Alignment::Right);
        match self {
            Level::None => String::new(),
            Level::Note => format!("{}", pad("note").bright_cyan()),
            Level::Help => format!("{}", pad("help").bright_green()),
            Level::Warning => format!("{}", pad("warning").bright_yellow()),
            Level::Error => format!("{}", pad("error").bright_red()),
        }
    }
}

/// A span output by cargo
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[allow(missing_docs)]
pub struct Span {
    pub file_name: PathBuf,
    pub byte_start: usize,
    pub byte_end: usize,
    pub line_start: usize,
    pub line_end: usize,
    pub column_start: usize,
    pub column_end: usize,
    pub is_primary: bool,
    pub text: Vec<Text>,
    pub label: Option<String>,
    pub suggested_replacement: Option<String>,
    pub suggestion_applicability: Option<String>,
    pub expansion: Option<Box<Expansion>>,
}

impl Span {
    /// Get the `Span`'s line and column
    pub fn line(&self) -> (usize, usize) {
        (self.line_start, self.column_start)
    }
    /// Get the `Span`'s file name as a `String`
    pub fn file_name_string(&self) -> String {
        self.file_name.to_string_lossy().into_owned()
    }
}

/// A piece of text output by cargo
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[allow(missing_docs)]
pub struct Text {
    pub text: String,
    pub highlight_start: usize,
    pub highlight_end: usize,
}

/// A macro expansion output by cargo
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[allow(missing_docs)]
pub struct Expansion {
    pub span: Span,
    pub macro_decl_name: String,
    pub def_site_span: Option<Span>,
}

/// A crate type output by cargo
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub enum CrateType {
    Lib,
    Bin,
    ProcMacro,
}

/// A profile output by cargo
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[allow(missing_docs)]
pub struct Profile {
    pub opt_level: String,
    pub debuginfo: u8,
    pub debug_assertions: bool,
    pub overflow_checks: bool,
    pub test: bool,
}

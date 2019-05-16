use std::{
    collections::VecDeque,
    error,
    fmt::{self, Debug, Display, Formatter},
    fs,
    io::{Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    result,
};

use colored::Colorize;
use pad::{Alignment, PadStr};
use serde_derive::{Deserialize, Serialize};

#[derive(Debug)]
pub enum Error {
    Cargo,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        use Error::*;
        match self {
            Cargo => write!(f, "Unable to run cargo"),
        }
    }
}

impl error::Error for Error {}

pub type Result<T> = result::Result<T, Error>;

fn terminal_width() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(100)
}

const LEVEL_COLUMN_WIDTH: usize = 7;
const FILE_COLUMN_WIDTH: usize = 15;
const LINE_COLUMN_WIDTH: usize = 8;

fn message_column_width() -> usize {
    terminal_width() - LEVEL_COLUMN_WIDTH - FILE_COLUMN_WIDTH - LINE_COLUMN_WIDTH
}

fn ensure_color() {
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();
}

pub struct Analyzer {
    child: Child,
    buffer: VecDeque<u8>,
    debug: bool,
}

impl Analyzer {
    pub fn new() -> Result<Analyzer> {
        ensure_color();
        Ok(Analyzer {
            child: Command::new("cargo")
                .args(&["check", "--message-format", "json"])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .stdout(Stdio::piped())
                .spawn()
                .map_err(|_| Error::Cargo)?,
            buffer: VecDeque::new(),
            debug: false,
        })
    }
    pub fn debug(self) -> Self {
        Analyzer {
            debug: true,
            ..self
        }
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
                    .open("check.json")
                    .unwrap();
                let _ = file.write(&entry_buffer).unwrap();
                writeln!(file).unwrap();
            }
            let entry: Entry = serde_json::from_slice(&entry_buffer).unwrap();
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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
}

impl Entry {
    /// Check if the `Entry` is a compiler message
    pub fn is_message(&self) -> bool {
        self.reason == Reason::CompilerMessage
    }
    pub fn report(&self) -> Option<String> {
        self.message.as_ref().and_then(Message::report)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    CompilerArtifact,
    CompilerMessage,
    BuildScriptExecuted,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Target {
    pub kind: Vec<TargetKind>,
    pub crate_types: Vec<CrateType>,
    pub name: String,
    pub src_path: PathBuf,
    pub edition: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TargetKind {
    Lib,
    Bin,
    CustomBuild,
    ProcMacro,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Message {
    pub message: String,
    pub code: Option<Code>,
    pub level: Level,
    pub spans: Option<Vec<Span>>,
    pub children: Option<Vec<Message>>,
    pub rendered: Option<String>,
}

impl Message {
    pub fn report_headers() -> String {
        ensure_color();
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
        format!("{} {}    {} {}", level, file, line, message)
    }
    pub fn report(&self) -> Option<String> {
        if let (Some(span), true) = (
            self.spans.as_ref().and_then(|v| v.first()),
            self.level.is_some(),
        ) {
            let level = self
                .level
                .to_string()
                .pad_to_width_with_alignment(LEVEL_COLUMN_WIDTH, Alignment::Right);
            let file = span
                .file_name_string()
                .pad_to_width_with_alignment(FILE_COLUMN_WIDTH, Alignment::Right)
                .bright_cyan();
            let (line, column) = span.line();
            let line = format!("{}:{}", line, column)
                .pad_to_width_with_alignment(LINE_COLUMN_WIDTH, Alignment::Left)
                .bright_cyan();
            let message = self.message[..(message_column_width().min(self.message.len()))].white();
            Some(format!("{} {} at {} {}", level, file, line, message))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Code {
    pub code: String,
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    #[serde(rename = "")]
    None,
    Note,
    Help,
    Warning,
    Error,
}

impl Level {
    pub fn is_some(self) -> bool {
        !self.is_none()
    }
    pub fn is_none(self) -> bool {
        self == Level::None
    }
}

impl Display for Level {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Level::None => write!(f, ""),
            Level::Note => write!(f, "{}", "note".bright_cyan()),
            Level::Help => write!(f, "{}", "help".bright_green()),
            Level::Warning => write!(f, "{}", "warning".bright_yellow()),
            Level::Error => write!(f, "{}", "error".bright_red()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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
    pub expansion: Option<String>,
}

impl Span {
    pub fn line(&self) -> (usize, usize) {
        (self.line_start, self.column_start)
    }
    pub fn file_name_string(&self) -> String {
        self.file_name.to_string_lossy().into_owned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Text {
    pub text: String,
    pub highlight_start: usize,
    pub highlight_end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CrateType {
    Lib,
    Bin,
    ProcMacro,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Profile {
    pub opt_level: String,
    pub debuginfo: u8,
    pub debug_assertions: bool,
    pub overflow_checks: bool,
    pub test: bool,
}

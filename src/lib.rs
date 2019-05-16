use std::{fs, path::PathBuf, process::Command};

use serde_derive::{Deserialize, Serialize};

pub fn parse() -> Vec<Entry> {
    let stdout = Command::new("cargo")
        .args(&["check", "--message-format", "json"])
        .output()
        .unwrap()
        .stdout;
    fs::write("check.json", &stdout).unwrap();
    stdout
        .split(|&b| b == b'\n')
        .filter(|slice| !slice.is_empty())
        .map(|obj| {
            #[cfg(debug_assertions)]
            println!("\r{:?}\n", String::from_utf8_lossy(obj));
            serde_json::from_slice(obj).unwrap()
        })
        .collect()
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

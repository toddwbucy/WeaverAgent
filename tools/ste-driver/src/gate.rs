//! Workflow 4.01, 4.05, 4.06 and 6.03. Call the existing operator gate client.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Keep the subprocess evidence even when it cannot supply an answered close.
#[derive(Debug)]
pub struct Capture {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Answer {
    pub run: String,
    pub turn: String,
    pub text: Option<String>,
    pub truncated: bool,
}

impl Capture {
    /// A valid completed turn can lack model text. That is a shape fault.
    /// A failed transport has no completed answer and cannot trigger a retry.
    pub fn answer(&self) -> Result<Answer, String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if self.exit_code != Some(0) {
            return Err(format!("gate client exit: {:?}", self.exit_code));
        }
        let value: Value = serde_json::from_slice(&self.stdout)
            .map_err(|e| format!("gate response is not one JSON value: {e}"))?;
        if value["kind"] != "answered" {
            return Err(format!("gate close is not answered: {}", value["kind"]));
        }
        let identity = |key| {
            value[key]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| format!("answered close lacks {key}"))
        };
        let truncated = match value.get("finish") {
            None => false,
            Some(Value::String(s)) if s == "length" => true,
            _ => return Err("answered close has an unknown finish".into()),
        };
        Ok(Answer {
            run: identity("run")?,
            turn: identity("turn")?,
            text: value["text"].as_str().map(str::to_owned),
            truncated,
        })
    }

    pub fn to_json(&self) -> Value {
        json!({"stdout_bytes": self.stdout, "stderr_bytes": self.stderr,
            "exit_code": self.exit_code, "error": self.error,
            "gate": serde_json::from_slice::<Value>(&self.stdout).ok()})
    }
}

pub trait Transport {
    fn send(&mut self, text: &str) -> Capture;
}

pub struct TurnClient {
    script: PathBuf,
    agent: String,
    config_root: PathBuf,
}

impl TurnClient {
    /// The caller supplies this checkout and the operator's admin config root.
    pub fn new(repo: &Path, agent: &str, config_root: &Path) -> Result<Self, String> {
        if agent.is_empty()
            || !agent
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        {
            return Err("invalid agent name".into());
        }
        let script = repo.join("deploy/turn.py");
        if !script.is_file() {
            return Err("the checkout has no deploy/turn.py".into());
        }
        Ok(Self {
            script,
            agent: agent.to_owned(),
            config_root: config_root.to_owned(),
        })
    }
}

impl Transport for TurnClient {
    fn send(&mut self, text: &str) -> Capture {
        // One argument carries the complete user block. No shell parses it.
        // No system flag is supplied. Prompt seeding is the operator's action.
        match Command::new("python3")
            .arg(&self.script)
            .arg(&self.agent)
            .arg(text)
            .arg("--raw")
            .env("WEAVER_ADMIN_CONFIG", &self.config_root)
            .output()
        {
            Ok(output) => Capture {
                stdout: output.stdout,
                stderr: output.stderr,
                exit_code: output.status.code(),
                error: None,
            },
            Err(error) => Capture {
                stdout: Vec::new(),
                stderr: Vec::new(),
                exit_code: None,
                error: Some(format!("gate client could not start: {error}")),
            },
        }
    }
}

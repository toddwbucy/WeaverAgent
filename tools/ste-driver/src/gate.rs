//! Workflow 4.01, 4.05, 4.06 and 6.03. Call the existing operator gate client.

use serde_json::{Value, json};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

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
    deadline: Duration,
}

impl TurnClient {
    /// The caller supplies this checkout, the operator's admin config root and the
    /// gate-call deadline. The deadline has no default, and zero is refused.
    pub fn new(
        repo: &Path,
        agent: &str,
        config_root: &Path,
        deadline: Duration,
    ) -> Result<Self, String> {
        if deadline.is_zero() {
            return Err("the gate-call deadline must be at least one second".into());
        }
        if Instant::now().checked_add(deadline).is_none() {
            return Err("the gate-call deadline is too large to measure".into());
        }
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
            deadline,
        })
    }
}

/// Read one pipe to its end on its own thread, so that neither pipe can fill and
/// stop the client while the caller waits on the other.
fn drain(pipe: Option<impl Read + Send + 'static>) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

impl Transport for TurnClient {
    /// A client still running at the deadline is killed and reaped. Its capture keeps
    /// the bytes it wrote and an error naming the deadline, so the caller records a
    /// transport stop. A turn the gate already admitted still runs in the agent.
    fn send(&mut self, text: &str) -> Capture {
        // One argument carries the complete user block. No shell parses it.
        // No system flag is supplied. Prompt seeding is the operator's action.
        let started = Command::new("python3")
            .arg(&self.script)
            .arg(&self.agent)
            .arg(text)
            .arg("--raw")
            .env("WEAVER_ADMIN_CONFIG", &self.config_root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match started {
            Ok(child) => child,
            Err(error) => {
                return Capture {
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    exit_code: None,
                    error: Some(format!("gate client could not start: {error}")),
                };
            }
        };
        let stdout = drain(child.stdout.take());
        let stderr = drain(child.stderr.take());
        // An end past what an instant can hold never comes.
        let end = Instant::now().checked_add(self.deadline);
        let ended = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status.code()),
                Ok(None) if end.is_some_and(|end| Instant::now() >= end) => {
                    let _ = child.kill();
                    // A client that exited by itself before the kill keeps its own status.
                    if let Ok(status) = child.wait()
                        && status.code().is_some()
                    {
                        break Ok(status.code());
                    }
                    break Err(format!(
                        "gate client passed the gate-call deadline of {} s and was killed",
                        self.deadline.as_secs_f64()
                    ));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(format!("gate client could not be waited on: {error}"));
                }
            }
        };
        // turn.py starts no process of its own, so both pipes close when it ends.
        let stdout = stdout.join().unwrap_or_default();
        let stderr = stderr.join().unwrap_or_default();
        match ended {
            Ok(exit_code) => Capture {
                stdout,
                stderr,
                exit_code,
                error: None,
            },
            Err(error) => Capture {
                stdout,
                stderr,
                exit_code: None,
                error: Some(error),
            },
        }
    }
}

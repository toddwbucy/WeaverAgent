//! Workflow 4.01, 4.05, 4.06, 6.03 and 7.08. Call the existing operator gate client
//! within the operator's gate-call deadline.

use serde_json::{Value, json};
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

/// How long both pipes may stay open after the client ends. Only a process the
/// client started can hold them, and its output is not waited for past this.
const PIPE_CLOSE_GRACE: Duration = Duration::from_secs(1);

/// The elected gate-call deadline in whole seconds. There is no default. A deadline
/// under one second, or one an instant cannot hold, is refused.
pub fn gate_call_deadline(seconds: u64) -> Result<Duration, String> {
    let deadline = Duration::from_secs(seconds);
    check_deadline(deadline)?;
    Ok(deadline)
}

fn check_deadline(deadline: Duration) -> Result<(), String> {
    if deadline < Duration::from_secs(1) {
        return Err("the gate-call deadline must be at least one second".into());
    }
    if Instant::now().checked_add(deadline).is_none() {
        return Err("the gate-call deadline is too large to measure".into());
    }
    Ok(())
}

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
    /// gate-call deadline, which [`gate_call_deadline`]'s rules bound.
    pub fn new(
        repo: &Path,
        agent: &str,
        config_root: &Path,
        deadline: Duration,
    ) -> Result<Self, String> {
        check_deadline(deadline)?;
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

/// One pipe read on its own thread, so that neither pipe can fill and stop the
/// client while the caller waits on the other. The bytes read so far stay readable.
struct Drain {
    bytes: Arc<Mutex<Vec<u8>>>,
    closed: mpsc::Receiver<()>,
}

impl Drain {
    fn start(pipe: Option<impl Read + Send + 'static>) -> Self {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let (tell, closed) = mpsc::channel();
        let sink = Arc::clone(&bytes);
        std::thread::spawn(move || {
            if let Some(mut pipe) = pipe {
                let mut chunk = [0u8; 8192];
                loop {
                    match pipe.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => sink
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .extend_from_slice(&chunk[..n]),
                        Err(error) if error.kind() == ErrorKind::Interrupted => {}
                        Err(_) => break,
                    }
                }
            }
            let _ = tell.send(());
        });
        Self { bytes, closed }
    }

    /// The bytes read, and whether the pipe closed by `until`.
    fn finish(self, until: Instant) -> (Vec<u8>, bool) {
        let closed = self
            .closed
            .recv_timeout(until.saturating_duration_since(Instant::now()))
            .is_ok();
        let bytes = std::mem::take(&mut *self.bytes.lock().unwrap_or_else(|e| e.into_inner()));
        (bytes, closed)
    }
}

impl Transport for TurnClient {
    /// A client still running at the deadline is killed and reaped. Its capture keeps
    /// the bytes it wrote and an error naming the deadline, so the caller records a
    /// transport stop. A turn the gate already admitted still runs in the agent. The
    /// call returns within [`PIPE_CLOSE_GRACE`] of the client's end, even when a
    /// process the client started still holds its pipes.
    fn send(&mut self, text: &str) -> Capture {
        let stopped = |stdout, stderr, error| Capture {
            stdout,
            stderr,
            exit_code: None,
            error: Some(error),
        };
        let Some(end) = Instant::now().checked_add(self.deadline) else {
            return stopped(
                Vec::new(),
                Vec::new(),
                "the gate-call deadline cannot be measured from now".into(),
            );
        };
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
                return stopped(
                    Vec::new(),
                    Vec::new(),
                    format!("gate client could not start: {error}"),
                );
            }
        };
        let stdout = Drain::start(child.stdout.take());
        let stderr = Drain::start(child.stderr.take());
        let ended = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status.code()),
                Ok(None) if Instant::now() >= end => {
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
        // turn.py starts no process, so both pipes close when it ends. A process a
        // client started can hold them open, and its output is not waited for.
        let grace = Instant::now() + PIPE_CLOSE_GRACE;
        let (stdout, out_closed) = stdout.finish(grace);
        let (stderr, err_closed) = stderr.finish(grace);
        match ended {
            Ok(_) if !(out_closed && err_closed) => stopped(
                stdout,
                stderr,
                "gate client ended, and a process it started still holds its output".into(),
            ),
            Ok(exit_code) => Capture {
                stdout,
                stderr,
                exit_code,
                error: None,
            },
            Err(error) => stopped(stdout, stderr, error),
        }
    }
}

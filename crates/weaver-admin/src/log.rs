//! conforms: admin-log-ndjson-own-schema
//! conforms: admin-log-never-records-agent-conduct
//! conforms: admin-rollback-logs-its-account
//!
//! The operations log, per `weaver-admin-Spec` section 8: NDJSON, one act per
//! line, sharing no schema with the trace.
//!
//! **What is logged is supervision and never conduct.** Transitions directed
//! and their outcomes, refusals issued, rollbacks with what each act undid or
//! could not, and units started and stopped. Never a fact about what an agent
//! did: the moment a line describes conduct rather than supervision it is a
//! second record of the agent, and conduct is recorded in the trace the
//! harness authors. The instrument is review, no mechanism being able to tell
//! a line about supervision from a line about conduct.

use std::io::Write;
use std::os::fd::{FromRawFd, OwnedFd};
use std::path::Path;

/// One supervisory act, per `weaver-admin-Spec` section 8: the command line,
/// the caller's uid, what was asked, the boundary file's digest in force, and
/// the outcome, the wall time added at the write. The field set is this
/// crate's own and deliberately not the event envelope, because a shared
/// schema is how a second author drifts into the first's record.
#[derive(Debug, Clone)]
pub struct Act {
    pub verb: &'static str,
    pub agent: String,
    /// The command line as the rule grants it, `weaver-admin <verb> <agent>`.
    pub command: String,
    /// The uid sudo reported, or 0 at a root shell.
    pub uid: u32,
    /// The sha256 hex of `roles.toml` in force, absent where none was read.
    pub boundary: Option<String>,
    pub outcome: String,
    /// For a load's line: the agent's SPU path, per sections 8 and 9, which
    /// SPU admin handed the worker being supervision.
    pub spu: Option<String>,
}

/// The log's writer: this crate's own file with this crate's own writer, a
/// logging framework being a second account with its own schema.
#[derive(Debug)]
pub struct OperationsLog {
    file: std::fs::File,
}

impl OperationsLog {
    /// Opens the agent's `admin.log` for appending, created `0640` where
    /// absent, **never through a link at its name**, per section 8, and
    /// close-on-exec at creation like every descriptor this crate holds.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(std::io::Error::other)?;
        let flags = nix::libc::O_WRONLY
            | nix::libc::O_APPEND
            | nix::libc::O_CREAT
            | nix::libc::O_NOFOLLOW
            | nix::libc::O_CLOEXEC;
        // SAFETY: open with a NUL-terminated path and a mode for O_CREAT.
        let fd = unsafe { nix::libc::open(c_path.as_ptr(), flags, 0o640 as nix::libc::c_uint) };
        if fd == -1 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: the number was just opened and is owned by nothing else.
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };
        Ok(OperationsLog {
            file: std::fs::File::from(owned),
        })
    }

    /// Appends one act as one JSON line, **written in one `write`**, so a
    /// line never interleaves with another writer's under `O_APPEND`.
    pub fn record(&mut self, act: &Act) -> std::io::Result<()> {
        let mut line = serde_json::Map::new();
        line.insert("wall_ms".into(), wall_ms().into());
        line.insert("command".into(), act.command.clone().into());
        line.insert("uid".into(), act.uid.into());
        line.insert("verb".into(), act.verb.into());
        line.insert("agent".into(), act.agent.clone().into());
        if let Some(boundary) = &act.boundary {
            line.insert("boundary".into(), boundary.clone().into());
        }
        line.insert("outcome".into(), act.outcome.clone().into());
        if let Some(spu) = &act.spu {
            line.insert("spu".into(), spu.clone().into());
        }
        let mut rendered = serde_json::to_string(&serde_json::Value::Object(line))
            .map_err(std::io::Error::other)?;
        rendered.push('\n');
        self.file.write_all(rendered.as_bytes())?;
        self.file.flush()
    }
}

/// Milliseconds since the epoch, the line's wall time.
fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act(verb: &'static str, outcome: &str, spu: Option<&str>) -> Act {
        Act {
            verb,
            agent: "karl".into(),
            command: format!("weaver-admin {verb} karl"),
            uid: 1000,
            boundary: Some("b0b0".into()),
            outcome: outcome.into(),
            spu: spu.map(str::to_string),
        }
    }

    /// **Each line carries Spec section 8's fields, and a load's line names
    /// the agent's SPU**: the wall time, the command line, the caller's uid,
    /// the verb, the boundary digest in force and the outcome. Perturbation:
    /// drop the uid or the boundary from the rendering and the matching
    /// assertion fails.
    #[test]
    fn a_line_carries_who_asked_what_and_under_which_boundary() {
        let path =
            std::env::temp_dir().join(format!("weaver-admin-log-{}.ndjson", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut log = OperationsLog::open(&path).unwrap();
        log.record(&act("load", "ready", Some("/opt/weaver/bin/weaver-spu")))
            .unwrap();
        log.record(&act("show", "unloaded", None)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines[0]["spu"], "/opt/weaver/bin/weaver-spu");
        assert_eq!(lines[0]["command"], "weaver-admin load karl");
        assert_eq!(lines[0]["uid"], 1000);
        assert_eq!(lines[0]["boundary"], "b0b0");
        assert!(lines[0]["wall_ms"].as_u64().unwrap() > 0);
        assert!(lines[1].get("spu").is_none(), "{:?}", lines[1]);
        let _ = std::fs::remove_file(&path);
    }

    /// **A link planted at the log's name is refused, never followed**, per
    /// Spec section 8. Perturbation: drop `O_NOFOLLOW` and the open writes
    /// through the link.
    #[test]
    fn a_link_at_the_logs_name_is_refused() {
        let dir =
            std::env::temp_dir().join(format!("weaver-admin-log-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("elsewhere");
        std::fs::write(&target, "").unwrap();
        std::os::unix::fs::symlink(&target, dir.join("admin.log")).unwrap();
        assert!(OperationsLog::open(&dir.join("admin.log")).is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

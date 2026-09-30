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

/// One supervisory act. The field set is this crate's own and deliberately not
/// the event envelope, because a shared schema is how a second author drifts
/// into the first's record.
#[derive(Debug, Clone)]
pub struct Act {
    pub verb: &'static str,
    pub agent: String,
    pub outcome: String,
    /// For a rollback line: what this act undid, or could not.
    pub undone: Option<String>,
    /// For a load's line: the agent's SPU key and path, per `weaver-admin-Spec`
    /// sections 8 and 9, which SPU admin handed the worker being supervision.
    pub spu: Option<String>,
}

/// The log's writer: this crate's own file with this crate's own writer, a
/// logging framework being a second account with its own schema.
#[derive(Debug)]
pub struct OperationsLog {
    file: std::fs::File,
}

impl OperationsLog {
    /// Opens the log with `O_APPEND` and `O_CLOEXEC` like every descriptor
    /// this crate holds. The directory is the operator's to place.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(std::io::Error::other)?;
        let flags =
            nix::libc::O_WRONLY | nix::libc::O_APPEND | nix::libc::O_CREAT | nix::libc::O_CLOEXEC;
        // SAFETY: open with a NUL-terminated path this frame owns; the
        // descriptor is adopted immediately.
        let fd = unsafe { nix::libc::open(c_path.as_ptr(), flags, 0o640 as nix::libc::c_uint) };
        if fd == -1 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: the kernel just created this descriptor.
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };
        Ok(OperationsLog {
            file: std::fs::File::from(owned),
        })
    }

    /// Records one act as one line.
    pub fn record(&mut self, act: &Act) -> std::io::Result<()> {
        let mut line = serde_json::Map::new();
        line.insert("verb".into(), act.verb.into());
        line.insert("agent".into(), act.agent.clone().into());
        line.insert("outcome".into(), act.outcome.clone().into());
        if let Some(undone) = &act.undone {
            line.insert("undone".into(), undone.clone().into());
        }
        if let Some(spu) = &act.spu {
            line.insert("spu".into(), spu.clone().into());
        }
        let rendered = serde_json::to_string(&serde_json::Value::Object(line))
            .map_err(std::io::Error::other)?;
        self.file.write_all(rendered.as_bytes())?;
        self.file.write_all(b"\n")?;
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A load's line names the agent's SPU and no other line carries the
    /// member**, per `weaver-admin-Spec` sections 8 and 9. Perturbation: drop
    /// the member from the rendering and the first assertion fails.
    #[test]
    fn a_loads_line_names_its_spu() {
        let path =
            std::env::temp_dir().join(format!("weaver-admin-log-{}.ndjson", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut log = OperationsLog::open(&path).unwrap();
        log.record(&Act {
            verb: "load",
            agent: "karl".into(),
            outcome: "ready".into(),
            undone: None,
            spu: Some("python /opt/weaver/python-spu/python-spu.pyz".into()),
        })
        .unwrap();
        log.record(&Act {
            verb: "stop",
            agent: "karl".into(),
            outcome: "stopped".into(),
            undone: None,
            spu: None,
        })
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(
            lines[0]["spu"],
            "python /opt/weaver/python-spu/python-spu.pyz"
        );
        assert!(lines[1].get("spu").is_none(), "{:?}", lines[1]);
        let _ = std::fs::remove_file(&path);
    }
}

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

/// **Opens a log this crate appends to as root**, `admin.log`, `worker.log`
/// or the member's `state.log`, per Spec sections 6 and 8: created `0640`
/// where absent, **never through a link at its name**, opened non-blocking so
/// a FIFO planted at the name cannot hold the verb, and refused unless it is a
/// regular file, close-on-exec at creation like every descriptor this crate
/// holds. Where `owner` is given the file is set to that uid and gid and
/// `0640` through the open descriptor, the operator's logs being the
/// operator's to read.
pub fn open_append(path: &Path, owner: Option<(u32, u32)>) -> std::io::Result<std::fs::File> {
    use std::os::fd::AsRawFd;
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(std::io::Error::other)?;
    let flags = nix::libc::O_WRONLY
        | nix::libc::O_APPEND
        | nix::libc::O_CREAT
        | nix::libc::O_NOFOLLOW
        | nix::libc::O_NONBLOCK
        | nix::libc::O_CLOEXEC;
    // SAFETY: open with a NUL-terminated path and a mode for O_CREAT.
    let fd = unsafe { nix::libc::open(c_path.as_ptr(), flags, 0o640 as nix::libc::c_uint) };
    if fd == -1 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: the number was just opened and is owned by nothing else.
    let file = std::fs::File::from(unsafe { OwnedFd::from_raw_fd(fd) });
    if !file.metadata()?.file_type().is_file() {
        return Err(std::io::Error::other("a log is a regular file"));
    }
    // **And carries no access entry beyond its mode** (#99 area 2, H5;
    // #107): an entry granting the member read survives the 0640 set below,
    // hidden under the mask, so such a log is refused rather than written.
    match crate::carries_access_entries_fd(std::os::fd::AsFd::as_fd(&file)) {
        Ok(false) => {}
        Ok(true) => {
            return Err(std::io::Error::other(
                "a log carries an access-control entry beyond its mode",
            ));
        }
        Err(e) => return Err(std::io::Error::from(e)),
    }
    let raw = file.as_raw_fd();
    if let Some((uid, gid)) = owner {
        // SAFETY: fchown and fchmod on the descriptor just opened.
        let set =
            unsafe { nix::libc::fchown(raw, uid, gid) == 0 && nix::libc::fchmod(raw, 0o640) == 0 };
        if !set {
            return Err(std::io::Error::last_os_error());
        }
    }
    // SAFETY: fcntl on the descriptor just opened, clearing O_NONBLOCK now
    // that it is known to be a regular file.
    unsafe {
        let status = nix::libc::fcntl(raw, nix::libc::F_GETFL);
        if status == -1
            || nix::libc::fcntl(raw, nix::libc::F_SETFL, status & !nix::libc::O_NONBLOCK) == -1
        {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(file)
}

impl OperationsLog {
    /// Opens the agent's `admin.log` for appending, owned by `owner`, per
    /// section 8, through `open_append`.
    pub fn open(path: &Path, owner: Option<(u32, u32)>) -> std::io::Result<Self> {
        Ok(OperationsLog {
            file: open_append(path, owner)?,
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
        let mut log = OperationsLog::open(&path, None).unwrap();
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
        assert!(OperationsLog::open(&dir.join("admin.log"), None).is_err());
        // A FIFO planted at the name neither holds the open nor takes a line.
        std::fs::remove_file(dir.join("admin.log")).unwrap();
        nix::unistd::mkfifo(&dir.join("admin.log"), nix::sys::stat::Mode::S_IRWXU).unwrap();
        assert!(OperationsLog::open(&dir.join("admin.log"), None).is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The operator's logs are the operator's to read**, per Spec section 8:
    /// a log opened with an owner is set to that uid and gid and `0640`
    /// through the descriptor, whatever mode it stood at. Run as the test's own
    /// uid, the one chown an unprivileged suite may make. Perturbation: drop
    /// the `fchmod` and the world-readable file stays `0666`.
    #[test]
    fn a_log_is_set_to_its_owner_and_0640() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let path =
            std::env::temp_dir().join(format!("weaver-admin-log-owner-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, "").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
        let (uid, gid) = (
            nix::unistd::getuid().as_raw(),
            nix::unistd::getgid().as_raw(),
        );
        drop(OperationsLog::open(&path, Some((uid, gid))).unwrap());
        let meta = std::fs::metadata(&path).unwrap();
        assert_eq!(meta.mode() & 0o7777, 0o640);
        assert_eq!((meta.uid(), meta.gid()), (uid, gid));
        let _ = std::fs::remove_file(&path);
    }
}

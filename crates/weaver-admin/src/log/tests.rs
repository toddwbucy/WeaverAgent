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
    let path = std::env::temp_dir().join(format!("weaver-admin-log-{}.ndjson", std::process::id()));
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
    let dir = std::env::temp_dir().join(format!("weaver-admin-log-link-{}", std::process::id()));
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
    let path = std::env::temp_dir().join(format!("weaver-admin-log-owner-{}", std::process::id()));
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

/// **A log name hard-linked to another file is refused** (#98; the custody
/// audit's G4): `O_NOFOLLOW` refuses a symbolic link and not a hard one, so
/// the open judges the link count on its descriptor. A file with one link
/// opens; the same file with a second link refuses, and nothing is written
/// to it. Perturbation: drop the link count from `log_standing`, and the
/// linked file opens.
#[test]
fn a_hard_linked_log_is_refused() {
    let dir = std::env::temp_dir().join(format!(
        "weaver-admin-log-link-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("trace.ndjson");
    std::fs::write(&target, "kept\n").unwrap();
    let log = dir.join("state.log");
    std::fs::write(&log, "").unwrap();
    assert!(open_append(&log, None).is_ok(), "one link opens");
    std::fs::remove_file(&log).unwrap();
    std::fs::hard_link(&target, &log).unwrap();
    assert!(open_append(&log, None).is_err(), "a second link refuses");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "kept\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// **A log owned by another principal is refused** (#98): one this open
/// made is the creator's, and one standing is the creator's or the owner's
/// asked for. Judged as a function, since the suite's uid cannot make a file
/// another uid owns. Perturbation: drop the owner comparison, and the
/// stranger's file stands.
#[test]
fn a_log_owned_by_a_stranger_is_refused() {
    assert_eq!(log_standing(1, 0, 0, None), Ok(()));
    assert_eq!(log_standing(1, 0, 0, Some(0)), Ok(()));
    assert_eq!(log_standing(1, 1000, 0, Some(1000)), Ok(()));
    assert!(log_standing(1, 1234, 0, None).is_err(), "a stranger's file");
    assert!(
        log_standing(1, 1234, 0, Some(0)).is_err(),
        "not the owner asked for"
    );
    assert!(log_standing(2, 0, 0, None).is_err(), "two links");
}

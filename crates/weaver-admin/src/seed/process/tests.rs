use super::*;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn yard(tag: &str) -> crate::scratch::Scratch {
    let path = std::env::temp_dir().join(format!(
        "weaver-seed-process-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    crate::scratch::Scratch(path)
}

struct Watched(Child);

impl std::ops::Deref for Watched {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}

impl std::ops::DerefMut for Watched {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}

impl Drop for Watched {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn shell(script: &str) -> Watched {
    Watched(
        Command::new("/bin/sh")
            .args(["-c", script])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    )
}

/// Removing the success-status check makes the nonzero case publishable;
/// removing the origin deadline check makes an already expired finish succeed.
#[test]
fn only_a_successful_exit_before_the_deadline_finishes() {
    let mut good = shell("exit 0");
    finish_member(&mut good, Instant::now() + Duration::from_secs(5)).unwrap();
    assert!(good.try_wait().unwrap().unwrap().success());

    let mut bad = shell("exit 23");
    let error = finish_member(&mut bad, Instant::now() + Duration::from_secs(5)).unwrap_err();
    assert!(error.detail.contains("failed at close"), "{error}");
    assert_eq!(bad.try_wait().unwrap().unwrap().code(), Some(23));

    let mut late = shell("exit 0");
    late.wait().unwrap();
    let error = finish_member(&mut late, Instant::now()).unwrap_err();
    assert!(error.detail.contains("deadline expired"), "{error}");
}

/// The deadline cannot leave a still-running child behind or turn escalation
/// into a successful seed finish. The shell is the child, with no grandchildren.
#[test]
fn a_finish_timeout_is_reaped_and_stays_a_failure() {
    let mut child = shell("while :; do :; done");
    let error = finish_member(&mut child, Instant::now() + Duration::from_millis(30)).unwrap_err();
    assert!(error.detail.contains("finish bound"), "{error}");
    assert!(
        child.try_wait().unwrap().is_some(),
        "the actual child was reaped"
    );
}

/// The child announces its ignored TERM before the test escalates, so the
/// KILL arm is exercised without a race against the shell's trap installation.
#[test]
fn rollback_kills_and_reaps_a_child_that_ignores_term() {
    use std::io::BufRead;
    let child = Command::new("/bin/sh")
        .args(["-c", "trap '' TERM; echo ready; while :; do :; done"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut child = Watched(child);
    let mut line = String::new();
    std::io::BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert_eq!(line, "ready\n");
    let status = terminate_and_reap(
        &mut child,
        Duration::from_millis(30),
        Duration::from_secs(5),
    )
    .unwrap();
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(status.signal(), Some(nix::libc::SIGKILL));
    assert!(child.try_wait().unwrap().is_some());
}

/// A lenient serving-end receipt is insufficient for a newly made seed
/// operation. Removing any strict receipt check is watched here.
#[test]
fn only_full_cgroup_and_record_removal_counts_as_cleanup() {
    use agent_cgroup::Ended;
    strict_end(Ok(Ended::Removed {
        standing: vec![42],
        record_removed: true,
    }))
    .unwrap();
    for result in [
        Ok(Ended::NoRecord),
        Ok(Ended::NotOwned),
        Ok(Ended::Stale {
            record_removed: true,
        }),
        Ok(Ended::Removed {
            standing: vec![],
            record_removed: false,
        }),
        Err(agent_cgroup::EndRefused::WouldNotExit(vec![42])),
        Err(agent_cgroup::EndRefused::Unverified("record".into())),
    ] {
        assert!(strict_end(result).is_err());
    }
}

/// The operation cannot borrow another operation's replacement record, even
/// when that record names an absent cgroup. Without the equality guard run_end
/// would remove this replacement record as stale.
#[test]
fn a_replaced_cgroup_record_is_left_untouched_and_refused() {
    let (_yard, directory) = containment::tests::run_directory("seed-replaced");
    let expected = containment::Record {
        path: "/nonexistent-seed-test/weaver-alpha".into(),
        dev: 1,
        ino: 2,
        run: "seed-one".into(),
    };
    let mut replacement = expected.clone();
    replacement.run = "seed-two".into();
    containment::write_record(directory.as_fd(), &replacement).unwrap();
    let read = directory.try_clone().unwrap();
    let error = OperationGroup {
        directory,
        agent: "alpha".into(),
        record: expected,
    }
    .end()
    .unwrap_err();
    assert!(error.detail.contains("record changed"));
    assert_eq!(
        containment::read_record(read.as_fd(), "alpha").unwrap(),
        Some(replacement)
    );
}

#[test]
fn all_cleanup_failures_remain_in_the_report() {
    let error = ProcessFailure::new("copy failed")
        .with_cleanup(Err(ProcessFailure::new("reap failed")))
        .with_cleanup(Err(ProcessFailure::new("cgroup failed")));
    let text = error.to_string();
    for detail in ["copy failed", "reap failed", "cgroup failed"] {
        assert!(text.contains(detail));
    }
}

/// No valid cgroup receipt is fabricated here: the fake record also refuses
/// cleanup. The primary result must still name the independently observed
/// premature exit, which removing the before-close status read loses.
#[test]
fn a_member_already_exited_zero_is_refused_before_channel_close() {
    let (_yard, directory) = containment::tests::run_directory("seed-early-exit");
    let run_yard = yard("early-exit-lock");
    let run_lock = start::take_run_lock(&run_yard.0).unwrap().unwrap();
    let mut child = Command::new("/bin/sh")
        .args(["-c", "exit 0"])
        .spawn()
        .unwrap();
    assert!(child.wait().unwrap().success());
    let (channel, _peer) = UnixStream::pair().unwrap();
    let operation = SeedChild {
        child,
        channel: channel.into(),
        room: directory.try_clone().unwrap(),
        group: OperationGroup {
            directory,
            agent: "alpha".into(),
            record: containment::Record {
                path: "/nonexistent-seed-test/weaver-alpha".into(),
                dev: 1,
                ino: 2,
                run: "unrecorded-test".into(),
            },
        },
        _run_lock: &run_lock,
    };
    let error = operation
        .finish(Instant::now() + Duration::from_secs(5))
        .unwrap_err();
    assert!(
        error.detail.contains("exited before channel close"),
        "{error}"
    );
    assert!(error.cleanup.unwrap().contains("record changed"));
}

/// This is the actual spawn helper, under uid 0 in an isolated test process.
/// Placement goes to /dev/null here; the separate delegated instrument below
/// watches real cgroup placement and end. No publication claim comes from it.
#[test]
#[ignore = "run by seed_spawn_custody_is_watched_inside_a_user_namespace"]
fn seed_spawn_lands_only_its_two_gifts_and_restricted_identity() {
    assert!(nix::unistd::geteuid().is_root());
    let yard = yard("spawn");
    std::fs::set_permissions(&yard.0, std::fs::Permissions::from_mode(0o755)).unwrap();
    let room = yard.0.join("state");
    std::fs::create_dir(&room).unwrap();
    let member = inventory::MemberAccount {
        uid: 4242,
        gid: 4243,
    };
    nix::unistd::chown(
        &room,
        Some(nix::unistd::Uid::from_raw(member.uid)),
        Some(nix::unistd::Gid::from_raw(member.gid)),
    )
    .unwrap();
    std::fs::set_permissions(&room, std::fs::Permissions::from_mode(0o700)).unwrap();
    let run_lock = start::take_run_lock(&yard.0).unwrap().unwrap();
    let placement = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/null")
        .unwrap();
    let stray_source = std::fs::File::open("/dev/null").unwrap();
    // SAFETY: a deliberately inheritable fixture descriptor, never a gift.
    let stray = unsafe { nix::libc::fcntl(stray_source.as_raw_fd(), nix::libc::F_DUPFD, 150) };
    assert!(stray >= 150);
    use std::os::fd::FromRawFd;
    // SAFETY: the successful duplicate is owned once by this scope.
    let _stray = unsafe { OwnedFd::from_raw_fd(stray) };
    let binary = yard.0.join("weaver-state");
    std::fs::write(&binary, format!(
        "#!/bin/sh\n[ \"$#\" -eq 2 ] || exit 70\n[ \"$1\" = '--mode=seed' ] || exit 71\n[ ! -e /proc/$$/fd/4 ] || exit 72\n[ ! -e /proc/$$/fd/{stray} ] || exit 73\n[ \"${{SEED_TEST_CALLER_ENV+x}}\" != x ] || exit 74\n/bin/cat /proc/$$/status > \"$2/status\"\n/usr/bin/readlink /proc/$$/fd/3 > \"$2/fd3\"\n/usr/bin/readlink /proc/$$/fd/9 > \"$2/fd9\"\npwd > \"$2/cwd\"\numask > \"$2/umask\"\nprintf 'READY\\n' >&3\n/bin/cat <&3 > /dev/null\nexit 0\n"
    )).unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    // SAFETY: this ignored test is launched alone in its own process. The
    // invocation's ignored signal and blocked mask are intentionally inherited.
    unsafe {
        nix::libc::signal(nix::libc::SIGUSR1, nix::libc::SIG_IGN);
        let mut mask: nix::libc::sigset_t = std::mem::zeroed();
        nix::libc::sigemptyset(&mut mask);
        nix::libc::sigaddset(&mut mask, nix::libc::SIGUSR1);
        assert_eq!(
            nix::libc::sigprocmask(nix::libc::SIG_BLOCK, &mask, std::ptr::null_mut()),
            0
        );
    }
    let (child, channel) =
        spawn_member(&binary, &room, member, &run_lock, placement.as_fd()).unwrap();
    let mut child = Watched(child);
    let mut stream = UnixStream::from(channel);
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut ready = [0; 6];
    stream.read_exact(&mut ready).unwrap();
    assert_eq!(&ready, b"READY\n");
    assert!(
        child.try_wait().unwrap().is_none(),
        "the member holds its image until EOF"
    );
    let status = std::fs::read_to_string(room.join("status")).unwrap();
    let field = |name: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .map(|value| value.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap()
    };
    assert_eq!(field("Uid:"), "4242 4242 4242 4242");
    assert_eq!(field("Gid:"), "4243 4243 4243 4243");
    assert_eq!(field("Groups:"), "4243");
    assert_eq!(field("NoNewPrivs:"), "1");
    // The shell can block SIGCHLD while waiting for the status-reading cat;
    // the inherited blocked SIGUSR1 is what the spawn must have cleared.
    let blocked = u64::from_str_radix(&field("SigBlk:"), 16).unwrap();
    assert_eq!(blocked & (1 << (nix::libc::SIGUSR1 - 1)), 0);
    let ignored = u64::from_str_radix(&field("SigIgn:"), 16).unwrap();
    assert_eq!(ignored & (1 << (nix::libc::SIGUSR1 - 1)), 0);
    // SAFETY: getsid observes the live child without changing it.
    assert_eq!(
        unsafe { nix::libc::getsid(child.id() as i32) },
        child.id() as i32
    );
    assert!(
        std::fs::read_to_string(room.join("fd3"))
            .unwrap()
            .starts_with("socket:[")
    );
    assert_eq!(
        std::fs::read_to_string(room.join("fd9")).unwrap().trim(),
        yard.0.join("run.lock").to_str().unwrap()
    );
    assert_eq!(std::fs::read_to_string(room.join("cwd")).unwrap(), "/\n");
    assert_eq!(
        std::fs::read_to_string(room.join("umask")).unwrap().trim(),
        "0027"
    );
    drop(stream);
    finish_member(&mut child, Instant::now() + Duration::from_secs(5)).unwrap();
    assert!(
        start::run_lock_held(&yard.0).unwrap(),
        "the parent keeps custody through cleanup/publication"
    );
    drop(run_lock);
    assert!(!start::run_lock_held(&yard.0).unwrap());

    // A member that exits before speaking must give EOF, even before admin
    // releases its endpoint. Any parent copy of member_end makes this EAGAIN.
    std::fs::write(&binary, "#!/bin/sh\nexit 0\n").unwrap();
    let lock = start::take_run_lock(&yard.0).unwrap().unwrap();
    let (child, channel) = spawn_member(&binary, &room, member, &lock, placement.as_fd()).unwrap();
    let mut child = Watched(child);
    child.wait().unwrap();
    let mut byte = [0u8];
    // SAFETY: bounded nonblocking read into one live byte on the held socket.
    assert_eq!(
        unsafe {
            nix::libc::recv(
                channel.as_raw_fd(),
                byte.as_mut_ptr().cast(),
                1,
                nix::libc::MSG_DONTWAIT,
            )
        },
        0
    );
}

#[test]
fn seed_spawn_custody_is_watched_inside_a_user_namespace() {
    let mut command = if nix::unistd::geteuid().is_root() {
        Command::new(std::env::current_exe().unwrap())
    } else {
        let mut command = Command::new("unshare");
        command
            .args(["--map-auto", "--map-root-user"])
            .arg(std::env::current_exe().unwrap());
        command
    };
    let output = command
        .args([
            "--exact",
            "seed::process::tests::seed_spawn_lands_only_its_two_gifts_and_restricted_identity",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("SEED_TEST_CALLER_ENV", "must-not-cross")
        .stdin(Stdio::null())
        .output();
    let output = match output {
        Ok(output) => output,
        Err(error) => {
            diag!("SKIP seed spawn custody watch: {error}");
            return;
        }
    };
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stderr.starts_with("unshare:") {
        diag!("SKIP seed spawn custody watch: {}", stderr.trim());
        return;
    }
    assert!(
        output.status.success() && stdout.contains("test result: ok. 1 passed"),
        "seed spawn custody failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// Host instrument: the real recorded cgroup contains the seed child, and a
/// finish reaps it then removes the entire cgroup and record while the parent
/// still holds its run lock. It runs in the existing delegated scope lane.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_seed_finish_closes_reaps_and_removes_its_own_operation() {
    let scratch = containment::tests::ScratchCgroup::new("seed-finish");
    containment::tests::mark_delegated(&scratch.cgroup);
    let (yard, directory) = containment::tests::run_directory("seed-finish");
    let run_lock = start::take_run_lock(&yard.0).unwrap().unwrap();
    let uid = nix::unistd::getuid().as_raw();
    let gid = nix::unistd::getgid().as_raw();
    let made = agent_cgroup::make_beneath(
        containment::Cgroup::open(scratch.cgroup.relative()),
        directory.as_fd(),
        "alpha",
        "seed-operation",
        (uid, gid),
        uid,
    )
    .unwrap();
    assert_eq!(
        containment::read_record(directory.as_fd(), "alpha").unwrap(),
        Some(made.record.clone())
    );
    let (admin, member) = UnixStream::pair().unwrap();
    let gift = start::high(member.as_raw_fd()).unwrap();
    let lock = start::high(run_lock.raw()).unwrap();
    let placement = made.constituents_procs().as_raw_fd();
    let raw_gift = gift.as_raw_fd();
    let raw_lock = lock.as_raw_fd();
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "printf 'READY\\n' >&3; /bin/cat <&3 >/dev/null"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: actual cgroup placement before descriptor changes, no allocation.
    unsafe {
        command.pre_exec(move || {
            containment::place_self(placement)?;
            start::place(raw_gift, 3)?;
            start::place(raw_lock, 9)?;
            start::seal_except(&[3, 9])
        });
    }
    let child = command.spawn().unwrap();
    drop(member);
    drop(gift);
    drop(lock);
    let mut admin = admin;
    admin
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut ready = [0; 6];
    admin.read_exact(&mut ready).unwrap();
    assert_eq!(&ready, b"READY\n");
    let cgroup = scratch.cgroup.child("weaver-alpha").unwrap();
    assert!(
        containment::Cgroup::open(&made.record.path)
            .unwrap()
            .child("constituents")
            .unwrap()
            .procs()
            .unwrap()
            .contains(&(child.id() as i32))
    );
    let record = made.record.clone();
    drop(made);
    let held_directory = directory.try_clone().unwrap();
    let operation = SeedChild {
        child,
        channel: admin.into(),
        room: directory.try_clone().unwrap(),
        group: OperationGroup {
            directory,
            agent: "alpha".into(),
            record,
        },
        _run_lock: &run_lock,
    };
    operation
        .finish(Instant::now() + Duration::from_secs(5))
        .unwrap();
    assert!(cgroup.populated().is_none() || cgroup.populated() == Some(false));
    assert!(scratch.cgroup.child("weaver-alpha").is_err());
    assert_eq!(
        containment::read_record(held_directory.as_fd(), "alpha"),
        Ok(None)
    );
    assert!(start::run_lock_held(&yard.0).unwrap());
}

/// Full joined host lane: as root inside a delegated root-owned scope, use
/// the same-tree member and complete coordinator pipeline. A mapped user
/// namespace cannot stand in for these prerequisites: unmapped host-root
/// ancestors correctly fail the preparation custody checks.
#[test]
#[ignore = "needs root in a delegated root-owned scope and the same-target weaver-state binary"]
fn containment_real_seed_member_copies_cleans_and_publishes() {
    assert!(
        nix::unistd::geteuid().is_root(),
        "this lane must run as root"
    );
    let executable = std::env::current_exe().unwrap();
    let binary = executable
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("weaver-state");
    assert!(
        binary.is_file(),
        "build the same-tree member first: {}",
        binary.display()
    );
    let member = inventory::MemberAccount {
        uid: 4242,
        gid: 4243,
    };
    let fixture = crate::seed::preparation::tests::fixture_with_member(member);
    let scratch = containment::tests::ScratchCgroup::new("full-seed");
    containment::tests::mark_delegated(&scratch.cgroup);
    let agent = format!("seed-test-{}", std::process::id());
    let (run_yard, directory) = containment::tests::run_directory("full-seed");
    let _invocation =
        start::take_invocation_lock(&run_yard.0, crate::verb_table::HolderVerb::Seed).unwrap();
    let run_lock = start::take_run_lock(&run_yard.0).unwrap().unwrap();
    let room_path = crate::prepare_territory(
        fixture.config.territory_fd().unwrap(),
        &fixture.config.territory,
        member,
    )
    .unwrap();
    let room = crate::save_points::open_directory(&room_path).unwrap();
    let made = agent_cgroup::make_beneath(
        containment::Cgroup::open(scratch.cgroup.relative()),
        directory.as_fd(),
        &agent,
        "fixture-seed-operation",
        (4244, 4245),
        0,
    )
    .unwrap();
    let group_name = agent_cgroup::cgroup_name(&agent);
    let recorded = made.record.clone();
    let held_directory = directory.try_clone().unwrap();
    assert_eq!(
        containment::read_record(directory.as_fd(), &agent).unwrap(),
        Some(recorded)
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut child = SeedChild::in_group(
        Launch {
            binary: &binary,
            room_path: &room_path,
            room,
            member,
        },
        &run_lock,
        directory,
        &agent,
        made,
        deadline,
    )
    .unwrap();
    child.check_alive().unwrap();
    let pid = child.child.id() as i32;
    assert!(
        scratch
            .cgroup
            .child(&group_name)
            .unwrap()
            .child("constituents")
            .unwrap()
            .procs()
            .unwrap()
            .contains(&pid)
    );
    let mut repository = crate::seed::repository(&fixture.config).unwrap();
    repository.require_unseeded().unwrap();
    let published =
        crate::seed::publish_member(&fixture.prepared, &mut repository, child, deadline)
            .unwrap_or_else(|error| {
                let log = std::fs::read_to_string(room_path.join("state.log")).unwrap_or_default();
                panic!("real seed failed: {error}\nmember log: {log}");
            });
    assert_eq!(published.seed_id, fixture.prepared.request().seed_id);
    assert!(published.parent_artifact_id.is_none());
    assert!(published.position.is_none());
    assert!(repository.require_unseeded().is_err());
    let draft = fixture.prepared.draft();
    let loaded = repository
        .load(
            None,
            &crate::save_points::serving::repository::LoadScope {
                session: &draft.agent().session,
                trace_path: crate::seed::trace_path(&draft.agent().trace_sink),
            },
        )
        .unwrap();
    assert_eq!(loaded.identity(), published);
    let mut dump = std::fs::File::from(loaded.dump().try_clone_to_owned().unwrap());
    let mut header = [0; 16];
    dump.read_exact(&mut header).unwrap();
    assert_eq!(&header, b"SQLite format 3\0");
    assert_eq!(
        containment::read_record(held_directory.as_fd(), &agent),
        Ok(None)
    );
    assert!(scratch.cgroup.child(&group_name).is_err());
    // SAFETY: this was our child. ECHILD establishes that finish reaped it;
    // an exited but unreaped process would still produce a waitpid result.
    assert_eq!(
        unsafe { nix::libc::waitpid(pid, std::ptr::null_mut(), nix::libc::WNOHANG) },
        -1
    );
    assert_eq!(
        io::Error::last_os_error().raw_os_error(),
        Some(nix::libc::ECHILD)
    );
    assert!(
        !room_path
            .join(format!("seed-{}.sqlite", published.seed_id))
            .exists()
    );
    assert!(
        start::run_lock_held(&run_yard.0).unwrap(),
        "the parent retains the lock through publication"
    );
    drop(run_lock);
    assert!(!start::run_lock_held(&run_yard.0).unwrap());
}

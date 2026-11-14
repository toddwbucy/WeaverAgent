//! A descriptor-sensitive fixture has its own process before it opens its files
//! or sockets. Other parallel test children inherit none of its descriptions.
//! An executable a test runs is written by a child process for the same reason
//! (#497).
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

const CASE: &str = "WEAVER_ADMIN_LOCK_CASE";

macro_rules! isolated {
    ($case:ident) => {{
        let _: fn() = $case;
        $crate::test_isolation::reexec(concat!(module_path!(), "::", stringify!($case)))
    }};
}
pub(crate) use isolated;

pub(crate) fn reexec(path: &'static str) -> bool {
    let name = path
        .strip_prefix(concat!(env!("CARGO_CRATE_NAME"), "::"))
        .expect("the test's module path starts with its crate");
    if std::env::var(CASE).ok().as_deref() == Some(name) {
        return false;
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            name,
            "--include-ignored",
            "--test-threads=1",
            "--nocapture",
        ])
        .env(CASE, name)
        .stdin(Stdio::null())
        .output()
        .expect("isolated admin fixture");
    assert!(
        output.status.success(),
        "isolated {name}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("test result: ok. 1 passed"),
        "the isolated command must execute its selected test: {name}"
    );
    true
}

/// **An executable a test writes and then runs is written by a child
/// process** (#497). Written in this process, its write description can be
/// inherited by another test thread's fork, which keeps it until that
/// child's own exec (close-on-exec acts only at exec), and an exec of the
/// file in that window is refused `ETXTBSY`. The bytes go to a staging file
/// beside it, which is never run, and `/bin/cp` writes the executable, so
/// this process never holds a description of it open for writing.
pub(crate) fn write_executable(path: &Path, bytes: impl AsRef<[u8]>, mode: u32) {
    write_executable_with(path, bytes.as_ref(), mode, || {});
}

/// [`write_executable`], with `while_staged` called while the staging file's
/// write description is open, which is where another thread's fork lands.
fn write_executable_with(path: &Path, bytes: &[u8], mode: u32, while_staged: impl FnOnce()) {
    let name = path.file_name().expect("an executable's path names a file");
    let staged = path.with_file_name(format!(".{}.staged", name.to_string_lossy()));
    {
        let mut file = std::fs::File::create(&staged).unwrap();
        file.write_all(bytes).unwrap();
        while_staged();
    }
    copy_executable(&staged, path, mode);
    std::fs::remove_file(&staged).unwrap();
}

/// A built executable copied in by `/bin/cp`, never by this process, as
/// [`write_executable`] writes one (#497).
pub(crate) fn copy_executable(from: impl AsRef<Path>, to: &Path, mode: u32) {
    let from = from.as_ref();
    let status = Command::new("/bin/cp")
        .arg("--")
        .arg(from)
        .arg(to)
        .stdin(Stdio::null())
        .status()
        .expect("/bin/cp runs");
    assert!(
        status.success(),
        "/bin/cp {} {}: {status}",
        from.display(),
        to.display()
    );
    std::fs::set_permissions(to, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn reexec_selects_its_case_from_an_unnamed_thread() {
    if std::thread::spawn(|| isolated!(reexec_selects_its_case_from_an_unnamed_thread))
        .join()
        .unwrap()
    {
        return;
    }
    assert_eq!(
        std::env::var(CASE).unwrap(),
        "test_isolation::reexec_selects_its_case_from_an_unnamed_thread"
    );
}

#[test]
fn exec_lock_probe() {
    let Ok(ready) = std::env::var("WEAVER_ADMIN_EXEC_READY_FD") else {
        return;
    };
    let ready: i32 = ready.parse().unwrap();
    let release: i32 = std::env::var("WEAVER_ADMIN_EXEC_RELEASE_FD")
        .unwrap()
        .parse()
        .unwrap();
    // SAFETY: the controlled fork inherits exactly these two pipe descriptions.
    let ready = unsafe { std::os::fd::BorrowedFd::borrow_raw(ready) };
    // SAFETY: the same producer retains this read end across exec.
    let release = unsafe { std::os::fd::BorrowedFd::borrow_raw(release) };
    assert_eq!(nix::unistd::write(ready, &[2]).unwrap(), 1);
    let mut byte = [0];
    let _ = nix::unistd::read(release, &mut byte);
}

#[test]
fn a_foreign_fork_retains_cloexec_locks_and_sockets_until_exec() {
    if isolated!(a_foreign_fork_retains_cloexec_locks_and_sockets_until_exec) {
        return;
    }
    use crate::start;
    use nix::fcntl::OFlag;
    use nix::sys::wait::waitpid;
    use nix::unistd::{ForkResult, Pid, fork, pipe2, read, write};
    use std::ffi::CString;
    use std::io::Read;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::net::UnixStream;

    struct Child(Pid);
    impl Drop for Child {
        fn drop(&mut self) {
            // SAFETY: this guard owns the unreaped child from the fork below.
            unsafe { nix::libc::kill(self.0.as_raw(), nix::libc::SIGKILL) };
            let _ = waitpid(self.0, None);
        }
    }
    let directory = start::tests::scratch("lock-inheritance");
    let lock = start::take_run_lock(&directory.0).unwrap().unwrap();
    let raw = lock.raw();
    let (mut socket, peer) = UnixStream::pair().unwrap();
    socket.set_nonblocking(true).unwrap();
    let (ready_read, ready_write) = pipe2(OFlag::O_CLOEXEC).unwrap();
    let (release_read, release_write) = pipe2(OFlag::O_CLOEXEC).unwrap();
    let ready_fd = ready_write.as_raw_fd();
    let release_fd = release_read.as_raw_fd();
    let executable = std::env::current_exe().unwrap();
    let program = CString::new(executable.as_os_str().as_bytes()).unwrap();
    let argv = [
        program.as_ptr(),
        c"--exact".as_ptr(),
        c"test_isolation::exec_lock_probe".as_ptr(),
        c"--nocapture".as_ptr(),
        std::ptr::null(),
    ];
    let ready_env = CString::new(format!("WEAVER_ADMIN_EXEC_READY_FD={ready_fd}")).unwrap();
    let release_env = CString::new(format!("WEAVER_ADMIN_EXEC_RELEASE_FD={release_fd}")).unwrap();
    let envp = [ready_env.as_ptr(), release_env.as_ptr(), std::ptr::null()];
    // SAFETY: the child only calls async-signal-safe libc functions with
    // prebuilt storage until exec. The parent owns and reaps this exact pid.
    let child = match unsafe { fork() }.unwrap() {
        ForkResult::Child => unsafe {
            nix::libc::close(ready_read.as_raw_fd());
            nix::libc::close(release_write.as_raw_fd());
            let flags = nix::libc::fcntl(raw, nix::libc::F_GETFD);
            let evidence = u8::from(flags >= 0 && flags & nix::libc::FD_CLOEXEC != 0);
            if nix::libc::write(ready_fd, (&evidence as *const u8).cast(), 1) != 1 {
                nix::libc::_exit(101);
            }
            let mut release = 0u8;
            if nix::libc::read(release_fd, (&mut release as *mut u8).cast(), 1) != 1 {
                nix::libc::_exit(102);
            }
            // Only the handshake pipes survive. The probe acknowledges from
            // user code after all of exec's descriptor cleanup has completed.
            if nix::libc::fcntl(ready_fd, nix::libc::F_SETFD, 0) == -1
                || nix::libc::fcntl(release_fd, nix::libc::F_SETFD, 0) == -1
            {
                nix::libc::_exit(104);
            }
            nix::libc::execve(program.as_ptr(), argv.as_ptr(), envp.as_ptr());
            nix::libc::_exit(103);
        },
        ForkResult::Parent { child } => Child(child),
    };
    drop(ready_write);
    drop(release_read);
    let mut evidence = [0u8];
    assert_eq!(read(&ready_read, &mut evidence).unwrap(), 1);
    assert_eq!(
        evidence,
        [1],
        "the foreign child inherited a CLOEXEC descriptor"
    );
    assert_eq!(
        std::fs::read_link(format!("/proc/{}/fd/{raw}", child.0)).unwrap(),
        directory.0.join("run.lock").canonicalize().unwrap()
    );
    drop(lock);
    drop(peer);
    assert_eq!(
        socket.read(&mut evidence).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "the foreign child also prevents the parent's socket close from yielding EOF"
    );
    assert!(
        start::run_lock_held(&directory.0).unwrap(),
        "closing the taker's copy does not release the inherited description"
    );
    assert!(start::take_run_lock(&directory.0).unwrap().is_none());
    assert_eq!(write(&release_write, &[1]).unwrap(), 1);
    // One CLOEXEC pipe's EOF can precede cleanup of another descriptor. An
    // acknowledgment from the executed probe observes the completed cleanup.
    assert_eq!(read(&ready_read, &mut evidence).unwrap(), 1);
    assert_eq!(evidence, [2]);
    // Inspect without reaping: the guard must remain this pid's sole reaper,
    // including when an unexpected early exit fails this assertion.
    assert_eq!(
        std::fs::read_link(format!("/proc/{}/exe", child.0)).unwrap(),
        executable.canonicalize().unwrap()
    );
    assert!(!start::run_lock_held(&directory.0).unwrap());
    assert_eq!(socket.read(&mut evidence).unwrap(), 0);
    let leaked = start::take_run_lock(&directory.0).unwrap().unwrap();
    assert!(
        start::run_lock_held(&directory.0).unwrap(),
        "a genuinely retained parent lock must still be detected"
    );
    assert!(start::take_run_lock(&directory.0).unwrap().is_none());
    drop(leaked);
    assert!(!start::run_lock_held(&directory.0).unwrap());
}

/// **A fork that holds an executable's write description refuses its exec,
/// and [`write_executable`]'s does not** (#497): a fork that lands while this
/// process writes a file keeps that description until its own exec, and an
/// exec of the file meanwhile is refused `ETXTBSY`, the full suite's failure.
/// Here the fork lands by construction, in a process of its own, and pauses
/// until it is killed. The control: the file runs once that fork is reaped.
///
/// Perturbation: write the executable itself in this process in place of its
/// staging file, and the second exec is refused.
#[test]
fn a_fork_holding_a_write_refuses_its_exec_and_a_staged_write_does_not() {
    if isolated!(a_fork_holding_a_write_refuses_its_exec_and_a_staged_write_does_not) {
        return;
    }
    use nix::sys::wait::waitpid;
    use nix::unistd::{ForkResult, Pid, fork};

    /// A fork that holds every description this process held at the fork.
    struct Held(Pid);
    impl Drop for Held {
        fn drop(&mut self) {
            // SAFETY: this guard owns the unreaped child of `held` below.
            unsafe { nix::libc::kill(self.0.as_raw(), nix::libc::SIGKILL) };
            let _ = waitpid(self.0, None);
        }
    }
    fn held() -> Held {
        // SAFETY: the isolated process runs this test alone, and the child
        // calls only `pause`, which is async-signal-safe, until it is killed.
        match unsafe { fork() }.unwrap() {
            ForkResult::Child => loop {
                unsafe { nix::libc::pause() };
            },
            ForkResult::Parent { child } => Held(child),
        }
    }
    let directory = crate::start::tests::scratch("exec-busy");
    let script = b"#!/bin/sh\nexit 0\n";

    let direct = directory.0.join("direct");
    let holder = {
        let mut file = std::fs::File::create(&direct).unwrap();
        file.write_all(script).unwrap();
        held()
    };
    std::fs::set_permissions(&direct, std::fs::Permissions::from_mode(0o755)).unwrap();
    let refused = Command::new(&direct).status().unwrap_err();
    assert_eq!(
        refused.raw_os_error(),
        Some(nix::libc::ETXTBSY),
        "the fork holds the write: {refused}"
    );
    drop(holder);
    assert!(
        Command::new(&direct).status().unwrap().success(),
        "the control: it runs once the fork is reaped"
    );

    let staged = directory.0.join("staged");
    let mut holder = None;
    write_executable_with(&staged, script, 0o755, || holder = Some(held()));
    assert!(
        holder.is_some(),
        "the fork landed while the staging file was open"
    );
    let ran = Command::new(&staged).status();
    drop(holder);
    assert!(
        ran.as_ref().is_ok_and(|status| status.success()),
        "the fork holds only the staging file: {ran:?}"
    );
}

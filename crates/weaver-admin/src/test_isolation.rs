//! A descriptor-sensitive fixture has its own process before it opens its files
//! or sockets. Other parallel test children inherit none of its descriptions.
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

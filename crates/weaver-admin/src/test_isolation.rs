//! A descriptor-sensitive fixture has its own process before it opens its files
//! or sockets. Other parallel test children inherit none of its descriptions.
use std::process::{Command, Stdio};

const CASE: &str = "WEAVER_ADMIN_LOCK_CASE";

pub(crate) fn reexec() -> bool {
    let thread = std::thread::current();
    let name = thread.name().expect("named libtest case");
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
        String::from_utf8_lossy(&output.stdout)
            .rsplit("test result:")
            .next()
            .is_some_and(|result| result.starts_with(" ok. 1 passed; 0 failed; 0 ignored;")),
        "the isolated command must execute its selected test: {name}"
    );
    true
}

#[test]
fn a_foreign_fork_retains_cloexec_locks_and_sockets_until_exec() {
    if reexec() {
        return;
    }
    use crate::start;
    use nix::fcntl::OFlag;
    use nix::sys::wait::waitpid;
    use nix::unistd::{ForkResult, Pid, fork, pipe2, read, write};
    use std::io::Read;
    use std::os::fd::AsRawFd;
    use std::os::unix::net::UnixStream;

    struct Child(Pid);
    impl Drop for Child {
        fn drop(&mut self) {
            // SAFETY: this guard owns the unreaped child from the fork below.
            unsafe { nix::libc::kill(self.0.as_raw(), nix::libc::SIGKILL) };
            let _ = waitpid(self.0, None);
        }
    }
    let path = std::env::temp_dir().join(format!(
        "weaver-admin-lock-inheritance-{}",
        std::process::id()
    ));
    std::fs::create_dir(&path).unwrap();
    let directory = crate::scratch::Scratch(path);
    let lock = start::take_run_lock(&directory.0).unwrap().unwrap();
    let raw = lock.raw();
    let (mut socket, peer) = UnixStream::pair().unwrap();
    socket.set_nonblocking(true).unwrap();
    let (ready_read, ready_write) = pipe2(OFlag::O_CLOEXEC).unwrap();
    let (release_read, release_write) = pipe2(OFlag::O_CLOEXEC).unwrap();
    let ready_fd = ready_write.as_raw_fd();
    let release_fd = release_read.as_raw_fd();
    let program = c"/bin/sleep";
    let argv = [program.as_ptr(), c"30".as_ptr(), std::ptr::null()];
    let envp = [std::ptr::null()];
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
        directory.0.join("run.lock")
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
    // EOF on the CLOEXEC pipe observes exec, rather than guessing a delay.
    assert_eq!(read(&ready_read, &mut evidence).unwrap(), 0);
    // Inspect without reaping: the guard must remain this pid's sole reaper,
    // including when an unexpected early exit fails this assertion.
    assert_eq!(
        std::fs::read_link(format!("/proc/{}/exe", child.0)).unwrap(),
        std::fs::canonicalize("/bin/sleep").unwrap()
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

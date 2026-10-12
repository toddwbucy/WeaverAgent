//! Test-only: the run lock suite of `main.rs`, moved out of the production file by #118.

/// **The member holds the run lock for its life**, per `weaver-state-Spec`
/// section 5: the inherited descriptor is marked close-on-exec and stays
/// open on the same file, read from outside through the descriptor table.
/// The number is a fresh one above any the test harness uses, so the test
/// clobbers nothing a concurrent test holds. Perturbation: close the
/// descriptor in `keep_run_lock` and the table no longer shows the file,
/// which is a load killed before its worker leaving a member the next
/// load cannot find.
#[test]
fn the_member_keeps_the_run_lock_close_on_exec() {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::MetadataExt;
    let path = std::env::temp_dir().join(format!("weaver-run-lock-{}", std::process::id()));
    let file = std::fs::File::create(&path).expect("a stand-in lock file");
    // SAFETY: F_DUPFD on a descriptor this test owns answers a fresh
    // number at or above the floor, leaving the original untouched.
    let fd = unsafe { nix::libc::fcntl(file.as_raw_fd(), nix::libc::F_DUPFD, 700) };
    assert!(fd >= 700, "a fresh number");
    // The duplicate arrives without the flag, as an inherited one does.
    // SAFETY: F_GETFD on the number just made.
    let before = unsafe { nix::libc::fcntl(fd, nix::libc::F_GETFD) };
    assert_eq!(before & nix::libc::FD_CLOEXEC, 0);

    assert!(
        super::keep_run_lock(fd).unwrap(),
        "a descriptor stood there"
    );

    // SAFETY: F_GETFD on the number, which must still be open.
    let after = unsafe { nix::libc::fcntl(fd, nix::libc::F_GETFD) };
    assert_ne!(after, -1, "the descriptor is kept, never closed");
    assert_ne!(after & nix::libc::FD_CLOEXEC, 0, "and marked close-on-exec");
    let held = std::fs::metadata(format!("/proc/self/fd/{fd}")).expect("in the table");
    let original = file.metadata().unwrap();
    assert_eq!((held.dev(), held.ino()), (original.dev(), original.ino()));
    // A number holding nothing is skipped, a hand-run member's case.
    // SAFETY: closing the test's own duplicate.
    unsafe { nix::libc::close(fd) };
    assert!(!super::keep_run_lock(fd).unwrap(), "nothing to keep");
    drop(std::fs::remove_file(&path));
}

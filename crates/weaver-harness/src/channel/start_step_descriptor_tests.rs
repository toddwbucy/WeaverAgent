//! Test-only: the `start_step_descriptor_tests` suite of `channel.rs`,
//! moved out of the production file by #118.

/// **An inherited number is marked close-on-exec and kept**, per
/// `weaver-harness-Spec` section 2.2, so no organ the worker forks
/// inherits the run lock or the relay's write end. The number is a fresh
/// one far above any the test harness uses. Perturbation: drop the
/// `F_SETFD` from `keep_close_on_exec` and the flag stays clear.
#[test]
fn an_inherited_number_is_kept_close_on_exec() {
    let (read_end, write_end) = nix::unistd::pipe().expect("a pipe");
    // SAFETY: F_DUPFD on a descriptor this test owns answers a fresh
    // number without the flag, as an inherited one arrives.
    let fd = unsafe {
        nix::libc::fcntl(
            std::os::fd::AsRawFd::as_raw_fd(&write_end),
            nix::libc::F_DUPFD,
            800,
        )
    };
    assert!(fd >= 800);
    assert!(
        super::keep_close_on_exec(fd).unwrap(),
        "a descriptor stood there"
    );
    // SAFETY: F_GETFD on the number, which must still be open.
    let flags = unsafe { nix::libc::fcntl(fd, nix::libc::F_GETFD) };
    assert_ne!(flags, -1, "kept, never closed");
    assert_ne!(flags & nix::libc::FD_CLOEXEC, 0, "marked close-on-exec");
    // SAFETY: closing the test's own duplicate.
    unsafe { nix::libc::close(fd) };
    assert!(
        !super::keep_close_on_exec(fd).unwrap(),
        "an empty number is skipped"
    );
    drop((read_end, write_end));
}

//! The confinement's own tests (#29, `weaver-gate-Spec` section 8.1): the
//! instruments of `tests/landlock.rs` turned on this module's builders, each
//! refusal paired with a control that removes the one right or layer.
//!
//! The ones that run a shell are `#[ignore]`d and need `WEAVER_BUSYBOX`, a
//! static busybox built from the stack's pinned configuration:
//!
//! ```text
//! WEAVER_BUSYBOX=<busybox> cargo test -p weaver-gate --locked --lib confine -- --include-ignored
//! ```

use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

use super::*;

/// The default listed set, the interpreter not among them.
const APPLETS: [&str; 14] = [
    "ls", "cat", "head", "tail", "wc", "grep", "sort", "uniq", "cut", "tr", "date", "echo", "pwd",
    "mktemp",
];

/// This suite's uid, which owns its scratch `commands/` as root owns the
/// stack's.
fn own() -> u32 {
    nix::unistd::geteuid().as_raw()
}

fn busybox() -> PathBuf {
    let path = std::env::var_os("WEAVER_BUSYBOX")
        .map(PathBuf::from)
        .expect("WEAVER_BUSYBOX names a static busybox built from the pinned configuration");
    assert!(path.is_file(), "{} is not a file", path.display());
    path
}

/// A scratch layout as production stands: `libexec/busybox`, `commands/`
/// with one symlink per listed applet and no `sh`, and `home/`.
struct Layout {
    base: PathBuf,
}

impl Layout {
    fn new(tag: &str) -> Layout {
        let base =
            std::env::temp_dir().join(format!("weaver-confine-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for dir in ["libexec", "commands", "home"] {
            std::fs::create_dir_all(base.join(dir)).unwrap();
        }
        std::fs::copy(busybox(), base.join("libexec/busybox")).unwrap();
        for applet in APPLETS {
            std::os::unix::fs::symlink(
                base.join("libexec/busybox"),
                base.join("commands").join(applet),
            )
            .unwrap();
        }
        Layout { base }
    }

    fn grant(&self) -> ShellGrant {
        ShellGrant {
            binary: self.base.join("libexec/busybox"),
            commands: self.base.join("commands"),
        }
    }

    fn shell(&self) -> Shell {
        Shell::confined_in(&self.grant(), self.base.join("home"), own())
            .expect("the confinement builds")
    }
}

impl Drop for Layout {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

/// A filter test's scratch home, removed when the test ends, a failed
/// assertion included.
struct ScratchHome(PathBuf);

impl ScratchHome {
    fn new(tag: &str) -> ScratchHome {
        let home =
            std::env::temp_dir().join(format!("weaver-confine-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        ScratchHome(home)
    }

    /// The confinement on this home, granting nothing the probes run: the
    /// test binary as the grant's file, the home as its commands directory.
    fn shell(&self) -> Shell {
        Shell::confined_in(
            &ShellGrant {
                binary: std::env::current_exe().unwrap(),
                commands: self.0.clone(),
            },
            self.0.clone(),
            own(),
        )
        .expect("the confinement builds")
    }
}

impl Drop for ScratchHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs `script` as an execution's child would, without the move into
/// `tools/`: the command, its layers, no input. A spawn that meets
/// `ETXTBSY` (a parallel test's fork holding a fresh copy open for write) is
/// retried, bounded.
fn run(shell: &Shell, script: &str) -> Output {
    let mut attempts = 0;
    loop {
        // SAFETY: the first step makes no call at all.
        let mut command = unsafe { shell.command(script, || Ok(())) };
        match command.stdin(Stdio::null()).output() {
            Err(error) if error.raw_os_error() == Some(libc::ETXTBSY) && attempts < 100 => {
                attempts += 1;
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            other => break other.expect("the shell spawns"),
        }
    }
}

fn said(output: &Output) -> String {
    format!(
        "status {:?}, stdout {:?}, stderr {:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Adds one rule to a built shell's ruleset, before any child restricts
/// itself: a control's one right.
fn add_right(shell: &Shell, path: &Path, access: u64) {
    let layers = shell.layers.as_ref().expect("a confined shell has layers");
    allow(&layers.ruleset, path, access).expect("the control's rule adds");
}

// ------------------------------------------------------------ no shell needed

/// **A failed ABI query, or one below the floor, refuses** (section 8.1):
/// nothing runs unconfined on a kernel without Landlock. The floor itself
/// and above pass.
///
/// Perturbation: let `abi_at_floor` answer `Ok` for a failed query and the
/// first case fails.
#[test]
fn the_abi_floor_fails_closed() {
    assert_eq!(
        abi_at_floor(Err(libc::EOPNOTSUPP)),
        Err(ConfineFault::AbiBelowFloor(None))
    );
    assert_eq!(
        abi_at_floor(Ok(5)),
        Err(ConfineFault::AbiBelowFloor(Some(5)))
    );
    assert_eq!(abi_at_floor(Ok(LANDLOCK_ABI_FLOOR)), Ok(LANDLOCK_ABI_FLOOR));
    assert_eq!(abi_at_floor(Ok(10)), Ok(10));
}

/// **The rlimits are the ruled values** (the operator's ruling of 2026-10-16
/// on #29), and the child sets each soft and hard alike.
#[test]
fn the_rlimits_are_the_ruled_values() {
    assert_eq!(SHELL_RLIMIT_FSIZE, 64 * 1024 * 1024);
    assert_eq!(SHELL_RLIMIT_NOFILE, 64);
    assert_eq!(SHELL_RLIMIT_AS, 512 * 1024 * 1024);
    assert_eq!(SHELL_RLIMIT_CORE, 0);
}

/// **The layers' seccomp filter refuses every socket, and the control opens
/// them** (section 8.1's sockets row, as M9): a child applies [`apply`] on a
/// ruleset granting nothing and tries `socket` for inet TCP and UDP and
/// `AF_UNIX`, `socketpair`, and `io_uring_setup`. With no-new-privs alone,
/// the same calls succeed.
///
/// Perturbation: take `SYS_socket` out of [`SHELL_FILTER`] and the inet
/// and unix sockets open.
#[test]
#[ignore = "a measurement of this box's kernel: needs Landlock ABI 6 and io_uring"]
fn the_filter_refuses_every_socket() {
    let home = ScratchHome::new("sockets");
    let empty = home.shell();
    let ruleset = empty.layers.as_ref().unwrap().ruleset.as_raw_fd();
    let filtered = socket_probe(Some(ruleset));
    assert_eq!(
        filtered, 0,
        "every kind refused under the layers: {filtered:#b}"
    );
    let control = socket_probe(None);
    assert_eq!(
        control, 0b11111,
        "every kind opens with no-new-privs alone: {control:#b}"
    );
}

/// A bit per kind that opened: inet TCP, inet UDP, unix stream, a unix
/// socketpair, an io_uring.
fn socket_probe(ruleset: Option<RawFd>) -> u8 {
    let (read, write) = nix::unistd::pipe().unwrap();
    // SAFETY: the child makes raw system calls only and exits.
    match unsafe { nix::unistd::fork() }.unwrap() {
        nix::unistd::ForkResult::Child => unsafe {
            let applied = match ruleset {
                Some(ruleset) => apply(ruleset, SHELL_FILTER.as_ptr() as usize).is_ok(),
                None => libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) == 0,
            };
            if !applied {
                libc::_exit(2);
            }
            let mut opened = 0u8;
            for (bit, (family, kind)) in [
                (libc::AF_INET, libc::SOCK_STREAM),
                (libc::AF_INET, libc::SOCK_DGRAM),
                (libc::AF_UNIX, libc::SOCK_STREAM),
            ]
            .into_iter()
            .enumerate()
            {
                if libc::socket(family, kind | libc::SOCK_CLOEXEC, 0) >= 0 {
                    opened |= 1 << bit;
                }
            }
            let mut pair = [0i32; 2];
            if libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, pair.as_mut_ptr()) == 0 {
                opened |= 1 << 3;
            }
            let mut params = [0u8; 120];
            if libc::syscall(libc::SYS_io_uring_setup, 4u32, params.as_mut_ptr()) >= 0 {
                opened |= 1 << 4;
            }
            libc::write(write.as_raw_fd(), [opened].as_ptr().cast(), 1);
            libc::_exit(0);
        },
        nix::unistd::ForkResult::Parent { child } => {
            drop(write);
            let status = nix::sys::wait::waitpid(child, None).unwrap();
            assert_eq!(
                status,
                nix::sys::wait::WaitStatus::Exited(child, 0),
                "the probe applied its layers"
            );
            let mut byte = [0u8; 1];
            assert_eq!(nix::unistd::read(&read, &mut byte).unwrap(), 1);
            byte[0]
        }
    }
}

// ------------------------------------------------------------ the file attributes

/// The filter's verdict on one call, by [`SHELL_FILTER`]'s own instructions
/// run over a `seccomp_data` as the kernel runs them: `nr`, `arch`, the
/// instruction pointer and six arguments, little-endian, `args[1]` at 24.
fn verdict(arch: u32, nr: u32, arg1: u64) -> u32 {
    let mut data = [0u8; 64];
    data[0..4].copy_from_slice(&nr.to_le_bytes());
    data[4..8].copy_from_slice(&arch.to_le_bytes());
    data[24..32].copy_from_slice(&arg1.to_le_bytes());
    let mut accumulator = 0u32;
    let mut next = 0usize;
    loop {
        let SockFilter { code, jt, jf, k } = SHELL_FILTER[next];
        next += 1;
        match code {
            BPF_LD_W_ABS => {
                let at = k as usize;
                accumulator = u32::from_le_bytes(data[at..at + 4].try_into().unwrap());
            }
            BPF_JEQ_K => next += usize::from(if accumulator == k { jt } else { jf }),
            BPF_JGE_K => next += usize::from(if accumulator >= k { jt } else { jf }),
            BPF_RET_K => return k,
            other => panic!("an instruction this reader does not know: {other:#x}"),
        }
    }
}

/// **The numbers the filter compares are the headers'**: `FS_IOC_FSSETXATTR`
/// and the two `FS_IOC_SETFLAGS` from `linux/fs.h`, `file_setattr` from
/// `asm/unistd_64.h`.
#[test]
fn the_attribute_numbers_are_the_headers() {
    assert_eq!(FS_IOC_FSSETXATTR, 0x401C_5820);
    assert_eq!(FS_IOC_SETFLAGS, 0x4008_6602);
    assert_eq!(FS_IOC32_SETFLAGS, 0x4004_6602);
    assert_eq!(SYS_FILE_SETATTR, 469);
}

/// **The program decides each call as section 8.1 states**, read by the
/// program's own instructions, so a jump offset that lands one instruction
/// off fails here without a kernel: another architecture, an x32 call,
/// `io_uring_setup`, the two socket calls, `file_setattr` and the three
/// attribute commands are refused `EACCES`, a command with its high word set
/// among them, and a read, `file_getattr`, the attribute reads, a terminal
/// `ioctl` and a non-`ioctl` call carrying a refused command as its second
/// argument are allowed.
///
/// Perturbation: shift any refusal's jump by one, or compare the command's
/// high word as well, and a case here turns.
#[test]
fn the_filter_decides_each_call_as_section_8_1_states() {
    const X86_64: u32 = AUDIT_ARCH_X86_64;
    const I386: u32 = 0x4000_0003;
    let ioctl = libc::SYS_ioctl as u32;
    let refused: [(&str, u32, u32, u64); 11] = [
        ("another architecture", I386, 20, 0),
        (
            "an x32 call",
            X86_64,
            X32_SYSCALL_BIT | libc::SYS_getpid as u32,
            0,
        ),
        ("io_uring_setup", X86_64, libc::SYS_io_uring_setup as u32, 0),
        ("socket", X86_64, libc::SYS_socket as u32, 0),
        ("socketpair", X86_64, libc::SYS_socketpair as u32, 0),
        ("file_setattr", X86_64, 469, 0),
        ("FS_IOC_FSSETXATTR", X86_64, ioctl, 0x401C_5820),
        ("FS_IOC_SETFLAGS", X86_64, ioctl, 0x4008_6602),
        ("FS_IOC32_SETFLAGS", X86_64, ioctl, 0x4004_6602),
        (
            "FS_IOC_SETFLAGS with bit 32 set",
            X86_64,
            ioctl,
            1 << 32 | 0x4008_6602,
        ),
        (
            "FS_IOC_FSSETXATTR with every high bit set",
            X86_64,
            ioctl,
            0xFFFF_FFFF_401C_5820,
        ),
    ];
    for (what, arch, nr, arg1) in refused {
        assert_eq!(verdict(arch, nr, arg1), DENY, "{what} is refused EACCES");
    }
    let allowed: [(&str, u32, u64); 7] = [
        ("read", libc::SYS_read as u32, 0),
        ("file_getattr", 468, 0),
        ("FS_IOC_GETFLAGS", ioctl, 0x8008_6601),
        ("FS_IOC_FSGETXATTR", ioctl, 0x801C_581F),
        ("TCGETS", ioctl, 0x5401),
        (
            "a command whose high word alone is FS_IOC_SETFLAGS",
            ioctl,
            0x4008_6602 << 32,
        ),
        (
            "write carrying FS_IOC_SETFLAGS as its second argument",
            libc::SYS_write as u32,
            0x4008_6602,
        ),
    ];
    for (what, nr, arg1) in allowed {
        assert_eq!(
            verdict(X86_64, nr, arg1),
            SECCOMP_RET_ALLOW,
            "{what} is allowed"
        );
    }
}

/// The calls [`attribute_probe`] makes, in its order: two reads, then each
/// write back of what they read.
const ATTRIBUTE_CALLS: [&str; 7] = [
    "FS_IOC_FSGETXATTR",
    "FS_IOC_GETFLAGS",
    "FS_IOC_FSSETXATTR",
    "FS_IOC_SETFLAGS",
    "FS_IOC32_SETFLAGS",
    "file_setattr",
    "FS_IOC_SETFLAGS with bit 32 of the command set",
];

/// **The layers' seccomp filter refuses every change of a file's attributes,
/// and the control's answers are the kernel's** (section 8.1's file
/// attributes row, #587): a child holding a file it made in its home applies
/// [`apply`] on a ruleset granting that home, reads the file's attributes by
/// `FS_IOC_FSGETXATTR` and `FS_IOC_GETFLAGS`, then writes them back unchanged
/// by `FS_IOC_FSSETXATTR`, `FS_IOC_SETFLAGS`, `FS_IOC32_SETFLAGS`,
/// `file_setattr` (after `file_getattr`), and `FS_IOC_SETFLAGS` with bit 32 of
/// the command set. Under the layers each write answers `EACCES`, and each
/// read answers as it does without them. With no-new-privs alone no call
/// answers `EACCES`: each answers what the home's filesystem does (success
/// where it keeps the attribute, `EOPNOTSUPP` or `ENOTTY` where it does not,
/// `ENOSYS` for `file_setattr` on a kernel without it), measured on tmpfs and
/// zfs, so the claim holds wherever the home sits. A box whose security module
/// denies the `ioctl` answers `EACCES` in the control too, and fails here by
/// name: the shell's layers use no such module.
///
/// Perturbation: take each comparison out of [`SHELL_FILTER`] in turn, and its
/// case answers the kernel's word. Require the command's high word to be zero
/// before comparing it, and the bit-32 case does.
#[test]
#[ignore = "a measurement of this box's kernel: needs Landlock ABI 6"]
fn the_filter_refuses_every_attribute_change() {
    let home = ScratchHome::new("attributes");
    let confined = home.shell();
    let ruleset = confined.layers.as_ref().unwrap().ruleset.as_raw_fd();
    let filtered = attribute_probe(&home.0, Some(ruleset));
    let control = attribute_probe(&home.0, None);
    for (at, what) in ATTRIBUTE_CALLS.iter().enumerate() {
        if at < 2 {
            assert_eq!(
                filtered[at], control[at],
                "{what} reads under the layers as without them"
            );
        } else {
            assert_eq!(
                filtered[at],
                -libc::EACCES,
                "{what} is refused EACCES under the layers"
            );
        }
        assert_ne!(
            control[at],
            -libc::EACCES,
            "{what} with no-new-privs alone answers the kernel's word"
        );
    }
}

/// 0 or `-errno`, from a raw call's result.
fn answered(result: libc::c_long) -> i32 {
    if result < 0 {
        -nix::errno::Errno::last_raw()
    } else {
        0
    }
}

/// What each of [`ATTRIBUTE_CALLS`] answered, 0 or `-errno`, on a file in
/// `home`, the child's layers applied where a ruleset is given and
/// no-new-privs alone where none is.
fn attribute_probe(home: &Path, ruleset: Option<RawFd>) -> [i32; 7] {
    use std::io::Read;
    let path = home.join("attributes");
    std::fs::write(&path, b"x").unwrap();
    let file = std::fs::File::open(&path).unwrap();
    let fd = file.as_raw_fd();
    let fs_ioc_fsgetxattr = libc::_IOR::<[u8; 28]>('X' as u32, 31);
    let (read, write) = nix::unistd::pipe().unwrap();
    // SAFETY: the child makes raw system calls only and exits.
    match unsafe { nix::unistd::fork() }.unwrap() {
        nix::unistd::ForkResult::Child => unsafe {
            let applied = match ruleset {
                Some(ruleset) => apply(ruleset, SHELL_FILTER.as_ptr() as usize).is_ok(),
                None => libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) == 0,
            };
            if !applied {
                libc::_exit(2);
            }
            let mut fsx = [0u8; 28];
            let mut flags: libc::c_long = 0;
            let mut attr = [0u8; 24];
            let mut out = [0i32; 7];
            out[0] = answered(libc::ioctl(fd, fs_ioc_fsgetxattr, fsx.as_mut_ptr()).into());
            out[1] = answered(libc::ioctl(fd, libc::FS_IOC_GETFLAGS, &mut flags).into());
            let flags32 = flags as libc::c_int;
            out[2] =
                answered(libc::ioctl(fd, FS_IOC_FSSETXATTR as libc::Ioctl, fsx.as_ptr()).into());
            out[3] = answered(libc::ioctl(fd, libc::FS_IOC_SETFLAGS, &flags).into());
            out[4] = answered(libc::ioctl(fd, libc::FS_IOC32_SETFLAGS, &flags32).into());
            libc::syscall(
                468,
                fd,
                c"".as_ptr(),
                attr.as_mut_ptr(),
                24usize,
                libc::AT_EMPTY_PATH,
            );
            out[5] = answered(libc::syscall(
                469,
                fd,
                c"".as_ptr(),
                attr.as_ptr(),
                24usize,
                libc::AT_EMPTY_PATH,
            ));
            out[6] = answered(libc::syscall(
                libc::SYS_ioctl,
                fd,
                1u64 << 32 | libc::FS_IOC_SETFLAGS,
                &flags,
            ));
            let mut bytes = [0u8; 28];
            for (at, value) in out.iter().enumerate() {
                bytes[at * 4..at * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
            libc::write(write.as_raw_fd(), bytes.as_ptr().cast(), bytes.len());
            libc::_exit(0);
        },
        nix::unistd::ForkResult::Parent { child } => {
            drop(write);
            let status = nix::sys::wait::waitpid(child, None).unwrap();
            assert_eq!(
                status,
                nix::sys::wait::WaitStatus::Exited(child, 0),
                "the probe applied its layers"
            );
            let mut bytes = [0u8; 28];
            std::fs::File::from(read).read_exact(&mut bytes).unwrap();
            drop(file);
            let mut out = [0i32; 7];
            for (at, value) in out.iter_mut().enumerate() {
                *value = i32::from_le_bytes(bytes[at * 4..at * 4 + 4].try_into().unwrap());
            }
            out
        }
    }
}

// ------------------------------------------------------------ the shell under the layers

/// **The listed set runs under the layers, the shell is `sh`, and its
/// redirects and temporaries land inside the wall** (as M11 and M12):
/// `argv[0]` is `sh`, a pipeline of listed applets runs through `PATH`,
/// `2>/dev/null` and `>` over a home file work, and `mktemp` lands under
/// `TMPDIR`, home's `.tmp`, which the confinement made.
#[test]
#[ignore = "runs the static busybox: needs WEAVER_BUSYBOX and Landlock ABI 6"]
fn the_listed_set_runs_under_the_layers() {
    let layout = Layout::new("runs");
    let shell = layout.shell();
    assert!(
        layout.base.join("home/.tmp").is_dir(),
        "home's .tmp is made at the build"
    );
    let out = run(
        &shell,
        "echo zero=$0; echo a b c | tr ' ' '\\n' | sort | wc -l; ls /nonexistent 2>/dev/null; \
         echo one > f; echo two > f; cat f; t=$(mktemp) && echo tmp=$t",
    );
    let said = said(&out);
    assert!(out.status.success(), "{said}");
    let text = stdout(&out);
    assert!(text.contains("zero=sh\n"), "{said}");
    assert!(text.contains("3\n"), "{said}");
    assert!(text.contains("two\n") && !text.contains("one\n"), "{said}");
    assert!(text.contains("/home/.tmp/tmp."), "{said}");
}

/// **A program outside the build is refused, and the control runs it** (as
/// M10 and M12): an identical static copy of the busybox placed outside the
/// grant is refused 126, the grant naming the file and not its contents, and
/// with execute on that one copy added to the same ruleset it runs.
#[test]
#[ignore = "runs the static busybox: needs WEAVER_BUSYBOX and Landlock ABI 6"]
fn a_program_outside_the_build_is_refused() {
    let layout = Layout::new("outside");
    let elsewhere = layout.base.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let outside = elsewhere.join("busybox");
    std::fs::copy(busybox(), &outside).unwrap();
    let probe = format!("{} echo ran; echo rc=$?", outside.display());
    let refused = run(&layout.shell(), &probe);
    assert!(stdout(&refused).contains("rc=126"), "{}", said(&refused));
    let granted = layout.shell();
    add_right(&granted, &outside, FS_EXECUTE | FS_READ_FILE);
    let control = run(&granted, &probe);
    assert!(stdout(&control).contains("ran\nrc=0"), "{}", said(&control));
}

/// **`.tmp` must be a directory of its own** (section 8.1): a `.tmp` that
/// stands as a file, or as a symlink out of home, refuses the confinement,
/// since every temporary would then be refused by the wall. The control: a
/// home with no `.tmp` builds and makes it.
///
/// Perturbation: accept any existing `.tmp` and both refusals build.
#[test]
#[ignore = "a measurement of this box's kernel and a built busybox: needs Landlock ABI 6 and WEAVER_BUSYBOX"]
fn a_tmp_that_is_not_a_directory_refuses() {
    let layout = Layout::new("tmp");
    let home = layout.base.join("home");
    std::fs::write(home.join(".tmp"), b"a file").unwrap();
    let file = Shell::confined_in(&layout.grant(), home.clone(), own());
    assert!(
        matches!(file, Err(ConfineFault::TmpDir(_))),
        "a file: {:?}",
        file.err()
    );
    std::fs::remove_file(home.join(".tmp")).unwrap();
    std::os::unix::fs::symlink(std::env::temp_dir(), home.join(".tmp")).unwrap();
    let link = Shell::confined_in(&layout.grant(), home.clone(), own());
    assert!(
        matches!(link, Err(ConfineFault::TmpDir(_))),
        "a symlink: {:?}",
        link.err()
    );
    std::fs::remove_file(home.join(".tmp")).unwrap();
    assert!(
        Shell::confined_in(&layout.grant(), home.clone(), own()).is_ok(),
        "the control builds"
    );
    assert!(home.join(".tmp").is_dir());
}

/// **A ruled limit above the gate's own hard limit refuses at the raise**:
/// the child cannot raise a hard limit, so every spawn would fail. A child
/// lowers its own hard `RLIMIT_NOFILE` below the ruled 64 and the judgment
/// refuses. The control: the same judgment with the limit untouched passes.
///
/// Perturbation: let `rlimits_within_reach` answer `Ok` always and the
/// lowered child passes.
#[test]
fn a_ruled_limit_above_the_gates_hard_limit_refuses() {
    let judged = |lower: bool| -> bool {
        // SAFETY: the child makes raw system calls only and exits.
        match unsafe { nix::unistd::fork() }.unwrap() {
            nix::unistd::ForkResult::Child => unsafe {
                if lower {
                    let limit = libc::rlimit {
                        rlim_cur: 32,
                        rlim_max: 32,
                    };
                    if libc::setrlimit(libc::RLIMIT_NOFILE, &limit) != 0 {
                        libc::_exit(2);
                    }
                }
                libc::_exit(if rlimits_within_reach().is_ok() { 0 } else { 1 });
            },
            nix::unistd::ForkResult::Parent { child } => {
                match nix::sys::wait::waitpid(child, None).unwrap() {
                    nix::sys::wait::WaitStatus::Exited(_, 0) => true,
                    nix::sys::wait::WaitStatus::Exited(_, 1) => false,
                    other => panic!("the judging child ended {other:?}"),
                }
            }
        }
    };
    assert!(!judged(true), "a hard NOFILE of 32 refuses the ruled 64");
    assert!(judged(false), "the control: the suite's own limits admit");
}

/// **Writes land beneath home alone, and the control writes elsewhere** (as
/// M7): a write to a directory beside home is refused, and with the home's
/// rights added there it lands.
#[test]
#[ignore = "runs the static busybox: needs WEAVER_BUSYBOX and Landlock ABI 6"]
fn writes_land_beneath_home_alone() {
    let layout = Layout::new("writes");
    let elsewhere = layout.base.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let probe = format!("echo x > {}/f 2>/dev/null; echo rc=$?", elsewhere.display());
    let refused = run(&layout.shell(), &probe);
    assert!(stdout(&refused).contains("rc=1"), "{}", said(&refused));
    let granted = layout.shell();
    add_right(&granted, &elsewhere, FS_ALL_V5 & !FS_EXECUTE);
    let control = run(&granted, &probe);
    assert!(stdout(&control).contains("rc=0"), "{}", said(&control));
}

/// **The rlimits bind, soft and hard, and the control sets each at its hard
/// value** (as M14): the shell reads every limit as set, a raise of each past
/// its hard limit is refused, each set to its hard value succeeds, and
/// `cat /dev/zero` into home stops at `SHELL_RLIMIT_FSIZE` killed by
/// `SIGXFSZ`.
#[test]
#[ignore = "runs the static busybox: needs WEAVER_BUSYBOX and Landlock ABI 6"]
fn the_rlimits_bind() {
    let layout = Layout::new("rlimits");
    let shell = layout.shell();
    let blocks = SHELL_RLIMIT_FSIZE / 512;
    let kib = SHELL_RLIMIT_AS >> 10;
    let nofile = SHELL_RLIMIT_NOFILE;
    let read = run(
        &shell,
        "echo f=$(ulimit -Sf)/$(ulimit -Hf) n=$(ulimit -Sn)/$(ulimit -Hn) \
         v=$(ulimit -Sv)/$(ulimit -Hv) c=$(ulimit -Sc)/$(ulimit -Hc)",
    );
    assert!(
        stdout(&read).contains(&format!(
            "f={blocks}/{blocks} n={nofile}/{nofile} v={kib}/{kib} c=0/0"
        )),
        "{}",
        said(&read)
    );
    for (flag, hard) in [("f", blocks), ("n", nofile), ("v", kib), ("c", 0)] {
        let past = run(
            &shell,
            &format!("ulimit -{flag} {} 2>/dev/null; echo rc=$?", hard + 1),
        );
        assert!(
            !stdout(&past).contains("rc=0"),
            "-{flag} raised: {}",
            said(&past)
        );
        let at = run(&shell, &format!("ulimit -{flag} {hard}; echo rc=$?"));
        assert!(
            stdout(&at).contains("rc=0"),
            "-{flag} at its hard value: {}",
            said(&at)
        );
    }
    let fsize = run(&shell, "cat /dev/zero > big; echo rc=$?; wc -c < big");
    let text = stdout(&fsize);
    assert!(text.contains("rc=153"), "{}", said(&fsize));
    assert!(
        text.contains(&SHELL_RLIMIT_FSIZE.to_string()),
        "{}",
        said(&fsize)
    );
}

/// What a child met on the two arms the socket probe does not reach: an x32
/// `getpid` (bit 30 set) and an i386 `getpid` through `int 0x80`, each the
/// raw return (`-errno` or a pid), under `apply` or under no-new-privs alone.
fn architecture_probe(ruleset: Option<RawFd>) -> (i64, i64) {
    let (read, write) = nix::unistd::pipe().unwrap();
    // SAFETY: the child makes raw system calls only and exits.
    match unsafe { nix::unistd::fork() }.unwrap() {
        nix::unistd::ForkResult::Child => unsafe {
            let applied = match ruleset {
                Some(ruleset) => apply(ruleset, SHELL_FILTER.as_ptr() as usize).is_ok(),
                None => libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) == 0,
            };
            if !applied {
                libc::_exit(2);
            }
            let x32 = libc::syscall(X32_SYSCALL_BIT as libc::c_long | libc::SYS_getpid);
            // errno read at once, before any other call.
            let x32 = if x32 < 0 {
                -(*libc::__errno_location() as i64)
            } else {
                x32
            };
            // i386's getpid is 20, reached through the 32-bit gate. The
            // kernel answers in rax, `-errno` on a refusal.
            let i386: i64;
            std::arch::asm!(
                "int 0x80",
                inlateout("rax") 20i64 => i386,
                out("r8") _, out("r9") _, out("r10") _, out("r11") _,
                options(nostack),
            );
            let mut bytes = [0u8; 16];
            bytes[..8].copy_from_slice(&x32.to_le_bytes());
            bytes[8..].copy_from_slice(&i386.to_le_bytes());
            libc::write(write.as_raw_fd(), bytes.as_ptr().cast(), 16);
            libc::_exit(0);
        },
        nix::unistd::ForkResult::Parent { child } => {
            drop(write);
            let status = nix::sys::wait::waitpid(child, None).unwrap();
            assert_eq!(
                status,
                nix::sys::wait::WaitStatus::Exited(child, 0),
                "the probe ran"
            );
            let mut bytes = [0u8; 16];
            assert_eq!(nix::unistd::read(&read, &mut bytes).unwrap(), 16);
            (
                i64::from_le_bytes(bytes[..8].try_into().unwrap()),
                i64::from_le_bytes(bytes[8..].try_into().unwrap()),
            )
        }
    }
}

/// **The production filter refuses an x32 call and a non-native
/// architecture's call `EACCES`, and the controls do not** (section 8.1's
/// sockets row, the grade of #176, F3): under [`apply`], an x32 `getpid`
/// and an i386 `getpid` through `int 0x80` both answer `-EACCES`. With
/// no-new-privs alone, the x32 call answers the kernel's own `-ENOSYS` and
/// the i386 call answers a pid, or `-ENOSYS` where the kernel runs no 32-bit
/// emulation, never `-EACCES`.
///
/// Perturbation: delete the filter's x32 arm, or its architecture check, and
/// the matching case answers the kernel's word, not `-EACCES`.
#[test]
#[ignore = "a measurement of this box's kernel: needs Landlock ABI 6"]
fn the_filter_refuses_x32_and_another_architecture() {
    let home = ScratchHome::new("arch");
    let shell = home.shell();
    let ruleset = shell.layers.as_ref().unwrap().ruleset.as_raw_fd();
    let eacces = -(libc::EACCES as i64);
    let (x32, i386) = architecture_probe(Some(ruleset));
    assert_eq!(x32, eacces, "x32 refused by the filter");
    assert_eq!(i386, eacces, "i386 refused by the filter");
    let (x32, i386) = architecture_probe(None);
    assert_eq!(
        x32,
        -(libc::ENOSYS as i64),
        "the control: the kernel's own x32 answer"
    );
    assert_ne!(
        i386, eacces,
        "the control: the kernel's own i386 answer, {i386}"
    );
}

/// **The home must be the gate's own directory below `/`** (the grade of
/// #176, F1): `/` would grant every right but execute on the whole
/// filesystem. `/`, a relative path, a symlink to a good home and a
/// directory owned by root each refuse `Home`. The control: the scratch home
/// builds.
///
/// Perturbation: drop `judge_home` and `/` builds.
#[test]
#[ignore = "a measurement of this box's kernel and a built busybox: needs Landlock ABI 6 and WEAVER_BUSYBOX"]
fn the_home_must_be_the_gates_own_directory() {
    let layout = Layout::new("home");
    let home = layout.base.join("home");
    let link = layout.base.join("home-link");
    std::os::unix::fs::symlink(&home, &link).unwrap();
    for bad in [
        PathBuf::from("/"),
        PathBuf::from("relative-home"),
        link,
        PathBuf::from("/var/empty"),
    ] {
        let built = Shell::confined_in(&layout.grant(), bad.clone(), own());
        assert!(
            matches!(built, Err(ConfineFault::Home(_))),
            "{}: {:?}",
            bad.display(),
            built.err()
        );
    }
    assert!(
        Shell::confined_in(&layout.grant(), home, own()).is_ok(),
        "the control builds"
    );
}

/// **The granted paths are judged on the descriptors the rules use** (the
/// grade of #176, F1): a directory, or a symlink, in the binary's place
/// refuses `Binary` (a directory would grant execute beneath it), and a
/// `commands/` that is a file, a symlink, or owned by another uid refuses
/// `Commands`. The control: the layout's own grant builds.
///
/// Perturbation: open without `O_NOFOLLOW`, or drop the file-type check, and
/// the matching case builds.
#[test]
#[ignore = "a measurement of this box's kernel and a built busybox: needs Landlock ABI 6 and WEAVER_BUSYBOX"]
fn the_granted_paths_are_judged_on_their_descriptors() {
    let layout = Layout::new("paths");
    let home = layout.base.join("home");
    let good = layout.grant();
    let binary_link = layout.base.join("busybox-link");
    std::os::unix::fs::symlink(&good.binary, &binary_link).unwrap();
    for binary in [layout.base.join("libexec"), binary_link] {
        let grant = ShellGrant {
            binary: binary.clone(),
            commands: good.commands.clone(),
        };
        let built = Shell::confined_in(&grant, home.clone(), own());
        assert!(
            matches!(built, Err(ConfineFault::Binary(_))),
            "{}: {:?}",
            binary.display(),
            built.err()
        );
    }
    let commands_link = layout.base.join("commands-link");
    std::os::unix::fs::symlink(&good.commands, &commands_link).unwrap();
    for commands in [
        good.binary.clone(),
        commands_link,
        PathBuf::from("/var/empty"),
    ] {
        let grant = ShellGrant {
            binary: good.binary.clone(),
            commands: commands.clone(),
        };
        let built = Shell::confined_in(&grant, home.clone(), own());
        assert!(
            matches!(built, Err(ConfineFault::Commands(_))),
            "{}: {:?}",
            commands.display(),
            built.err()
        );
    }
    assert!(
        Shell::confined_in(&good, home, own()).is_ok(),
        "the control builds"
    );
}

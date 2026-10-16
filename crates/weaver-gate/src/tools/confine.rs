//! The shell's confinement (#29, `weaver-gate-Spec` section 8.1): the agent's
//! shell secured as any untrusted user is on a controlled box, by the kernel's
//! unprivileged primitives alone.
//!
//! **Built in the parent, applied by the child to itself.** At the raise,
//! where the instruction carries the shell's grant, this module reads the
//! Landlock ABI (refusing below [`LANDLOCK_ABI_FLOOR`] and where the query
//! fails), makes home's `.tmp`, builds the Landlock ruleset as an owned
//! descriptor and the seccomp program as a fixed array. Each execution's
//! child then applies the layers in `pre_exec`, after its move into `tools/`
//! and before its exec, on those values alone, so it allocates nothing:
//! no-new-privs first (both filters need it), the rlimits, the socket filter,
//! and `landlock_restrict_self`. Any of them failing ends the child before its
//! exec, the spawn failing, answered `Errored` with nothing run.
//!
//! **The three Landlock calls go by number** through `nix`'s re-exported
//! `libc`, the one stated exception to "no raw `libc`" (section 1): `nix` 0.31
//! has no Landlock, and `libc` carries the numbers but not
//! `linux/landlock.h`, whose structures are stated here.

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use nix::libc;
use weaver_types::ShellGrant;

/// The lowest Landlock ABI the shell is offered on: 6 brings signal and
/// abstract-socket scoping, with TCP rules (4) and `TRUNCATE` (3) beneath it.
pub const LANDLOCK_ABI_FLOOR: i64 = 6;

/// The shell's rlimits, soft and hard alike, so no command raises its own
/// (the operator's ruling of 2026-10-16 on #29, measured as M14).
pub const SHELL_RLIMIT_FSIZE: u64 = 64 << 20;
pub const SHELL_RLIMIT_NOFILE: u64 = 64;
pub const SHELL_RLIMIT_AS: u64 = 512 << 20;
pub const SHELL_RLIMIT_CORE: u64 = 0;

/// The rlimits in the order the child sets them.
const RLIMITS: [(libc::__rlimit_resource_t, u64); 4] = [
    (libc::RLIMIT_FSIZE, SHELL_RLIMIT_FSIZE),
    (libc::RLIMIT_NOFILE, SHELL_RLIMIT_NOFILE),
    (libc::RLIMIT_AS, SHELL_RLIMIT_AS),
    (libc::RLIMIT_CORE, SHELL_RLIMIT_CORE),
];

// ------------------------------------------------------------ linux/landlock.h

const CREATE_RULESET_VERSION: u32 = 1 << 0;
const RULE_PATH_BENEATH: u32 = 1;

const FS_EXECUTE: u64 = 1 << 0;
const FS_WRITE_FILE: u64 = 1 << 1;
const FS_READ_FILE: u64 = 1 << 2;
const FS_READ_DIR: u64 = 1 << 3;
/// Every filesystem right through ABI 5: `REFER` (2), `TRUNCATE` (3) and
/// `IOCTL_DEV` (5) beside ABI 1's.
const FS_ALL_V5: u64 = (1 << 16) - 1;
/// Pathname UNIX socket lookups, `connect` and an addressed `sendmsg`: ABI 9.
const FS_RESOLVE_UNIX: u64 = 1 << 16;

const NET_BIND_TCP: u64 = 1 << 0;
const NET_CONNECT_TCP: u64 = 1 << 1;

const SCOPE_ABSTRACT_UNIX_SOCKET: u64 = 1 << 0;
const SCOPE_SIGNAL: u64 = 1 << 1;

#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
    handled_access_net: u64,
    scoped: u64,
}

#[repr(C, packed)]
struct PathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

// ------------------------------------------------------------ the seccomp filter

#[repr(C)]
#[derive(Clone, Copy)]
struct SockFilter {
    code: u16,
    jt: u8,
    jf: u8,
    k: u32,
}

#[repr(C)]
struct SockFprog {
    len: u16,
    filter: *const SockFilter,
}

const BPF_LD_W_ABS: u16 = 0x20;
const BPF_JEQ_K: u16 = 0x15;
const BPF_JGE_K: u16 = 0x35;
const BPF_RET_K: u16 = 0x06;
const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;
const AUDIT_ARCH_X86_64: u32 = 0xc000_003e;
const X32_SYSCALL_BIT: u32 = 0x4000_0000;
const DENY: u32 = SECCOMP_RET_ERRNO | libc::EACCES as u32;

const fn op(code: u16, jt: u8, jf: u8, k: u32) -> SockFilter {
    SockFilter { code, jt, jf, k }
}

/// **The socket filter** (section 8.1's sockets row): any architecture but
/// x86-64 refused whole, every x32 system call refused (`nr` with bit 30
/// set, x32 sharing the architecture word), `io_uring_setup` refused (its
/// `IORING_OP_SOCKET` makes a socket without `socket`), and `socket` and
/// `socketpair` refused `EACCES` for every family. The shell makes no socket.
static SOCKET_FILTER: [SockFilter; 11] = [
    // seccomp_data: nr at 0, arch at 4.
    op(BPF_LD_W_ABS, 0, 0, 4),
    op(BPF_JEQ_K, 1, 0, AUDIT_ARCH_X86_64),
    op(BPF_RET_K, 0, 0, DENY),
    op(BPF_LD_W_ABS, 0, 0, 0),
    op(BPF_JGE_K, 0, 1, X32_SYSCALL_BIT),
    op(BPF_RET_K, 0, 0, DENY),
    op(BPF_JEQ_K, 3, 0, libc::SYS_io_uring_setup as u32),
    op(BPF_JEQ_K, 2, 0, libc::SYS_socket as u32),
    op(BPF_JEQ_K, 1, 0, libc::SYS_socketpair as u32),
    op(BPF_RET_K, 0, 0, SECCOMP_RET_ALLOW),
    op(BPF_RET_K, 0, 0, DENY),
];

// ------------------------------------------------------------ the confinement

/// Why the raise refuses the shell `BoundaryUnverified`, nothing offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfineFault {
    /// The Landlock ABI is below the floor, or the query failed (Landlock
    /// absent from the kernel or disabled at boot answers as below it).
    AbiBelowFloor(Option<i64>),
    /// This uid has no home in the account database.
    NoHome,
    /// Home's `.tmp` could not be made.
    TmpDir(String),
    /// The ruleset or one of its rules could not be built.
    Ruleset(String),
}

impl std::fmt::Display for ConfineFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfineFault::AbiBelowFloor(Some(abi)) => {
                write!(
                    f,
                    "Landlock ABI {abi} is below the floor of {LANDLOCK_ABI_FLOOR}"
                )
            }
            ConfineFault::AbiBelowFloor(None) => write!(f, "the Landlock ABI query failed"),
            ConfineFault::NoHome => write!(f, "this uid has no home in the account database"),
            ConfineFault::TmpDir(detail) => write!(f, "home's .tmp: {detail}"),
            ConfineFault::Ruleset(detail) => write!(f, "the Landlock ruleset: {detail}"),
        }
    }
}

/// The Landlock ABI this kernel reports, or the errno of the query.
pub fn landlock_abi() -> Result<i64, i32> {
    // SAFETY: the version query takes no attribute and creates nothing.
    let version = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<RulesetAttr>(),
            0usize,
            CREATE_RULESET_VERSION,
        )
    };
    if version < 0 {
        Err(nix::errno::Errno::last_raw())
    } else {
        Ok(version)
    }
}

/// **The floor's judgment of an ABI query** (section 8.1): a failed query
/// answers as one below the floor, so nothing runs unconfined on a kernel
/// that lacks Landlock.
fn abi_at_floor(query: Result<i64, i32>) -> Result<i64, ConfineFault> {
    match query {
        Ok(abi) if abi >= LANDLOCK_ABI_FLOOR => Ok(abi),
        Ok(abi) => Err(ConfineFault::AbiBelowFloor(Some(abi))),
        Err(_) => Err(ConfineFault::AbiBelowFloor(None)),
    }
}

/// The shell this gate runs: the granted busybox under its layers.
pub struct Shell {
    program: PathBuf,
    argv0: &'static str,
    home: PathBuf,
    /// `PATH` and `TMPDIR` where confined. The test-only bare shell sets
    /// neither, as before #29.
    path: Option<PathBuf>,
    tmpdir: Option<PathBuf>,
    layers: Option<Layers>,
}

/// The values the child's `pre_exec` reads: built before the fork, held for
/// the gate's life.
struct Layers {
    ruleset: OwnedFd,
}

impl Shell {
    /// **The confined shell, built at the raise from the grant** (section
    /// 8.1): the ABI read and refused below the floor, home's `.tmp` made as
    /// the agent's uid, and the ruleset built and owned.
    pub fn confined(grant: &ShellGrant) -> Result<Shell, ConfineFault> {
        // The home comes from the account database rather than from the
        // environment: the gate inherits the agent's environment, which may
        // or may not carry `HOME`, and the account database is where the
        // uid's home is a fact rather than an inheritance.
        let home = nix::unistd::User::from_uid(nix::unistd::getuid())
            .ok()
            .flatten()
            .map(|user| user.dir)
            .ok_or(ConfineFault::NoHome)?;
        Shell::confined_in(grant, home)
    }

    /// [`Shell::confined`] beneath a named home: the production path, the
    /// suite naming a scratch home where the gate names the uid's own.
    fn confined_in(grant: &ShellGrant, home: PathBuf) -> Result<Shell, ConfineFault> {
        let abi = abi_at_floor(landlock_abi())?;
        let tmpdir = home.join(".tmp");
        match std::fs::create_dir(&tmpdir) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(ConfineFault::TmpDir(error.to_string())),
        }
        let ruleset = build_ruleset(abi, grant, &home)?;
        Ok(Shell {
            program: grant.binary.clone(),
            argv0: "sh",
            home,
            path: Some(grant.commands.clone()),
            tmpdir: Some(tmpdir),
            layers: Some(Layers { ruleset }),
        })
    }

    /// **A bare shell for the suite's supervision tests alone**: `program`
    /// run as itself with no layers. It does not compile outside `cfg(test)`,
    /// so the production binary has no unconfined path. The clock, the cancel
    /// and the containment are program-neutral, and their fixtures need what
    /// the confined shell does not hold.
    #[cfg(test)]
    pub(crate) fn bare(program: &str) -> Shell {
        let home = nix::unistd::User::from_uid(nix::unistd::getuid())
            .ok()
            .flatten()
            .map(|user| user.dir)
            .expect("the suite's uid has a home");
        Shell {
            program: PathBuf::from(program),
            argv0: "bash",
            home,
            path: None,
            tmpdir: None,
            layers: None,
        }
    }

    /// The command for one execution, `-c` and the call's command, with the
    /// gate's environment and the home as its directory. Its layers are not
    /// yet installed: the caller registers its own `pre_exec` first (the move
    /// into `tools/`), then calls [`Shell::install_layers`].
    pub fn command(&self, script: &str) -> Command {
        let mut command = Command::new(&self.program);
        command
            .arg0(self.argv0)
            .arg("-c")
            .arg(script)
            .current_dir(&self.home)
            // **The command starts with the gate's environment and nothing
            // else**: `HOME`, and where confined `PATH` the commands
            // directory and `TMPDIR` home's `.tmp`. The operator's ruling of
            // 2026-10-01 reaches organs, not the commands a tool runs.
            .env_clear()
            .env("HOME", &self.home);
        if let Some(path) = &self.path {
            command.env("PATH", path);
        }
        if let Some(tmpdir) = &self.tmpdir {
            command.env("TMPDIR", tmpdir);
        }
        command
    }

    /// **The child's layers, in `pre_exec`**, in section 8.1's order:
    /// no-new-privs, the rlimits, the socket filter, the Landlock
    /// restriction. Each is an async-signal-safe call on a value read here,
    /// before the fork, and the first failure ends the child before its exec.
    pub fn install_layers(&self, command: &mut Command) {
        let Some(layers) = &self.layers else {
            return;
        };
        let ruleset: RawFd = layers.ruleset.as_raw_fd();
        // The program lives in a `static`, so its address outlives every
        // fork. An address, not a pointer, keeps the closure `Send`.
        let filter = SOCKET_FILTER.as_ptr() as usize;
        // SAFETY: `apply` makes only async-signal-safe calls on values read
        // before the fork, the child allocating nothing.
        unsafe {
            command.pre_exec(move || apply(ruleset, filter));
        }
    }
}

/// **The layers, applied by the child to itself**, in section 8.1's order:
/// no-new-privs (both filters need it), the rlimits soft and hard alike, the
/// socket filter, the Landlock restriction. The first failure answers its
/// errno, ending the child before its exec.
///
/// # Safety
///
/// Called in a forked child before its exec, with `ruleset` an open Landlock
/// ruleset and `filter` the address of [`SOCKET_FILTER`].
unsafe fn apply(ruleset: RawFd, filter: usize) -> std::io::Result<()> {
    // SAFETY: prctl, setrlimit and a raw system call, each async-signal-safe,
    // on values the caller read before the fork.
    unsafe {
        if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        for (resource, value) in RLIMITS {
            let limit = libc::rlimit {
                rlim_cur: value,
                rlim_max: value,
            };
            if libc::setrlimit(resource, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        let prog = SockFprog {
            len: SOCKET_FILTER.len() as u16,
            filter: filter as *const SockFilter,
        };
        if libc::prctl(
            libc::PR_SET_SECCOMP,
            libc::SECCOMP_MODE_FILTER,
            &prog as *const SockFprog,
            0,
            0,
        ) != 0
        {
            return Err(std::io::Error::last_os_error());
        }
        if libc::syscall(libc::SYS_landlock_restrict_self, ruleset, 0u32) != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// **The wall's ruleset** (section 8.1): every filesystem right through ABI
/// 5 handled, with `RESOLVE_UNIX` where the kernel offers it, TCP connect and
/// bind handled, and signal and abstract-socket scoping. Granted: execute and
/// read on the one busybox file, read on the commands directory, every right
/// but execute beneath home, read and write on `/dev/null`, and read on
/// `/dev/zero` and `/dev/urandom`. Nothing else, and no TCP port.
fn build_ruleset(abi: i64, grant: &ShellGrant, home: &Path) -> Result<OwnedFd, ConfineFault> {
    let handled_fs = if abi >= 9 {
        FS_ALL_V5 | FS_RESOLVE_UNIX
    } else {
        FS_ALL_V5
    };
    let attr = RulesetAttr {
        handled_access_fs: handled_fs,
        handled_access_net: NET_CONNECT_TCP | NET_BIND_TCP,
        scoped: SCOPE_SIGNAL | SCOPE_ABSTRACT_UNIX_SOCKET,
    };
    // SAFETY: the attribute is a live `repr(C)` value of the size passed.
    let fd = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            &attr as *const RulesetAttr,
            std::mem::size_of::<RulesetAttr>(),
            0u32,
        )
    };
    if fd < 0 {
        let errno = nix::errno::Errno::last();
        return Err(ConfineFault::Ruleset(format!("create: {errno}")));
    }
    // SAFETY: a fresh descriptor the kernel returned, owned from here.
    let ruleset = unsafe { OwnedFd::from_raw_fd(fd as RawFd) };
    let rules: [(&Path, u64); 6] = [
        (&grant.binary, FS_EXECUTE | FS_READ_FILE),
        (&grant.commands, FS_READ_DIR),
        (home, FS_ALL_V5 & !FS_EXECUTE),
        (Path::new("/dev/null"), FS_READ_FILE | FS_WRITE_FILE),
        (Path::new("/dev/zero"), FS_READ_FILE),
        (Path::new("/dev/urandom"), FS_READ_FILE),
    ];
    for (path, access) in rules {
        allow(&ruleset, path, access)?;
    }
    Ok(ruleset)
}

/// One path-beneath rule, the path opened `O_PATH` for the call alone.
fn allow(ruleset: &OwnedFd, path: &Path, access: u64) -> Result<(), ConfineFault> {
    let parent = nix::fcntl::open(
        path,
        nix::fcntl::OFlag::O_PATH | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )
    .map_err(|errno| ConfineFault::Ruleset(format!("open {}: {errno}", path.display())))?;
    let rule = PathBeneathAttr {
        allowed_access: access,
        parent_fd: parent.as_raw_fd(),
    };
    // SAFETY: a live `repr(C, packed)` attribute and two owned descriptors.
    let added = unsafe {
        libc::syscall(
            libc::SYS_landlock_add_rule,
            ruleset.as_raw_fd(),
            RULE_PATH_BENEATH,
            &rule as *const PathBeneathAttr,
            0u32,
        )
    };
    if added != 0 {
        let errno = nix::errno::Errno::last();
        return Err(ConfineFault::Ruleset(format!(
            "rule {}: {errno}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests;

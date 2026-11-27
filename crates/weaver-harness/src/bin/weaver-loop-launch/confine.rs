//! The launcher's walls, per `weaver-harness-Spec` section 2.5: the kernel's
//! unprivileged primitives, laid by the launcher on itself before it execs the
//! interpreter, in the shell's order: no-new-privs, the limits, the filter,
//! then Landlock.
//!
//! **A copy, not a link.** The Landlock and seccomp structures, the ABI query
//! and its floor, `allow`, `open_judged` and `add_rule` are copied as written
//! from `weaver-gate`'s `src/tools/confine.rs`. The gate does not change, and
//! this binary takes no edge to it: the harness links no organ. `apply`, the
//! limits' reach and the ruleset's creation are written again for the loop's
//! grant. The filter is the loop's own. Making the two copies one waits for
//! the operator's rethink of the repository's layout.
//!
//! **The three Landlock calls go by number** through `nix`'s re-exported
//! `libc`: `nix` 0.31 has no Landlock, and `libc` carries the numbers but not
//! `linux/landlock.h`, whose structures are stated here.

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::Path;

use nix::libc;

// **The seccomp filter names x86-64's architecture word and refuses every
// other**, as the shell's does, so a build for another architecture would
// offer a loop whose every call its own filter ends. That build fails here.
#[cfg(not(target_arch = "x86_64"))]
compile_error!("the loop's seccomp filter is written for x86-64 alone (weaver-harness-Spec 2.5)");

/// The lowest Landlock ABI the loop is started on: 6 brings signal and
/// abstract-socket scoping, with TCP rules (4) and `TRUNCATE` (3) beneath it.
/// The shell's floor.
pub const LANDLOCK_ABI_FLOOR: i64 = 6;

/// The loop's limits, soft and hard alike, so no wire file raises its own:
/// the shell's values unchanged (`weaver-gate` `confine.rs`), about ten times
/// the peak measured with the largest return.
pub const LOOP_RLIMIT_FSIZE: u64 = 64 << 20;
pub const LOOP_RLIMIT_NOFILE: u64 = 64;
pub const LOOP_RLIMIT_AS: u64 = 512 << 20;
pub const LOOP_RLIMIT_CORE: u64 = 0;

/// The limits in the order the launcher sets them.
const RLIMITS: [(libc::__rlimit_resource_t, u64); 4] = [
    (libc::RLIMIT_FSIZE, LOOP_RLIMIT_FSIZE),
    (libc::RLIMIT_NOFILE, LOOP_RLIMIT_NOFILE),
    (libc::RLIMIT_AS, LOOP_RLIMIT_AS),
    (libc::RLIMIT_CORE, LOOP_RLIMIT_CORE),
];

// ------------------------------------------------------------ linux/landlock.h

const CREATE_RULESET_VERSION: u32 = 1 << 0;
const RULE_PATH_BENEATH: u32 = 1;

pub const FS_EXECUTE: u64 = 1 << 0;
pub const FS_READ_FILE: u64 = 1 << 2;
pub const FS_READ_DIR: u64 = 1 << 3;
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
/// Every refusal of the filter answers `EACCES`, as the shell's do, so a
/// refusal reads as the filter's, and a loop does not take it for "try again".
const DENY: u32 = SECCOMP_RET_ERRNO | libc::EACCES as u32;

/// `seccomp_data`'s offsets: `nr` at 0, `arch` at 4.
const DATA_NR: u32 = 0;
const DATA_ARCH: u32 = 4;

const fn op(code: u16, jt: u8, jf: u8, k: u32) -> SockFilter {
    SockFilter { code, jt, jf, k }
}

/// **The loop's seccomp filter**, every refusal `EACCES`:
///
/// - any architecture but x86-64 refused whole, and every x32 system call
///   (`nr` with bit 30 set, x32 sharing the architecture word).
/// - `io_uring_setup` (its `IORING_OP_SOCKET` makes a socket without
///   `socket`), and `socket` and `socketpair` for every family, so the loop
///   makes no socket. Measured: the runner makes none of the three.
/// - `clone`, `fork`, `vfork` and `clone3`, so the loop makes no process and no
///   thread (the Planner's ruling 3). Measured: the runner serves with them
///   refused.
///
/// The shell's file-attribute rows are left out: they keep a shell's writes in
/// its home's quota, and the loop writes nothing. A jump goes to the next
/// instruction plus its offset, so every refusal row reaches the last.
static LOOP_FILTER: [SockFilter; 15] = [
    op(BPF_LD_W_ABS, 0, 0, DATA_ARCH),
    op(BPF_JEQ_K, 1, 0, AUDIT_ARCH_X86_64),
    op(BPF_RET_K, 0, 0, DENY),
    op(BPF_LD_W_ABS, 0, 0, DATA_NR),
    op(BPF_JGE_K, 0, 1, X32_SYSCALL_BIT),
    op(BPF_RET_K, 0, 0, DENY),
    op(BPF_JEQ_K, 7, 0, libc::SYS_io_uring_setup as u32),
    op(BPF_JEQ_K, 6, 0, libc::SYS_socket as u32),
    op(BPF_JEQ_K, 5, 0, libc::SYS_socketpair as u32),
    op(BPF_JEQ_K, 4, 0, libc::SYS_clone as u32),
    op(BPF_JEQ_K, 3, 0, libc::SYS_fork as u32),
    op(BPF_JEQ_K, 2, 0, libc::SYS_vfork as u32),
    op(BPF_JEQ_K, 1, 0, libc::SYS_clone3 as u32),
    op(BPF_RET_K, 0, 0, SECCOMP_RET_ALLOW),
    op(BPF_RET_K, 0, 0, DENY),
];

// ------------------------------------------------------------ the floor and the reach

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

/// **The floor's judgment of an ABI query**: a failed query answers as one
/// below the floor, so nothing runs unconfined on a kernel that lacks Landlock.
pub fn abi_at_floor(query: Result<i64, i32>) -> Result<i64, String> {
    match query {
        Ok(abi) if abi >= LANDLOCK_ABI_FLOOR => Ok(abi),
        Ok(abi) => Err(format!(
            "Landlock ABI {abi} is below the floor of {LANDLOCK_ABI_FLOOR}"
        )),
        Err(_) => Err("the Landlock ABI query failed".to_string()),
    }
}

/// **The limits must be within the launcher's own hard limits**: it lowers
/// each to the loop's value, soft and hard alike, and an unprivileged process
/// cannot raise a hard limit, so a lower one refuses here, naming it.
pub fn rlimits_within_reach() -> Result<(), String> {
    for (resource, value) in RLIMITS {
        let mut limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: getrlimit into a value this frame holds.
        if unsafe { libc::getrlimit(resource, &mut limit) } != 0 {
            let errno = nix::errno::Errno::last();
            return Err(format!("getrlimit: {errno}"));
        }
        if limit.rlim_max != libc::RLIM_INFINITY && limit.rlim_max < value {
            return Err(format!(
                "resource {resource}: the hard limit {} is below the loop's {value}",
                limit.rlim_max
            ));
        }
    }
    Ok(())
}

// ------------------------------------------------------------ the ruleset

/// One granted path, opened and judged, with the rights its rule grants.
pub struct Rule {
    pub fd: OwnedFd,
    pub access: u64,
    pub path: std::path::PathBuf,
}

/// **The loop's ruleset**: every filesystem right through ABI 5 handled, with
/// `RESOLVE_UNIX` where the kernel offers it, TCP connect and bind handled,
/// and signal and abstract-socket scoping, as the shell's. Granted: the rules
/// given, on the descriptors they were judged on, and nothing else. No write,
/// no `/dev` file and no TCP port.
pub fn build_ruleset(abi: i64, rules: &[Rule]) -> Result<OwnedFd, String> {
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
        return Err(format!("the Landlock ruleset: create: {errno}"));
    }
    // SAFETY: a fresh descriptor the kernel returned, owned from here.
    let ruleset = unsafe { OwnedFd::from_raw_fd(fd as RawFd) };
    for rule in rules {
        add_rule(&ruleset, &rule.fd, rule.access, &rule.path)?;
    }
    Ok(ruleset)
}

/// `path` opened `O_PATH` without following a link at its last component,
/// and judged by `fstat` on that descriptor: `fault` answers why it is not
/// what the rule may take, or nothing. Copied as written from the shell's.
pub fn open_judged(
    path: &Path,
    fault: impl Fn(&libc::stat) -> Option<String>,
) -> Result<OwnedFd, String> {
    let fd = nix::fcntl::open(
        path,
        nix::fcntl::OFlag::O_PATH | nix::fcntl::OFlag::O_NOFOLLOW | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )
    .map_err(|errno| format!("{} does not open: {errno}", path.display()))?;
    let stat = nix::sys::stat::fstat(&fd)
        .map_err(|errno| format!("{}: fstat: {errno}", path.display()))?;
    if stat.st_mode & libc::S_IFMT == libc::S_IFLNK {
        return Err(format!("{} is a link", path.display()));
    }
    match fault(&stat) {
        Some(why) => Err(format!("{} {why}", path.display())),
        None => Ok(fd),
    }
}

/// One path-beneath rule on an open descriptor, `path` naming it in a fault.
/// Copied as written from the shell's.
fn add_rule(ruleset: &OwnedFd, parent: &OwnedFd, access: u64, path: &Path) -> Result<(), String> {
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
        return Err(format!(
            "the Landlock ruleset: rule {}: {errno}",
            path.display()
        ));
    }
    Ok(())
}

// ------------------------------------------------------------ the layers

/// **The walls, laid by the launcher on itself**, in the shell's order:
/// no-new-privs (both filters need it), the limits soft and hard alike, the
/// seccomp filter, the Landlock restriction. The first failure answers its
/// errno, and the launcher ends before its exec. One thread holds the process,
/// so the filter and the domain cover all of it.
pub fn apply(ruleset: &OwnedFd) -> Result<(), String> {
    // SAFETY: prctl, setrlimit and a raw system call on values this frame
    // holds. The filter lives in a `static`, so its address is valid through
    // the call.
    unsafe {
        if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
            return Err(format!("no-new-privs: {}", std::io::Error::last_os_error()));
        }
        for (resource, value) in RLIMITS {
            let limit = libc::rlimit {
                rlim_cur: value,
                rlim_max: value,
            };
            if libc::setrlimit(resource, &limit) != 0 {
                return Err(format!(
                    "the limit {resource}: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }
        let prog = SockFprog {
            len: LOOP_FILTER.len() as u16,
            filter: LOOP_FILTER.as_ptr(),
        };
        if libc::prctl(
            libc::PR_SET_SECCOMP,
            libc::SECCOMP_MODE_FILTER,
            &prog as *const SockFprog,
            0,
            0,
        ) != 0
        {
            return Err(format!(
                "the seccomp filter: {}",
                std::io::Error::last_os_error()
            ));
        }
        if libc::syscall(libc::SYS_landlock_restrict_self, ruleset.as_raw_fd(), 0u32) != 0 {
            return Err(format!(
                "the Landlock restriction: {}",
                std::io::Error::last_os_error()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

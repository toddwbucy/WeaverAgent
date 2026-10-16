//! **Landlock on this box, measured before any Spec text** (#29, the operator's
//! rulings of 2026-10-15: the shell tool locked down as any untrusted user is,
//! Landlock the wall for the command list). Each instrument confines a child by a
//! ruleset the parent builds, the child's `pre_exec` setting `PR_SET_NO_NEW_PRIVS`
//! and restricting itself, then execs copies of real commands from a scratch
//! command directory. They are `#[ignore]`: they read the kernel this suite runs
//! on, and print what they measure (run with `--nocapture`).
//!
//! The uapi's structs and constants are stated here: `libc` 0.2.189 carries the
//! three system call numbers and none of `linux/landlock.h`.

use std::ffi::CString;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use nix::libc;

const CREATE_RULESET_VERSION: u32 = 1 << 0;
const RULE_PATH_BENEATH: u32 = 1;
const RULE_NET_PORT: u32 = 2;

const FS_EXECUTE: u64 = 1 << 0;
const FS_WRITE_FILE: u64 = 1 << 1;
const FS_READ_FILE: u64 = 1 << 2;
const FS_READ_DIR: u64 = 1 << 3;
const FS_ALL_V1: u64 = (1 << 13) - 1;

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

#[repr(C)]
struct NetPortAttr {
    allowed_access: u64,
    port: u64,
}

/// The Landlock ABI version the kernel reports, or the errno it refused with.
fn abi() -> Result<i64, i32> {
    // SAFETY: a query by number with a null attribute and the version flag.
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

/// A ruleset handling `fs`, `net` and `scoped`, built in the parent.
struct Ruleset(OwnedFd);

impl Ruleset {
    fn new(fs: u64, net: u64, scoped: u64) -> Self {
        let attr = RulesetAttr {
            handled_access_fs: fs,
            handled_access_net: net,
            scoped,
        };
        // SAFETY: the attribute is a live repr(C) value of the size passed.
        let fd = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                &attr as *const RulesetAttr,
                std::mem::size_of::<RulesetAttr>(),
                0u32,
            )
        };
        assert!(
            fd >= 0,
            "landlock_create_ruleset: {}",
            nix::errno::Errno::last()
        );
        // SAFETY: a fresh descriptor the kernel returned.
        Ruleset(unsafe { OwnedFd::from_raw_fd(fd as RawFd) })
    }

    /// Allows `access` beneath `path`.
    fn allow(self, path: &Path, access: u64) -> Self {
        let dir = std::fs::File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let rule = PathBeneathAttr {
            allowed_access: access,
            parent_fd: dir.as_raw_fd(),
        };
        // SAFETY: the rule is a live packed value naming an open descriptor.
        let added = unsafe {
            libc::syscall(
                libc::SYS_landlock_add_rule,
                self.0.as_raw_fd(),
                RULE_PATH_BENEATH,
                &rule as *const PathBeneathAttr,
                0u32,
            )
        };
        assert_eq!(
            added,
            0,
            "landlock_add_rule {}: {}",
            path.display(),
            nix::errno::Errno::last()
        );
        self
    }

    /// Allows connecting to TCP `port`.
    fn allow_connect(self, port: u16) -> Self {
        let rule = NetPortAttr {
            allowed_access: NET_CONNECT_TCP,
            port: u64::from(port),
        };
        // SAFETY: as above.
        let added = unsafe {
            libc::syscall(
                libc::SYS_landlock_add_rule,
                self.0.as_raw_fd(),
                RULE_NET_PORT,
                &rule as *const NetPortAttr,
                0u32,
            )
        };
        assert_eq!(
            added,
            0,
            "landlock_add_rule port: {}",
            nix::errno::Errno::last()
        );
        self
    }

    /// Runs `command` confined: `PR_SET_NO_NEW_PRIVS`, then
    /// `landlock_restrict_self`, in the child before its exec.
    fn run(&self, command: &mut Command) -> Output {
        self.run_filtered(command, None)
    }

    /// [`Ruleset::run`] with a seccomp filter loaded first where one is given,
    /// as section 8.1's child loads its socket filter.
    fn run_filtered(&self, command: &mut Command, filter: Option<&[SockFilter]>) -> Output {
        let ruleset = self.0.as_raw_fd();
        let prog = filter.map(|filter| SockFprog {
            len: filter.len() as u16,
            filter: filter.as_ptr(),
        });
        // An address, not a pointer, so the closure is `Send`: `prog` outlives
        // the spawn, which returns only once the child has exec'd.
        let prog_addr = prog
            .as_ref()
            .map_or(0usize, |prog| prog as *const SockFprog as usize);
        // SAFETY: prctl and a raw system call on a descriptor opened before the
        // fork, both async-signal-safe.
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if prog_addr != 0
                    && libc::prctl(
                        libc::PR_SET_SECCOMP,
                        libc::SECCOMP_MODE_FILTER,
                        prog_addr as *const SockFprog,
                        0,
                        0,
                    ) != 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::syscall(libc::SYS_landlock_restrict_self, ruleset, 0u32) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        // The environment is cleared where the caller set none, and kept as
        // the caller set it otherwise (the shell's `HOME`, `PATH`, `TMPDIR`).
        if command.get_envs().next().is_none() {
            command.env_clear();
        }
        // **A spawn that meets `ETXTBSY` is retried, bounded** (Codex on
        // #175): a parallel test's fork can hold the write descriptor of a
        // binary this test just copied until that fork execs, and an exec of
        // the copy meanwhile fails "Text file busy". The retry is the one site
        // every instrument's spawn passes, a second or so in all.
        let mut attempts = 0;
        let spawned = loop {
            match command.stdin(Stdio::null()).output() {
                Err(error) if error.raw_os_error() == Some(libc::ETXTBSY) && attempts < 100 => {
                    attempts += 1;
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                other => break other,
            }
        };
        match spawned {
            Ok(output) => output,
            // The spawn's own failure, a `pre_exec` step or the exec, is a
            // measurement too: it rides the output as status None.
            Err(error) => Output {
                status: std::os::unix::process::ExitStatusExt::from_raw(0x7f00),
                stdout: Vec::new(),
                stderr: format!("spawn failed: {error}").into_bytes(),
            },
        }
    }
}

/// A scratch command directory holding copies of `commands` from `/usr/bin`.
struct CommandDir(PathBuf);

impl CommandDir {
    fn new(tag: &str, commands: &[&str]) -> Self {
        let dir =
            std::env::temp_dir().join(format!("weaver-landlock-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in commands {
            std::fs::copy(Path::new("/usr/bin").join(name), dir.join(name)).unwrap();
        }
        Self(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for CommandDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn said(output: &Output) -> String {
    format!(
        "status {:?}, stdout {:?}, stderr {:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).trim(),
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

/// The child ran and exited 0.
fn assert_ran(what: &str, output: &Output) {
    assert_eq!(output.status.code(), Some(0), "{what}: {}", said(output));
}

/// The exec itself was refused: the spawn failed `EACCES`, nothing ran.
fn assert_exec_denied(what: &str, output: &Output) {
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(
        err.contains("spawn failed: Permission denied"),
        "{what}: {}",
        said(output)
    );
}

/// `needle` in the output's stdout (`out`) or stderr.
fn assert_says(what: &str, output: &Output, needle: &str) {
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        all.contains(needle),
        "{what}: expected {needle:?} in {}",
        said(output)
    );
}

/// The read-only roots a dynamically linked command maps: the interpreter, the
/// shared libraries, and the files libc reads at start.
const LIBRARY_ROOTS: [&str; 3] = ["/usr/lib", "/usr/lib64", "/etc"];

/// The ELF interpreter, by its real path (`/lib64` and `/usr/lib64` resolve to
/// `/usr/lib` on this box).
const INTERPRETER: &str = "/usr/lib/ld-linux-x86-64.so.2";

/// The command directory executable and readable, the library roots readable,
/// and the interpreter alone executable among them (M2e's finding).
fn standard(ruleset: Ruleset, bin: &CommandDir) -> Ruleset {
    let mut rules = ruleset.allow(&bin.0, FS_EXECUTE | FS_READ_FILE | FS_READ_DIR);
    for root in LIBRARY_ROOTS {
        rules = rules.allow(Path::new(root), FS_READ_FILE | FS_READ_DIR);
    }
    rules.allow(Path::new(INTERPRETER), FS_EXECUTE | FS_READ_FILE)
}

/// **M1: the ABI.** Landlock is enabled and reports its version. Signal and
/// abstract-socket scoping need 6, TCP rules 4, `REFER` 2, `TRUNCATE` 3.
#[test]
#[ignore = "a measurement of this box's kernel"]
fn m1_the_landlock_abi() {
    let version = abi().expect("Landlock is enabled");
    println!("M1 Landlock ABI version: {version}");
    assert!(version >= 6, "the floor is 6");
    assert!(version >= 1);
}

/// **M2: what a dynamically linked command needs.** With `EXECUTE` handled
/// and granted only on the command directory, does the exec of a copied
/// `ls` succeed where the libraries and interpreter are readable but not
/// executable, and fail where they are not even readable?
#[test]
#[ignore = "a measurement of this box's kernel"]
fn m2_execute_and_the_interpreter() {
    let bin = CommandDir::new("m2", &["ls"]);
    let handled = FS_EXECUTE | FS_READ_FILE | FS_READ_DIR;
    // a: the libraries readable, not executable.
    let mut readable = Ruleset::new(handled, 0, 0).allow(&bin.0, FS_EXECUTE | FS_READ_FILE);
    for root in LIBRARY_ROOTS {
        readable = readable.allow(Path::new(root), FS_READ_FILE | FS_READ_DIR);
    }
    let a = readable.run(Command::new(bin.path("ls")).arg("/"));
    println!("M2a libraries read-only: {}", said(&a));
    assert_exec_denied("M2a", &a);
    // b: the libraries neither readable nor executable.
    let bare = Ruleset::new(handled, 0, 0).allow(&bin.0, FS_EXECUTE | FS_READ_FILE);
    let b = bare.run(Command::new(bin.path("ls")).arg("/"));
    println!("M2b libraries unreadable: {}", said(&b));
    assert_exec_denied("M2b", &b);
    // c: the libraries readable and executable.
    let mut full = Ruleset::new(handled, 0, 0).allow(&bin.0, FS_EXECUTE | FS_READ_FILE);
    for root in LIBRARY_ROOTS {
        full = full.allow(Path::new(root), FS_EXECUTE | FS_READ_FILE | FS_READ_DIR);
    }
    let c = full.run(Command::new(bin.path("ls")).arg("/"));
    println!("M2c libraries executable: {}", said(&c));
    assert_says("M2c: the exec ran, / unread", &c, "cannot open directory");
    // d: /usr/bin/ls itself, outside the command directory, libraries readable.
    let mut outside = Ruleset::new(handled, 0, 0).allow(&bin.0, FS_EXECUTE | FS_READ_FILE);
    for root in LIBRARY_ROOTS {
        outside = outside.allow(Path::new(root), FS_READ_FILE | FS_READ_DIR);
    }
    let d = outside.run(Command::new("/usr/bin/ls").arg("/"));
    println!("M2d /usr/bin/ls outside the directory: {}", said(&d));
    assert_exec_denied("M2d", &d);
    // e: the interpreter alone executable, the libraries read-only, `/` readable.
    let e = standard(Ruleset::new(handled, 0, 0), &bin)
        .allow(Path::new("/"), FS_READ_DIR)
        .run(Command::new(bin.path("ls")).arg("/"));
    println!("M2e the interpreter alone executable: {}", said(&e));
    assert_ran("M2e", &e);
    // f: the interpreter not executable, everything else as e.
    let mut f = Ruleset::new(handled, 0, 0).allow(&bin.0, FS_EXECUTE | FS_READ_FILE);
    for root in LIBRARY_ROOTS {
        f = f.allow(Path::new(root), FS_READ_FILE | FS_READ_DIR);
    }
    let f = f
        .allow(Path::new("/"), FS_READ_DIR)
        .run(Command::new(bin.path("ls")).arg("/"));
    println!("M2f the interpreter read-only: {}", said(&f));
    assert_exec_denied("M2f", &f);
}

/// **M3: a symlink in the command directory.** Landlock judges the file the
/// path resolves to, so a symlink to `/usr/bin/ls` is expected to be denied:
/// the directory must hold copies (or same-filesystem hard links).
#[test]
#[ignore = "a measurement of this box's kernel"]
fn m3_a_symlink_is_judged_at_its_target() {
    let bin = CommandDir::new("m3", &[]);
    std::os::unix::fs::symlink("/usr/bin/ls", bin.path("ls")).unwrap();
    let handled = FS_EXECUTE | FS_READ_FILE | FS_READ_DIR;
    let out = standard(Ruleset::new(handled, 0, 0), &bin)
        .run(Command::new(bin.path("ls")).arg(bin.0.to_str().unwrap()));
    println!("M3 symlink to /usr/bin/ls: {}", said(&out));
    assert_exec_denied("M3", &out);
}

/// **M4: a breakout-capable command can still start only listed programs.**
/// `find -exec`, `awk`'s `system()` and `bash` itself, confined to the
/// directory, try `/usr/bin/id`, which is not in it: each must fail with a
/// permission error, and `id` listed must run.
#[test]
#[ignore = "a measurement of this box's kernel"]
fn m4_breakout_commands_start_only_listed_programs() {
    let bin = CommandDir::new("m4", &["find", "awk", "bash", "id"]);
    let handled = FS_EXECUTE | FS_READ_FILE | FS_READ_DIR;
    let rules = || standard(Ruleset::new(handled, 0, 0), &bin);
    let find = rules().run(Command::new(bin.path("find")).args([
        bin.0.to_str().unwrap(),
        "-maxdepth",
        "0",
        "-exec",
        "/usr/bin/id",
        ";",
    ]));
    println!("M4a find -exec /usr/bin/id: {}", said(&find));
    assert_says("M4a", &find, "Permission denied");
    assert!(
        !String::from_utf8_lossy(&find.stdout).contains("uid="),
        "M4a ran id"
    );
    let awk = rules().run(
        Command::new(bin.path("awk"))
            .args(["BEGIN { r = system(\"/usr/bin/id\"); print \"system returned \" r }"]),
    );
    println!("M4b awk system(/usr/bin/id): {}", said(&awk));
    assert_says("M4b", &awk, "system returned 126");
    let bash = rules().run(Command::new(bin.path("bash")).args(["-c", "/usr/bin/id"]));
    println!("M4c bash -c /usr/bin/id: {}", said(&bash));
    assert_eq!(bash.status.code(), Some(126), "M4c: {}", said(&bash));
    let listed = rules()
        .run(Command::new(bin.path("bash")).args(["-c", &format!("{}", bin.path("id").display())]));
    println!("M4d bash -c <listed id>: {}", said(&listed));
    assert_says("M4d", &listed, "uid=");
}

/// **M5: TCP connect denial.** With `CONNECT_TCP` handled and no port allowed,
/// `bash`'s `/dev/tcp` connect to a local listener fails, and with that port
/// allowed, it connects.
#[test]
#[ignore = "a measurement of this box's kernel"]
fn m5_tcp_connect_is_denied_unless_elected() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let bin = CommandDir::new("m5", &["bash"]);
    let handled = FS_EXECUTE | FS_READ_FILE | FS_READ_DIR;
    let rules = |net: Option<u16>| {
        let rules = standard(Ruleset::new(handled, NET_CONNECT_TCP, 0), &bin);
        match net {
            Some(port) => rules.allow_connect(port),
            None => rules,
        }
    };
    let script = format!("exec 3<>/dev/tcp/127.0.0.1/{port} && echo connected");
    let denied = rules(None).run(Command::new(bin.path("bash")).args(["-c", &script]));
    println!("M5a connect, no port elected: {}", said(&denied));
    assert_says("M5a", &denied, "Permission denied");
    let allowed = rules(Some(port)).run(Command::new(bin.path("bash")).args(["-c", &script]));
    println!("M5b connect, the port elected: {}", said(&allowed));
    assert_says("M5b", &allowed, "connected");
    drop(listener);
}

/// A free loopback TCP port, found by binding port 0 and released at once.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Binds a TCP socket to `port` on the loopback from a child confined by
/// `rules` alone, then listens, answering which step stopped it.
fn landlock_bind_probe(rules: &Ruleset, port: u16) -> String {
    let ruleset = rules.0.as_raw_fd();
    // SAFETY: the child makes raw system calls only and exits.
    match unsafe { nix::unistd::fork() }.unwrap() {
        nix::unistd::ForkResult::Child => unsafe {
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0
                || libc::syscall(libc::SYS_landlock_restrict_self, ruleset, 0u32) != 0
            {
                libc::_exit(250);
            }
            let fd = libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0);
            if fd < 0 {
                libc::_exit(249);
            }
            let mut addr: libc::sockaddr_in = std::mem::zeroed();
            addr.sin_family = libc::AF_INET as libc::sa_family_t;
            addr.sin_port = port.to_be();
            addr.sin_addr.s_addr = u32::from_be_bytes([127, 0, 0, 1]).to_be();
            if libc::bind(
                fd,
                &addr as *const _ as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
            ) != 0
            {
                libc::_exit((*libc::__errno_location()).min(99));
            }
            if libc::listen(fd, 1) != 0 {
                libc::_exit(100 + (*libc::__errno_location()).min(99));
            }
            libc::_exit(0);
        },
        nix::unistd::ForkResult::Parent { child } => {
            match nix::sys::wait::waitpid(child, None).unwrap() {
                nix::sys::wait::WaitStatus::Exited(_, 0) => "listening".into(),
                nix::sys::wait::WaitStatus::Exited(_, 250) => "the restriction failed".into(),
                nix::sys::wait::WaitStatus::Exited(_, 249) => "no socket".into(),
                nix::sys::wait::WaitStatus::Exited(_, code) if code >= 100 => {
                    format!(
                        "bound, listen refused: {}",
                        nix::errno::Errno::from_raw(code - 100)
                    )
                }
                nix::sys::wait::WaitStatus::Exited(_, errno) => {
                    format!("bind refused: {}", nix::errno::Errno::from_raw(errno))
                }
                other => format!("{other:?}"),
            }
        }
    }
}

/// **M5b: the shell never binds, so never listens** (Codex on #175). With
/// `CONNECT_TCP` and `BIND_TCP` handled and connect allowed on one port alone,
/// as section 8.1's wall stands where a port is elected, a `bind` on the
/// elected port, on another port and on port 0 (an ephemeral one) is refused
/// `EACCES`, so `listen` is never reached. Without the seccomp filter, which
/// admits inet stream sockets where a port is elected, so the wall alone
/// answers. Run with `BIND_TCP` unhandled, the same binds succeed and listen.
#[test]
#[ignore = "a measurement of this box's kernel"]
fn m5b_bind_is_refused_on_every_port() {
    let elected = free_port();
    let other = free_port();
    let rules =
        || Ruleset::new(FS_READ_FILE, NET_CONNECT_TCP | NET_BIND_TCP, 0).allow_connect(elected);
    for (what, port) in [
        ("the elected port", elected),
        ("another port", other),
        ("port 0", 0),
    ] {
        let bound = landlock_bind_probe(&rules(), port);
        println!("M5b bind on {what} ({port}): {bound}");
        assert_eq!(
            bound, "bind refused: EACCES: Permission denied",
            "M5b bind on {what}"
        );
    }
}

/// **M6: signal and abstract-socket scoping.** With `SCOPE_SIGNAL`, a confined
/// `bash` cannot signal a process outside its domain (`kill -0`), and can
/// signal its own child. With `SCOPE_ABSTRACT_UNIX_SOCKET` it cannot connect
/// to an abstract socket bound outside.
#[test]
#[ignore = "a measurement of this box's kernel"]
fn m6_signal_and_abstract_socket_scoping() {
    let version = abi().expect("Landlock is enabled");
    if version < 6 {
        println!("M6 scoping needs ABI 6, this kernel reports {version}");
        return;
    }
    let mut outside = Command::new("/usr/bin/sleep").arg("30").spawn().unwrap();
    let bin = CommandDir::new("m6", &["bash", "sleep"]);
    let handled = FS_EXECUTE | FS_READ_FILE | FS_READ_DIR;
    let rules = standard(
        Ruleset::new(handled, 0, SCOPE_SIGNAL | SCOPE_ABSTRACT_UNIX_SOCKET),
        &bin,
    );
    let script = format!(
        "kill -0 {} 2>&1; echo outside=$?; {} 30 & kill -0 $! 2>&1; echo own_child=$?; kill $!",
        outside.id(),
        bin.path("sleep").display()
    );
    let out = rules.run(Command::new(bin.path("bash")).args(["-c", &script]));
    println!("M6a signal scoping: {}", said(&out));
    assert_says("M6a", &out, "outside=1");
    assert_says("M6a", &out, "own_child=0");
    let _ = outside.kill();
    let _ = outside.wait();
    // The abstract socket: bound here, outside the domain.
    let name = format!("weaver-landlock-m6-{}", std::process::id());
    let addr = nix::sys::socket::UnixAddr::new_abstract(name.as_bytes()).unwrap();
    let server = nix::sys::socket::socket(
        nix::sys::socket::AddressFamily::Unix,
        nix::sys::socket::SockType::Stream,
        nix::sys::socket::SockFlag::SOCK_CLOEXEC,
        None,
    )
    .unwrap();
    nix::sys::socket::bind(server.as_raw_fd(), &addr).unwrap();
    nix::sys::socket::listen(&server, nix::sys::socket::Backlog::new(1).unwrap()).unwrap();
    let probe = abstract_connect_probe(&rules_for_abstract(&bin), &name);
    println!("M6b abstract socket connect from inside: {probe}");
    assert!(probe.contains("EPERM"), "M6b: {probe}");
}

fn rules_for_abstract(bin: &CommandDir) -> Ruleset {
    standard(
        Ruleset::new(
            FS_EXECUTE | FS_READ_FILE | FS_READ_DIR,
            0,
            SCOPE_ABSTRACT_UNIX_SOCKET,
        ),
        bin,
    )
}

/// Connects to the abstract socket `name` from a child confined by `rules`,
/// the child being this test binary's own fork, and answers what it met.
fn abstract_connect_probe(rules: &Ruleset, name: &str) -> String {
    let ruleset = rules.0.as_raw_fd();
    let name = CString::new(name).unwrap();
    // SAFETY: the child makes raw system calls only and exits.
    match unsafe { nix::unistd::fork() }.unwrap() {
        nix::unistd::ForkResult::Child => unsafe {
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0
                || libc::syscall(libc::SYS_landlock_restrict_self, ruleset, 0u32) != 0
            {
                libc::_exit(90);
            }
            let fd = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0);
            let mut addr: libc::sockaddr_un = std::mem::zeroed();
            addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
            let bytes = name.as_bytes();
            for (i, b) in bytes.iter().enumerate() {
                addr.sun_path[i + 1] = *b as libc::c_char;
            }
            let len =
                (std::mem::size_of::<libc::sa_family_t>() + 1 + bytes.len()) as libc::socklen_t;
            let r = libc::connect(fd, &addr as *const _ as *const libc::sockaddr, len);
            let errno = if r == 0 { 0 } else { *libc::__errno_location() };
            libc::_exit(errno.min(89));
        },
        nix::unistd::ForkResult::Parent { child } => {
            match nix::sys::wait::waitpid(child, None).unwrap() {
                nix::sys::wait::WaitStatus::Exited(_, 0) => "connected".to_string(),
                nix::sys::wait::WaitStatus::Exited(_, 90) => "the restriction failed".to_string(),
                nix::sys::wait::WaitStatus::Exited(_, errno) => {
                    format!("refused: {}", nix::errno::Errno::from_raw(errno))
                }
                other => format!("{other:?}"),
            }
        }
    }
}

/// **M7: writes beneath home alone, and the full v1 set handled.** Every
/// filesystem right of ABI 1 handled, writes granted beneath a scratch home
/// only: a write there succeeds and one in another directory is denied.
#[test]
#[ignore = "a measurement of this box's kernel"]
fn m7_writes_beneath_home_alone() {
    let bin = CommandDir::new("m7", &["bash"]);
    let home = std::env::temp_dir().join(format!("weaver-landlock-home-{}", std::process::id()));
    let elsewhere =
        std::env::temp_dir().join(format!("weaver-landlock-else-{}", std::process::id()));
    for dir in [&home, &elsewhere] {
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).unwrap();
    }
    let rules = standard(Ruleset::new(FS_ALL_V1, 0, 0), &bin).allow(&home, FS_ALL_V1 & !FS_EXECUTE);
    let script = format!(
        "echo a > {}/in-home && echo home=ok; echo b > {}/elsewhere 2>&1; echo elsewhere=$?",
        home.display(),
        elsewhere.display()
    );
    let out = rules.run(Command::new(bin.path("bash")).args(["-c", &script]));
    println!("M7 writes: {}", said(&out));
    assert_says("M7", &out, "home=ok");
    assert_says("M7", &out, "elsewhere=1");
    let _ = FS_WRITE_FILE;
    for dir in [&home, &elsewhere] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// **M3b: a hard link in the command directory.** Landlock judges the path
/// walked, so a same-filesystem hard link reached through the directory runs,
/// and the same inode reached by its other path does not. Measured on
/// `/var/tmp` with a file this suite owns (`protected_hardlinks` refuses a
/// link to a file of another owner, which admin, as root, would not meet).
#[test]
#[ignore = "a measurement of this box's kernel"]
fn m3b_a_hard_link_is_judged_by_the_path_walked() {
    let base = PathBuf::from(format!(
        "/var/tmp/weaver-landlock-m3b-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("bin")).unwrap();
    std::fs::create_dir_all(base.join("source")).unwrap();
    std::fs::copy("/usr/bin/id", base.join("source/id")).unwrap();
    std::fs::hard_link(base.join("source/id"), base.join("bin/id")).unwrap();
    let bin = CommandDir(base.join("bin"));
    let handled = FS_EXECUTE | FS_READ_FILE | FS_READ_DIR;
    let through =
        standard(Ruleset::new(handled, 0, 0), &bin).run(&mut Command::new(base.join("bin/id")));
    println!("M3b through the directory: {}", said(&through));
    assert_ran("M3b through", &through);
    let other =
        standard(Ruleset::new(handled, 0, 0), &bin).run(&mut Command::new(base.join("source/id")));
    println!("M3b by its other path: {}", said(&other));
    assert_exec_denied("M3b other", &other);
    std::mem::forget(bin);
    let _ = std::fs::remove_dir_all(&base);
}

/// **M8: the interpreter as a program.** The interpreter must be executable
/// for any dynamically linked command (M2e, M2f), and run directly it loads
/// whatever readable ELF it is handed by `mmap`, which Landlock does not
/// check. An ELF placed in the writable home, not executable and not listed,
/// is run that way? Then the command list does not hold for a dynamically
/// linked set, and the remedy is measured beside it: a statically linked
/// command needs no interpreter, so none is granted execute.
#[test]
#[ignore = "a measurement of this box's kernel"]
fn m8_the_interpreter_as_a_program() {
    let bin = CommandDir::new("m8", &["bash"]);
    let home = std::env::temp_dir().join(format!("weaver-landlock-m8home-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    // The agent could write these bytes itself, and the suite places them.
    std::fs::copy("/usr/bin/id", home.join("payload")).unwrap();
    std::fs::set_permissions(
        home.join("payload"),
        std::os::unix::fs::PermissionsExt::from_mode(0o644),
    )
    .unwrap();
    let handled = FS_EXECUTE | FS_READ_FILE | FS_READ_DIR;
    let rules =
        standard(Ruleset::new(handled, 0, 0), &bin).allow(&home, FS_READ_FILE | FS_READ_DIR);
    let direct = format!("{}/payload; echo direct=$?", home.display());
    let via = format!(
        "{INTERPRETER} {}/payload; echo via_interpreter=$?",
        home.display()
    );
    let out = rules.run(Command::new(bin.path("bash")).args(["-c", &format!("{direct}; {via}")]));
    println!("M8 an unlisted ELF in home: {}", said(&out));
    // The hole is measured open: what the static set and the noexec home close.
    assert_says("M8 direct", &out, "direct=126");
    assert_says("M8 via the interpreter", &out, "via_interpreter=0");
    let _ = std::fs::remove_dir_all(&home);
}

/// **M8b: a statically linked command set needs no interpreter execute.** A
/// static launcher in the command directory runs with the interpreter not
/// granted execute, and handed the interpreter and the payload it can start
/// neither: the interpreter-as-program route is closed. `docker-init` is the
/// static binary at hand on this box, a launcher that execs its argument.
#[test]
#[ignore = "a measurement of this box's kernel"]
fn m8b_a_static_set_grants_no_interpreter() {
    let bin = CommandDir::new("m8b", &["docker-init"]);
    let home = std::env::temp_dir().join(format!("weaver-landlock-m8bhome-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    std::fs::copy("/usr/bin/id", home.join("payload")).unwrap();
    let handled = FS_EXECUTE | FS_READ_FILE | FS_READ_DIR;
    let rules = || {
        let mut rules =
            Ruleset::new(handled, 0, 0).allow(&bin.0, FS_EXECUTE | FS_READ_FILE | FS_READ_DIR);
        for root in LIBRARY_ROOTS {
            rules = rules.allow(Path::new(root), FS_READ_FILE | FS_READ_DIR);
        }
        rules.allow(&home, FS_READ_FILE | FS_READ_DIR)
    };
    let alone = rules().run(Command::new(bin.path("docker-init")).arg("--version"));
    println!("M8b the static launcher alone: {}", said(&alone));
    assert_ran("M8b alone", &alone);
    let via = rules().run(Command::new(bin.path("docker-init")).args([
        "-s",
        "--",
        INTERPRETER,
        home.join("payload").to_str().unwrap(),
    ]));
    println!(
        "M8b launching the interpreter on the payload: {}",
        said(&via)
    );
    assert_eq!(via.status.code(), Some(126), "M8b via: {}", said(&via));
    let _ = std::fs::remove_dir_all(&home);
}

/// **M8c: a `noexec` home closes the route for a dynamic set.** Run under
/// `unshare --map-root-user --mount`: a tmpfs mounted `noexec` stands for
/// the agent's home, and the interpreter, executable as a dynamic set needs,
/// cannot map the payload there. Fails naming the namespace when not in one.
#[test]
#[ignore = "a measurement of this box's kernel; run under unshare --map-root-user --mount"]
fn m8c_a_noexec_home_closes_the_interpreter_route() {
    PrivateNamespace::check();
    let bin = CommandDir::new("m8c", &["bash"]);
    let home = std::env::temp_dir().join(format!("weaver-landlock-m8chome-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let target = CString::new(home.to_str().unwrap()).unwrap();
    // SAFETY: a mount by C strings this frame holds, inside a mount namespace.
    let mounted = unsafe {
        libc::mount(
            c"none".as_ptr(),
            target.as_ptr(),
            c"tmpfs".as_ptr(),
            libc::MS_NOEXEC | libc::MS_NOSUID | libc::MS_NODEV,
            std::ptr::null(),
        )
    };
    assert_eq!(mounted, 0, "mount: {}", nix::errno::Errno::last());
    // Unmounted on every exit path, a failed assertion's unwind among them.
    let _mounted = Unmount(target);
    std::fs::copy("/usr/bin/id", home.join("payload")).unwrap();
    let handled = FS_EXECUTE | FS_READ_FILE | FS_READ_DIR;
    let rules =
        standard(Ruleset::new(handled, 0, 0), &bin).allow(&home, FS_READ_FILE | FS_READ_DIR);
    let via = format!(
        "{INTERPRETER} {}/payload; echo via_interpreter=$?",
        home.display()
    );
    let out = rules.run(Command::new(bin.path("bash")).args(["-c", &via]));
    println!("M8c the interpreter on a noexec home: {}", said(&out));
    assert_says("M8c", &out, "failed to map segment");
    assert_says("M8c", &out, "via_interpreter=127");
}

/// **The private user and mount namespace M8c's mount needs**, verified
/// rather than assumed from euid 0. The uid map must not be the identity
/// (`0 0 4294967295`, the initial namespace): it maps this process's 0 to
/// another uid outside, which proves a user namespace. The mount the test then
/// makes succeeding proves a mount namespace that user namespace owns: an
/// account unprivileged outside can mount only there, never in the initial
/// one. The parent process is not compared with: run under `unshare ... cargo
/// test`, the parent is cargo, already inside the same namespaces.
struct PrivateNamespace;

impl PrivateNamespace {
    fn check() {
        assert!(
            nix::unistd::geteuid().is_root(),
            "run under `unshare --map-root-user --mount`"
        );
        let map = std::fs::read_to_string("/proc/self/uid_map").expect("the uid map reads");
        let first: Vec<u64> = map
            .split_whitespace()
            .take(3)
            .map(|n| n.parse().unwrap())
            .collect();
        assert!(
            first != [0, 0, 4_294_967_295] && first.get(1).is_some_and(|outside| *outside != 0),
            "a private user namespace mapping 0 to another uid, not {map:?}"
        );
    }
}

/// A tmpfs this test mounted, detached when dropped on every exit path.
struct Unmount(CString);

impl Drop for Unmount {
    fn drop(&mut self) {
        // SAFETY: an unmount by a C string this value holds.
        unsafe {
            libc::umount2(self.0.as_ptr(), libc::MNT_DETACH);
        }
    }
}

/// **M8d: a library root readable wholesale holds programs too.** With the
/// interpreter executable and `/usr/lib` readable as a root, an ELF program
/// that lives under `/usr/lib` (not in the command directory) runs through
/// the interpreter: a dynamic set must grant reads on its libraries' closure
/// file by file, not on the root, or be statically linked.
#[test]
#[ignore = "a measurement of this box's kernel"]
fn m8d_a_readable_library_root_holds_programs() {
    // A regular ELF program under `/usr/lib`, not a link into `/usr/bin` (a
    // symlink is judged at its target, M3).
    let program = [
        "/usr/lib/git-core/git-credential-libsecret",
        "/usr/lib/systemd/systemd",
    ]
    .into_iter()
    .find(|p| Path::new(p).is_file() && !Path::new(p).is_symlink())
    .expect("an ELF program under /usr/lib on this box");
    let bin = CommandDir::new("m8d", &["bash"]);
    let handled = FS_EXECUTE | FS_READ_FILE | FS_READ_DIR;
    let rules = standard(Ruleset::new(handled, 0, 0), &bin);
    let via = format!("{INTERPRETER} {program} --version; echo via_interpreter=$?");
    let out = rules.run(Command::new(bin.path("bash")).args(["-c", &via]));
    println!("M8d {program} through the interpreter: {}", said(&out));
    // A hole measured open, closed only by the static set or a per-file closure.
    assert_says("M8d", &out, "via_interpreter=0");
}

// ------------------------------------------------------------- M9: seccomp

const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;
const AUDIT_ARCH_X86_64: u32 = 0xc000_003e;
const BPF_LD_W_ABS: u16 = 0x20;
const BPF_JEQ_K: u16 = 0x15;
const BPF_RET_K: u16 = 0x06;
const BPF_ALU_AND_K: u16 = 0x54;
const BPF_JGE_K: u16 = 0x35;
const X32_SYSCALL_BIT: u32 = 0x4000_0000;

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

const fn op(code: u16, jt: u8, jf: u8, k: u32) -> SockFilter {
    SockFilter { code, jt, jf, k }
}

/// **The socket-family filter** the shell applies to itself under
/// no-new-privs (#29, the operator's answer 4). Any architecture but x86-64
/// is refused whole, and so is every x32 system call (`nr` with bit 30 set),
/// x32 sharing the architecture word. `io_uring_setup` is refused, io_uring's
/// `IORING_OP_SOCKET` being a route to a socket that never calls `socket`.
/// `socket` and `socketpair` are refused for `AF_UNIX` (Codex on #175: below
/// Landlock ABI 9 nothing bounds a pathname socket's connect), and allowed for
/// `AF_INET` and `AF_INET6` only as `SOCK_STREAM` with protocol 0 or
/// `IPPROTO_TCP`, and only where `tcp` (a TCP port is elected): a pinned protocol keeps MPTCP and
/// SCTP stream sockets out. Everything else that makes a socket is refused
/// `EACCES`.
fn socket_filter(tcp: bool) -> Vec<SockFilter> {
    let deny = SECCOMP_RET_ERRNO | libc::EACCES as u32;
    let inet = if tcp { SECCOMP_RET_ALLOW } else { deny };
    // seccomp_data: nr at 0, arch at 4, args[i] at 16 + 8 i (the low word).
    vec![
        /* 0 */ op(BPF_LD_W_ABS, 0, 0, 4),
        /* 1 */ op(BPF_JEQ_K, 1, 0, AUDIT_ARCH_X86_64),
        /* 2 */ op(BPF_RET_K, 0, 0, deny),
        /* 3 */ op(BPF_LD_W_ABS, 0, 0, 0),
        /* 4 */ op(BPF_JGE_K, 0, 1, X32_SYSCALL_BIT),
        /* 5 */ op(BPF_RET_K, 0, 0, deny),
        /* 6 */ op(BPF_JEQ_K, 0, 1, libc::SYS_io_uring_setup as u32),
        /* 7 */ op(BPF_RET_K, 0, 0, deny),
        /* 8 */ op(BPF_JEQ_K, 2, 0, libc::SYS_socket as u32),
        /* 9 */ op(BPF_JEQ_K, 1, 0, libc::SYS_socketpair as u32),
        /* 10 */ op(BPF_RET_K, 0, 0, SECCOMP_RET_ALLOW),
        /* 11 */ op(BPF_LD_W_ABS, 0, 0, 16),
        /* 12 */ op(BPF_JEQ_K, 0, 1, libc::AF_UNIX as u32),
        /* 13 */ op(BPF_RET_K, 0, 0, deny),
        /* 14 */ op(BPF_JEQ_K, 1, 0, libc::AF_INET as u32),
        /* 15 */ op(BPF_JEQ_K, 0, 7, libc::AF_INET6 as u32),
        /* 16 */ op(BPF_LD_W_ABS, 0, 0, 24),
        /* 17 */ op(BPF_ALU_AND_K, 0, 0, 0xf),
        /* 18 */ op(BPF_JEQ_K, 0, 4, libc::SOCK_STREAM as u32),
        /* 19 */ op(BPF_LD_W_ABS, 0, 0, 32),
        /* 20 */ op(BPF_JEQ_K, 1, 0, 0),
        /* 21 */ op(BPF_JEQ_K, 0, 1, libc::IPPROTO_TCP as u32),
        /* 22 */ op(BPF_RET_K, 0, 0, inet),
        /* 23 */ op(BPF_RET_K, 0, 0, deny),
    ]
}

/// Opens one socket of each kind from a child that set no-new-privs and
/// loaded `filter`, answering which opened.
fn socket_probe(filter: &[SockFilter]) -> String {
    let prog = SockFprog {
        len: filter.len() as u16,
        filter: filter.as_ptr(),
    };
    let kinds: [(i32, i32, i32, &str); 8] = [
        (libc::AF_UNIX, libc::SOCK_STREAM, 0, "unix stream"),
        (libc::AF_INET, libc::SOCK_STREAM, 0, "inet tcp"),
        (
            libc::AF_INET6,
            libc::SOCK_STREAM,
            libc::IPPROTO_TCP,
            "inet6 tcp",
        ),
        (libc::AF_INET, libc::SOCK_DGRAM, 0, "inet udp"),
        (libc::AF_INET, libc::SOCK_RAW, 0, "inet raw"),
        (libc::AF_NETLINK, libc::SOCK_RAW, 0, "netlink"),
        (
            libc::AF_INET,
            libc::SOCK_STREAM,
            libc::IPPROTO_MPTCP,
            "inet mptcp",
        ),
        (
            libc::AF_INET,
            libc::SOCK_STREAM,
            libc::IPPROTO_SCTP,
            "inet sctp",
        ),
    ];
    // The result crosses a pipe: an exit status keeps eight bits only.
    let (report_r, report_w) = nix::unistd::pipe().unwrap();
    let report = report_w.as_raw_fd();
    // SAFETY: the child makes raw system calls only and exits.
    match unsafe { nix::unistd::fork() }.unwrap() {
        nix::unistd::ForkResult::Child => unsafe {
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0
                || libc::prctl(
                    libc::PR_SET_SECCOMP,
                    libc::SECCOMP_MODE_FILTER,
                    &prog as *const SockFprog,
                    0,
                    0,
                ) != 0
            {
                libc::_exit(255);
            }
            let mut opened = 0;
            for (bit, (family, kind, protocol, _)) in kinds.iter().enumerate() {
                let fd = libc::socket(*family, *kind | libc::SOCK_CLOEXEC, *protocol);
                if fd >= 0 {
                    opened |= 1 << bit;
                }
            }
            // io_uring_setup with a zeroed params block: a ring opens, or not.
            let mut params = [0u8; 120];
            let ring = libc::syscall(libc::SYS_io_uring_setup, 4u32, params.as_mut_ptr());
            if ring >= 0 {
                opened |= 1 << 8;
            }
            // An x32 `socket` (bit 30 set): refused by the filter, or ENOSYS
            // where the kernel runs no x32.
            let x32 = libc::syscall(
                X32_SYSCALL_BIT as libc::c_long | libc::SYS_socket,
                libc::AF_UNIX,
                libc::SOCK_STREAM,
                0,
            );
            let mut pair = [0i32; 2];
            if libc::socketpair(
                libc::AF_UNIX,
                libc::SOCK_STREAM | libc::SOCK_CLOEXEC,
                0,
                pair.as_mut_ptr(),
            ) == 0
            {
                opened |= 1 << 11;
            }
            let x32_errno = if x32 < 0 {
                *libc::__errno_location()
            } else {
                0
            };
            if x32_errno == libc::EACCES {
                opened |= 1 << 9;
            } else if x32_errno == libc::ENOSYS {
                opened |= 1 << 10;
            }
            let bytes = (opened as u32).to_le_bytes();
            libc::write(report, bytes.as_ptr().cast(), 4);
            libc::_exit(0);
        },
        nix::unistd::ForkResult::Parent { child } => {
            drop(report_w);
            let mut bytes = [0u8; 4];
            let read = nix::unistd::read(&report_r, &mut bytes).unwrap_or(0);
            let opened = u32::from_le_bytes(bytes);
            match nix::sys::wait::waitpid(child, None).unwrap() {
                nix::sys::wait::WaitStatus::Exited(_, 255) => "the filter did not load".into(),
                nix::sys::wait::WaitStatus::Exited(_, 0) if read == 4 => kinds
                    .iter()
                    .enumerate()
                    .map(|(bit, (_, _, _, name))| {
                        format!(
                            "{name}={}",
                            if opened & (1 << bit) != 0 {
                                "open"
                            } else {
                                "refused"
                            }
                        )
                    })
                    .chain(std::iter::once(format!(
                        "io_uring={}",
                        if opened & (1 << 8) != 0 {
                            "open"
                        } else {
                            "refused"
                        }
                    )))
                    .chain(std::iter::once(format!(
                        "unix socketpair={}",
                        if opened & (1 << 11) != 0 {
                            "open"
                        } else {
                            "refused"
                        }
                    )))
                    .chain(std::iter::once(format!(
                        "x32={}",
                        if opened & (1 << 9) != 0 {
                            "refused by the filter (EACCES)"
                        } else if opened & (1 << 10) != 0 {
                            "ENOSYS"
                        } else {
                            "neither EACCES nor ENOSYS"
                        }
                    )))
                    .collect::<Vec<_>>()
                    .join(", "),
                other => format!("{other:?}"),
            }
        }
    }
}

/// **M9: the socket-family filter.** Unfiltered, every kind opens (the box
/// allows raw only to root, so `inet raw` is refused by DAC alone). With no
/// TCP port elected, only `AF_UNIX` opens. With one elected, TCP on both
/// inet families opens too, UDP, raw and netlink refused, MPTCP and SCTP
/// stream sockets refused by the protocol pin, `io_uring_setup` refused, and
/// an x32 call refused `EACCES` (where the kernel alone answers `ENOSYS`).
#[test]
#[ignore = "a measurement of this box's kernel"]
fn m9_the_socket_family_filter() {
    let unfiltered = socket_probe(&[op(BPF_RET_K, 0, 0, SECCOMP_RET_ALLOW)]);
    println!("M9 unfiltered: {unfiltered}");
    assert!(unfiltered.contains("unix stream=open"), "{unfiltered}");
    assert!(unfiltered.contains("io_uring=open"), "{unfiltered}");
    assert!(unfiltered.contains("x32=ENOSYS"), "{unfiltered}");
    for (tcp, label) in [(false, "no TCP elected"), (true, "a TCP port elected")] {
        let filtered = socket_probe(&socket_filter(tcp));
        println!("M9 {label}: {filtered}");
        for refused in [
            "unix stream=refused",
            "unix socketpair=refused",
            "inet udp=refused",
            "netlink=refused",
            "inet mptcp=refused",
            "inet sctp=refused",
            "io_uring=refused",
            "x32=refused by the filter",
        ] {
            assert!(
                filtered.contains(refused),
                "{label}: {refused} in {filtered}"
            );
        }
        let tcp_state = if tcp { "open" } else { "refused" };
        assert!(
            filtered.contains(&format!("inet tcp={tcp_state}")),
            "{label}: {filtered}"
        );
        assert!(
            filtered.contains(&format!("inet6 tcp={tcp_state}")),
            "{label}: {filtered}"
        );
    }
}

/// **M10: execute granted on one file, reached through a symlinked command
/// directory.** The stand-in for the stack's static multi-call binary is
/// `docker-init` (static). The commands directory holds a symlink to it, and
/// Landlock grants execute on that one file and read on the directory. Run
/// through the symlink it starts, since a symlink is judged at its target
/// (M3). An identical copy elsewhere is denied, the grant naming the file and
/// not its contents.
#[test]
#[ignore = "a measurement of this box's kernel"]
fn m10_execute_on_one_file_through_symlinks() {
    let base = std::env::temp_dir().join(format!("weaver-landlock-m10-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("libexec")).unwrap();
    std::fs::create_dir_all(base.join("commands")).unwrap();
    std::fs::create_dir_all(base.join("elsewhere")).unwrap();
    let multicall = base.join("libexec/multicall");
    std::fs::copy("/usr/bin/docker-init", &multicall).unwrap();
    std::fs::copy("/usr/bin/docker-init", base.join("elsewhere/multicall")).unwrap();
    std::os::unix::fs::symlink(&multicall, base.join("commands/tini")).unwrap();
    let handled = FS_EXECUTE | FS_READ_FILE | FS_READ_DIR;
    let rules = || {
        Ruleset::new(handled, 0, 0)
            .allow(&multicall, FS_EXECUTE | FS_READ_FILE)
            .allow(&base.join("commands"), FS_READ_DIR)
    };
    let through = rules().run(Command::new(base.join("commands/tini")).arg("--version"));
    println!("M10 through the symlink: {}", said(&through));
    assert_ran("M10 through", &through);
    let copy = rules().run(Command::new(base.join("elsewhere/multicall")).arg("--version"));
    println!("M10 an identical copy elsewhere: {}", said(&copy));
    assert_exec_denied("M10 copy", &copy);
    // The grant names one file and nothing else: the launcher, run through
    // its symlink, cannot start the interpreter, nor a copy of itself the
    // agent writes into its home (readable and writable to it).
    let home = base.join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::copy(&multicall, home.join("multicall")).unwrap();
    let home_rules = || rules().allow(&home, FS_READ_FILE | FS_READ_DIR);
    let interpreter = home_rules().run(Command::new(base.join("commands/tini")).args([
        "-s",
        "--",
        INTERPRETER,
        "--version",
    ]));
    println!(
        "M10 the launcher starting the interpreter: {}",
        said(&interpreter)
    );
    assert_eq!(
        interpreter.status.code(),
        Some(126),
        "M10 interpreter: {}",
        said(&interpreter)
    );
    let written = home_rules().run(Command::new(base.join("commands/tini")).args([
        "-s",
        "--",
        home.join("multicall").to_str().unwrap(),
        "--version",
    ]));
    println!(
        "M10 the launcher starting a copy written into home: {}",
        said(&written)
    );
    assert_eq!(
        written.status.code(),
        Some(126),
        "M10 written: {}",
        said(&written)
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// Connects a socket of `protocol` to `port` on the loopback from a child
/// confined by `rules` alone (no seccomp), answering what the connect met.
fn landlock_connect_probe(rules: &Ruleset, protocol: i32, port: u16) -> String {
    let ruleset = rules.0.as_raw_fd();
    // SAFETY: the child makes raw system calls only and exits.
    match unsafe { nix::unistd::fork() }.unwrap() {
        nix::unistd::ForkResult::Child => unsafe {
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0
                || libc::syscall(libc::SYS_landlock_restrict_self, ruleset, 0u32) != 0
            {
                libc::_exit(250);
            }
            let fd = libc::socket(libc::AF_INET, libc::SOCK_STREAM, protocol);
            if fd < 0 {
                libc::_exit(200 + (*libc::__errno_location()).min(49));
            }
            let mut addr: libc::sockaddr_in = std::mem::zeroed();
            addr.sin_family = libc::AF_INET as libc::sa_family_t;
            addr.sin_port = port.to_be();
            addr.sin_addr.s_addr = u32::from_be_bytes([127, 0, 0, 1]).to_be();
            let r = libc::connect(
                fd,
                &addr as *const _ as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
            );
            let errno = if r == 0 { 0 } else { *libc::__errno_location() };
            libc::_exit(errno.min(199));
        },
        nix::unistd::ForkResult::Parent { child } => {
            match nix::sys::wait::waitpid(child, None).unwrap() {
                nix::sys::wait::WaitStatus::Exited(_, 0) => "connected".into(),
                nix::sys::wait::WaitStatus::Exited(_, 250) => "the restriction failed".into(),
                nix::sys::wait::WaitStatus::Exited(_, code) if code >= 200 => {
                    format!("no socket: {}", nix::errno::Errno::from_raw(code - 200))
                }
                nix::sys::wait::WaitStatus::Exited(_, errno) => {
                    format!("refused: {}", nix::errno::Errno::from_raw(errno))
                }
                other => format!("{other:?}"),
            }
        }
    }
}

/// **M9b: does Landlock's TCP right bound MPTCP and SCTP?** With
/// `CONNECT_TCP` handled and no port allowed, and no seccomp filter, a plain
/// TCP connect to a loopback listener is denied (M5). An MPTCP and an SCTP
/// stream socket try the same: where either connects, Landlock does not bound
/// it, and the filter's protocol pin is what keeps it out.
#[test]
#[ignore = "a measurement of this box's kernel"]
fn m9b_landlock_tcp_against_mptcp_and_sctp() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let rules = || Ruleset::new(FS_READ_FILE, NET_CONNECT_TCP, 0);
    let tcp = landlock_connect_probe(&rules(), 0, port);
    let mptcp = landlock_connect_probe(&rules(), libc::IPPROTO_MPTCP, port);
    let sctp = landlock_connect_probe(&rules(), libc::IPPROTO_SCTP, port);
    println!("M9b plain TCP: {tcp}, MPTCP: {mptcp}, SCTP: {sctp}");
    assert!(tcp.contains("EACCES"), "plain TCP is bounded: {tcp}");
    // Measured unbounded: why the filter pins the protocol.
    assert_eq!(mptcp, "connected", "MPTCP passes Landlock's TCP right");
    assert!(
        !sctp.contains("EACCES"),
        "SCTP is not bounded by it: {sctp}"
    );
    drop(listener);
}

// ------------------------------------------------------------- the real busybox

/// Every filesystem right through ABI 5: `REFER` (2), `TRUNCATE` (3) and
/// `IOCTL_DEV` (5) beside ABI 1's.
const FS_ALL_V5: u64 = (1 << 16) - 1;
const FS_TRUNCATE: u64 = 1 << 14;
/// Pathname UNIX socket lookups, `connect` and an addressed `sendmsg`: ABI 9.
const FS_RESOLVE_UNIX: u64 = 1 << 16;

/// The static busybox under measurement, named by `WEAVER_BUSYBOX`: built
/// from a pinned source with the default applets, `ash` and the listing
/// (`CONFIG_BUSYBOX`, so `busybox --list` answers the compiled set).
/// Absent, the instrument fails naming it, never passing by skip.
fn busybox() -> PathBuf {
    let path = std::env::var_os("WEAVER_BUSYBOX").map(PathBuf::from).expect(
        "WEAVER_BUSYBOX names a statically linked busybox built with the default applets, ash and the listing",
    );
    assert!(path.is_file(), "{} is not a file", path.display());
    path
}

/// A scratch layout as the shell's stands in production (section 8.1):
/// `libexec/busybox` (a copy), `commands/` holding one symlink per listed
/// applet to it and **no entry for the interpreter**, and `home/` with its
/// `.tmp`. The shell is reached as the gate reaches it, the busybox file
/// itself run with `argv[0]` `sh` (Codex on #175).
struct ShellLayout {
    base: PathBuf,
}

impl ShellLayout {
    fn new(tag: &str, applets: &[&str]) -> Self {
        assert!(
            !applets.contains(&"sh"),
            "the interpreter is never a listed command"
        );
        let base = std::env::temp_dir().join(format!("weaver-shell-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for dir in ["libexec", "commands", "home/.tmp"] {
            std::fs::create_dir_all(base.join(dir)).unwrap();
        }
        std::fs::copy(busybox(), base.join("libexec/busybox")).unwrap();
        for applet in applets {
            std::os::unix::fs::symlink(
                base.join("libexec/busybox"),
                base.join("commands").join(applet),
            )
            .unwrap();
        }
        Self { base }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.base.join(rel)
    }

    /// The shell's ruleset as section 8.1 states it: execute and read on the
    /// busybox file, read on `commands/`, every right but execute beneath
    /// home (`TRUNCATE` among them unless `truncate_home` is false), read and
    /// write on `/dev/null` (with truncate where `truncate_null`), and read on
    /// `/dev/zero` and `/dev/urandom`.
    fn rules(&self, truncate_home: bool, truncate_null: bool) -> Ruleset {
        let home_rights = if truncate_home {
            FS_ALL_V5 & !FS_EXECUTE
        } else {
            FS_ALL_V5 & !FS_EXECUTE & !FS_TRUNCATE
        };
        let null_rights =
            FS_READ_FILE | FS_WRITE_FILE | if truncate_null { FS_TRUNCATE } else { 0 };
        // `RESOLVE_UNIX` (ABI 9) handled with no grant where the kernel offers
        // it, a second layer under the filter's `AF_UNIX` refusal.
        let handled = if abi().unwrap_or(0) >= 9 {
            FS_ALL_V5 | FS_RESOLVE_UNIX
        } else {
            FS_ALL_V5
        };
        Ruleset::new(handled, 0, 0)
            .allow(&self.path("libexec/busybox"), FS_EXECUTE | FS_READ_FILE)
            .allow(&self.path("commands"), FS_READ_DIR)
            .allow(&self.path("home"), home_rights)
            .allow(Path::new("/dev/null"), null_rights)
            .allow(Path::new("/dev/zero"), FS_READ_FILE)
            .allow(Path::new("/dev/urandom"), FS_READ_FILE)
    }

    /// `sh -c script`, the busybox file run with `argv[0]` `sh`, with the gate's
    /// environment: `HOME`, `PATH` the commands directory, `TMPDIR` home's
    /// `.tmp`.
    fn sh(&self, rules: &Ruleset, script: &str) -> Output {
        self.sh_filtered(rules, script, None)
    }

    /// [`ShellLayout::sh`] with the child's socket filter loaded too.
    fn sh_filtered(&self, rules: &Ruleset, script: &str, filter: Option<&[SockFilter]>) -> Output {
        rules.run_filtered(&mut self.sh_command(script), filter)
    }

    /// The shell's command, not yet run: the busybox file with `argv[0]`
    /// `sh`, `-c script`, and the gate's environment.
    fn sh_command(&self, script: &str) -> Command {
        let mut command = Command::new(self.path("libexec/busybox"));
        command
            .arg0("sh")
            .args(["-c", script])
            .env_clear()
            .env("HOME", self.path("home"))
            .env("PATH", self.path("commands"))
            .env("TMPDIR", self.path("home/.tmp"))
            .current_dir(self.path("home"));
        command
    }
}

impl Drop for ShellLayout {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

/// The default listed set of section 8.1, the interpreter not among them.
const APPLETS: [&str; 14] = [
    "ls", "cat", "head", "tail", "wc", "grep", "sort", "uniq", "cut", "tr", "date", "echo", "pwd",
    "mktemp",
];

/// **M11: the redirects a shell needs.** Under section 8.1's ruleset: a
/// redirect to `/dev/null`, `>` over an existing file in home (an open with
/// `O_TRUNC`), and `mktemp` under `TMPDIR`, home's `.tmp`. Then the two
/// truncate rights withheld in turn, to show which each redirect needs.
#[test]
#[ignore = "a measurement of this box's kernel and a built busybox"]
fn m11_redirects_truncation_and_tmpdir() {
    let layout = ShellLayout::new("m11", &APPLETS);
    let script = "ls /nonexistent 2>/dev/null; echo null=$?; \
                  echo first > f && echo second > f && cat f; \
                  t=$(mktemp) && echo tmp=$t && echo x > \"$t\" && cat \"$t\"";
    let full = layout.sh(&layout.rules(true, true), script);
    println!("M11 the ruleset as stated: {}", said(&full));
    assert_says("M11 null", &full, "null=1");
    assert_says("M11 truncate", &full, "second");
    assert_says("M11 mktemp", &full, "/home/.tmp/tmp.");
    let forms = layout.sh(
        &layout.rules(true, true),
        "echo TMPDIR=$TMPDIR; mktemp; echo bare=$?; mktemp -t; echo t=$?; mktemp -p \"$TMPDIR\"; echo p=$?",
    );
    println!("M11 mktemp's forms: {}", said(&forms));
    for form in ["bare=0", "t=0", "p=0"] {
        assert_says("M11 forms", &forms, form);
    }
    let no_home_truncate = layout.sh(
        &layout.rules(false, true),
        "echo first > f; echo second > f; echo rc=$?; cat f",
    );
    println!(
        "M11 TRUNCATE withheld beneath home: {}",
        said(&no_home_truncate)
    );
    assert_says(
        "M11 no home truncate",
        &no_home_truncate,
        "can't create f: Permission denied",
    );
    let no_null_truncate = layout.sh(&layout.rules(true, false), "echo x > /dev/null; echo rc=$?");
    println!(
        "M11 TRUNCATE withheld on /dev/null: {}",
        said(&no_null_truncate)
    );
    assert_says("M11 no null truncate", &no_null_truncate, "rc=0");
}

/// **M12: the real static busybox under the ruleset.** Its applet by the
/// symlink's full path, `busybox <applet>` by the binary's full path, and the
/// interpreter's `-c` with a pipe. **The compiled set is exactly the listed
/// applets and the interpreter's two names** (Codex on #175): `busybox
/// <applet>` reaches any applet compiled in, so one more in the build, `nc`
/// or another interpreter, widens the boundary whatever `commands/` lists.
/// An applet the build does not hold, asked of the binary, refuses.
#[test]
#[ignore = "a measurement of this box's kernel and a built busybox"]
fn m12_the_static_busybox_under_the_ruleset() {
    let layout = ShellLayout::new("m12", &APPLETS);
    let rules = || layout.rules(true, true);
    let listing = rules().run(Command::new(layout.path("libexec/busybox")).arg("--list"));
    assert_ran("M12 --list", &listing);
    let compiled: std::collections::BTreeSet<String> = String::from_utf8_lossy(&listing.stdout)
        .lines()
        .map(str::to_owned)
        .collect();
    let expected: std::collections::BTreeSet<String> = APPLETS
        .iter()
        .chain(&["sh", "ash"])
        .map(|name| (*name).to_owned())
        .collect();
    println!("M12 the compiled set: {compiled:?}");
    assert_eq!(compiled, expected, "M12 the compiled set is the listed one");
    let by_link = rules().run(Command::new(layout.path("commands/echo")).arg("by-link"));
    println!("M12 an applet by its link: {}", said(&by_link));
    assert_says("M12 by link", &by_link, "by-link");
    assert_ran("M12 by link", &by_link);
    let by_name =
        rules().run(Command::new(layout.path("libexec/busybox")).args(["echo", "by-name"]));
    println!("M12 busybox <applet>: {}", said(&by_name));
    assert_says("M12 by name", &by_name, "by-name");
    let pipe = layout.sh(&rules(), "echo a b c | tr ' ' '\\n' | wc -l");
    println!("M12 sh -c with a pipe: {}", said(&pipe));
    assert_eq!(
        String::from_utf8_lossy(&pipe.stdout).trim(),
        "3",
        "M12 pipe: {}",
        said(&pipe)
    );
    let absent = rules().run(Command::new(layout.path("libexec/busybox")).args(["wget", "--help"]));
    println!("M12 an applet not compiled in: {}", said(&absent));
    assert_says("M12 absent", &absent, "applet not found");
    let outside = layout.sh(&rules(), "/usr/bin/id; echo rc=$?");
    println!("M12 a program outside the build: {}", said(&outside));
    assert_says("M12 outside", &outside, "rc=126");
}

/// Section 8.1's rlimits, soft and hard alike, proposed for the operator's edit.
const SHELL_RLIMIT_FSIZE: u64 = 64 << 20;
const SHELL_RLIMIT_NOFILE: u64 = 64;
const SHELL_RLIMIT_AS: u64 = 512 << 20;
const SHELL_RLIMIT_CORE: u64 = 0;

/// **M14: the default set runs under section 8.1's rlimits, and they bind.**
/// The child sets each limit, soft and hard alike, before the ruleset, as the
/// gate's child will. Every default applet and the interpreter run under them.
/// `cat /dev/zero` into a home file stops at `SHELL_RLIMIT_FSIZE`, killed by
/// `SIGXFSZ` (153), its file exactly that size, and the shell reads every
/// limit as set, soft and hard, refusing a raise of each past its hard one.
#[test]
#[ignore = "a measurement of this box's kernel and a built busybox"]
fn m14_the_default_set_under_the_rlimits() {
    let layout = ShellLayout::new("m14", &APPLETS);
    let limited = |script: &str| {
        let mut command = layout.sh_command(script);
        // SAFETY: setrlimit on values held by copy, async-signal-safe.
        unsafe {
            command.pre_exec(|| {
                for (resource, value) in [
                    (libc::RLIMIT_FSIZE, SHELL_RLIMIT_FSIZE),
                    (libc::RLIMIT_NOFILE, SHELL_RLIMIT_NOFILE),
                    (libc::RLIMIT_AS, SHELL_RLIMIT_AS),
                    (libc::RLIMIT_CORE, SHELL_RLIMIT_CORE),
                ] {
                    let limit = libc::rlimit {
                        rlim_cur: value,
                        rlim_max: value,
                    };
                    if libc::setrlimit(resource, &limit) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
        layout.rules(true, true).run(&mut command)
    };
    let every = limited(
        "t=$(mktemp) && echo hello world > \"$t\" && cat \"$t\" | head -n 1 | tail -n 1 \
         | cut -d' ' -f1 | tr h H | grep Hello | sort | uniq | wc -l && ls \"$TMPDIR\" >/dev/null \
         && date >/dev/null && pwd >/dev/null && echo every=ok",
    );
    println!("M14 the default set: {}", said(&every));
    assert_eq!(
        String::from_utf8_lossy(&every.stdout).trim(),
        "1\nevery=ok",
        "M14 default set: {}",
        said(&every)
    );
    assert!(every.stderr.is_empty(), "M14 default set: {}", said(&every));
    let fsize = limited("cat /dev/zero > big; echo rc=$?; wc -c < big");
    println!("M14 RLIMIT_FSIZE: {}", said(&fsize));
    assert_says("M14 fsize stops", &fsize, "rc=153");
    assert_says("M14 fsize size", &fsize, &SHELL_RLIMIT_FSIZE.to_string());
    // ash's `ulimit` reads each limit, soft and hard, in its own units: `-f`
    // in 512-byte blocks, `-v` in KiB, `-n` a count, `-c` in blocks.
    let blocks = SHELL_RLIMIT_FSIZE / 512;
    let nofile = SHELL_RLIMIT_NOFILE;
    let kib = SHELL_RLIMIT_AS >> 10;
    let read = limited(
        "echo f=$(ulimit -Sf)/$(ulimit -Hf) n=$(ulimit -Sn)/$(ulimit -Hn) \
         v=$(ulimit -Sv)/$(ulimit -Hv) c=$(ulimit -Sc)/$(ulimit -Hc)",
    );
    println!("M14 the limits as the shell reads them: {}", said(&read));
    assert_says(
        "M14 limits",
        &read,
        &format!("f={blocks}/{blocks} n={nofile}/{nofile} v={kib}/{kib} c=0/0"),
    );
    // A raise of each, soft and hard together as `ulimit` sets them, one
    // past its hard limit, each refused.
    for (flag, past) in [
        ("f", blocks + 1),
        ("n", nofile + 1),
        ("v", kib + 1),
        ("c", 1),
    ] {
        let raise = limited(&format!("ulimit -{flag} {past} 2>/dev/null; echo raise=$?"));
        println!("M14 a raise of -{flag} to {past}: {}", said(&raise));
        assert!(
            !String::from_utf8_lossy(&raise.stdout).contains("raise=0"),
            "M14 -{flag} raised past its hard limit: {}",
            said(&raise)
        );
        assert_says("M14 raise", &raise, "raise=");
    }
}

/// **M13: both filters require no-new-privs.** Without `PR_SET_NO_NEW_PRIVS`
/// and without `CAP_SYS_ADMIN`, the kernel refuses `landlock_restrict_self`
/// and a seccomp filter's load, which is the gate's case, the agent's uid
/// holding no capability. Run as root in a user namespace, `CAP_SYS_ADMIN`
/// stands in for no-new-privs and both are admitted, and the instrument asserts
/// that too, so it holds in either run and measures the gate's case outside.
#[test]
#[ignore = "a measurement of this box's kernel"]
fn m13_both_filters_require_no_new_privs() {
    let rules = Ruleset::new(FS_READ_FILE, 0, 0);
    let ruleset = rules.0.as_raw_fd();
    let filter = [op(BPF_RET_K, 0, 0, SECCOMP_RET_ALLOW)];
    let prog = SockFprog {
        len: 1,
        filter: filter.as_ptr(),
    };
    // SAFETY: the child makes raw system calls only and exits.
    match unsafe { nix::unistd::fork() }.unwrap() {
        nix::unistd::ForkResult::Child => unsafe {
            let landlock = libc::syscall(libc::SYS_landlock_restrict_self, ruleset, 0u32);
            let landlock_errno = if landlock == 0 {
                0
            } else {
                *libc::__errno_location()
            };
            let seccomp = libc::prctl(
                libc::PR_SET_SECCOMP,
                libc::SECCOMP_MODE_FILTER,
                &prog as *const SockFprog,
                0,
                0,
            );
            let seccomp_errno = if seccomp == 0 {
                0
            } else {
                *libc::__errno_location()
            };
            libc::_exit((landlock_errno.min(15) << 4 | seccomp_errno.min(15)) as i32);
        },
        nix::unistd::ForkResult::Parent { child } => {
            let status = nix::sys::wait::waitpid(child, None).unwrap();
            let nix::sys::wait::WaitStatus::Exited(_, code) = status else {
                panic!("M13's child ended {status:?}");
            };
            let landlock = nix::errno::Errno::from_raw(code >> 4);
            let seccomp = nix::errno::Errno::from_raw(code & 0xf);
            println!(
                "M13 landlock_restrict_self without no-new-privs: {landlock}, seccomp without it: {seccomp}"
            );
            if nix::unistd::geteuid().is_root() {
                // Root in a user namespace holds CAP_SYS_ADMIN there, which the
                // kernel accepts in place of no-new-privs for both: the run
                // outside a namespace is the one that measures the gate's case.
                assert_eq!(landlock, nix::errno::Errno::UnknownErrno);
                assert_eq!(seccomp, nix::errno::Errno::UnknownErrno);
            } else {
                // The gate's case: an unprivileged uid, as the agent's.
                assert_eq!(landlock, nix::errno::Errno::EPERM);
                assert_eq!(seccomp, nix::errno::Errno::EACCES);
            }
        }
    }
}

/// Connects to the pathname UNIX socket `path` from a child confined by
/// `rules` alone, answering what it met.
fn unix_connect_probe(rules: &Ruleset, path: &Path) -> String {
    let ruleset = rules.0.as_raw_fd();
    let path = CString::new(path.to_str().unwrap()).unwrap();
    // SAFETY: the child makes raw system calls only and exits.
    match unsafe { nix::unistd::fork() }.unwrap() {
        nix::unistd::ForkResult::Child => unsafe {
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0
                || libc::syscall(libc::SYS_landlock_restrict_self, ruleset, 0u32) != 0
            {
                libc::_exit(250);
            }
            let fd = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0);
            if fd < 0 {
                libc::_exit(249);
            }
            let mut addr: libc::sockaddr_un = std::mem::zeroed();
            addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
            for (i, b) in path.as_bytes().iter().enumerate() {
                addr.sun_path[i] = *b as libc::c_char;
            }
            let r = libc::connect(
                fd,
                &addr as *const _ as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t,
            );
            let errno = if r == 0 { 0 } else { *libc::__errno_location() };
            libc::_exit(errno.min(199));
        },
        nix::unistd::ForkResult::Parent { child } => {
            match nix::sys::wait::waitpid(child, None).unwrap() {
                nix::sys::wait::WaitStatus::Exited(_, 0) => "connected".into(),
                nix::sys::wait::WaitStatus::Exited(_, 250) => "the restriction failed".into(),
                nix::sys::wait::WaitStatus::Exited(_, 249) => "no socket".into(),
                nix::sys::wait::WaitStatus::Exited(_, errno) => {
                    format!("refused: {}", nix::errno::Errno::from_raw(errno))
                }
                other => format!("{other:?}"),
            }
        }
    }
}

/// **M9c: pathname UNIX sockets, two layers, and the shell still works.**
/// A socket this uid can reach is bound outside the domain. Landlock alone
/// with `RESOLVE_UNIX` handled and no grant (ABI 9) refuses the connect. With
/// it unhandled the same connect succeeds, which is the gap below ABI 9 the
/// filter's `AF_UNIX` refusal closes (M9). Then the shell under the filter and
/// the full ruleset: its pipeline and its redirects, which use `pipe` and
/// never a socket, still work.
#[test]
#[ignore = "a measurement of this box's kernel and a built busybox"]
fn m9c_pathname_sockets_and_the_shell_under_the_filter() {
    let version = abi().expect("Landlock is enabled");
    let path =
        std::env::temp_dir().join(format!("weaver-landlock-m9c-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let unhandled = unix_connect_probe(&Ruleset::new(FS_READ_FILE, 0, 0), &path);
    println!("M9c a pathname connect, RESOLVE_UNIX not handled: {unhandled}");
    assert_eq!(unhandled, "connected", "the gap the filter closes");
    if version >= 9 {
        let handled =
            unix_connect_probe(&Ruleset::new(FS_READ_FILE | FS_RESOLVE_UNIX, 0, 0), &path);
        println!("M9c a pathname connect, RESOLVE_UNIX handled, no grant: {handled}");
        assert!(handled.contains("EACCES"), "{handled}");
    }
    drop(listener);
    let _ = std::fs::remove_file(&path);
    let layout = ShellLayout::new("m9c", &APPLETS);
    let filter = socket_filter(false);
    let out = layout.sh_filtered(
        &layout.rules(true, true),
        "echo a b c | tr ' ' '\\n' | wc -l; ls /nonexistent 2>/dev/null; echo x > f && cat f",
        Some(&filter),
    );
    println!("M9c the shell under the filter: {}", said(&out));
    assert_ran("M9c", &out);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .collect::<Vec<_>>(),
        ["3", "x"],
        "M9c: {}",
        said(&out)
    );
}

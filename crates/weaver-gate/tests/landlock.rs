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
        let ruleset = self.0.as_raw_fd();
        // SAFETY: prctl and a raw system call on a descriptor opened before the
        // fork, both async-signal-safe.
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::syscall(libc::SYS_landlock_restrict_self, ruleset, 0u32) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        match command.env_clear().stdin(Stdio::null()).output() {
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

/// **M1: the ABI.** Landlock is enabled and reports its version; signal and
/// abstract-socket scoping need 6, TCP rules 4, `REFER` 2, `TRUNCATE` 3.
#[test]
#[ignore = "a measurement of this box's kernel"]
fn m1_the_landlock_abi() {
    let version = abi().expect("Landlock is enabled");
    println!("M1 Landlock ABI version: {version}");
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
    // b: the libraries neither readable nor executable.
    let bare = Ruleset::new(handled, 0, 0).allow(&bin.0, FS_EXECUTE | FS_READ_FILE);
    let b = bare.run(Command::new(bin.path("ls")).arg("/"));
    println!("M2b libraries unreadable: {}", said(&b));
    // c: the libraries readable and executable.
    let mut full = Ruleset::new(handled, 0, 0).allow(&bin.0, FS_EXECUTE | FS_READ_FILE);
    for root in LIBRARY_ROOTS {
        full = full.allow(Path::new(root), FS_EXECUTE | FS_READ_FILE | FS_READ_DIR);
    }
    let c = full.run(Command::new(bin.path("ls")).arg("/"));
    println!("M2c libraries executable: {}", said(&c));
    // d: /usr/bin/ls itself, outside the command directory, libraries readable.
    let mut outside = Ruleset::new(handled, 0, 0).allow(&bin.0, FS_EXECUTE | FS_READ_FILE);
    for root in LIBRARY_ROOTS {
        outside = outside.allow(Path::new(root), FS_READ_FILE | FS_READ_DIR);
    }
    let d = outside.run(Command::new("/usr/bin/ls").arg("/"));
    println!("M2d /usr/bin/ls outside the directory: {}", said(&d));
    // e: the interpreter alone executable, the libraries read-only, `/` readable.
    let e = standard(Ruleset::new(handled, 0, 0), &bin)
        .allow(Path::new("/"), FS_READ_DIR)
        .run(Command::new(bin.path("ls")).arg("/"));
    println!("M2e the interpreter alone executable: {}", said(&e));
    // f: the interpreter not executable, everything else as e.
    let mut f = Ruleset::new(handled, 0, 0).allow(&bin.0, FS_EXECUTE | FS_READ_FILE);
    for root in LIBRARY_ROOTS {
        f = f.allow(Path::new(root), FS_READ_FILE | FS_READ_DIR);
    }
    let f = f
        .allow(Path::new("/"), FS_READ_DIR)
        .run(Command::new(bin.path("ls")).arg("/"));
    println!("M2f the interpreter read-only: {}", said(&f));
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
    let awk = rules().run(
        Command::new(bin.path("awk"))
            .args(["BEGIN { r = system(\"/usr/bin/id\"); print \"system returned \" r }"]),
    );
    println!("M4b awk system(/usr/bin/id): {}", said(&awk));
    let bash = rules().run(Command::new(bin.path("bash")).args(["-c", "/usr/bin/id"]));
    println!("M4c bash -c /usr/bin/id: {}", said(&bash));
    let listed = rules()
        .run(Command::new(bin.path("bash")).args(["-c", &format!("{}", bin.path("id").display())]));
    println!("M4d bash -c <listed id>: {}", said(&listed));
}

/// **M5: TCP connect denial.** With `CONNECT_TCP` handled and no port allowed,
/// `bash`'s `/dev/tcp` connect to a local listener fails; with that port
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
    let allowed = rules(Some(port)).run(Command::new(bin.path("bash")).args(["-c", &script]));
    println!("M5b connect, the port elected: {}", said(&allowed));
    drop(listener);
}

/// **M6: signal and abstract-socket scoping.** With `SCOPE_SIGNAL`, a confined
/// `bash` cannot signal a process outside its domain (`kill -0`), and can
/// signal its own child; with `SCOPE_ABSTRACT_UNIX_SOCKET` it cannot connect
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
    let other =
        standard(Ruleset::new(handled, 0, 0), &bin).run(&mut Command::new(base.join("source/id")));
    println!("M3b by its other path: {}", said(&other));
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
    // The agent could write these bytes itself; the suite places them.
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
    let _ = std::fs::remove_dir_all(&home);
}

/// **M8c: a `noexec` home closes the route for a dynamic set.** Run under
/// `unshare --map-root-user --mount`: a tmpfs mounted `noexec` stands for
/// the agent's home, and the interpreter, executable as a dynamic set needs,
/// cannot map the payload there. Fails naming the namespace when not in one.
#[test]
#[ignore = "a measurement of this box's kernel; run under unshare --map-root-user --mount"]
fn m8c_a_noexec_home_closes_the_interpreter_route() {
    assert!(
        nix::unistd::geteuid().is_root(),
        "run under `unshare --map-root-user --mount`"
    );
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
}

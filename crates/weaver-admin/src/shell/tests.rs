//! The shell's deployment (#29, `weaver-admin-Spec` sections 4, 6 and 9).
//! The judgment of a real busybox runs as an `#[ignore]`d instrument naming
//! `WEAVER_BUSYBOX`, a static busybox built from the stack's pinned
//! configuration with the listing. Everything else runs plain.

use super::*;
use std::os::unix::fs::PermissionsExt;

/// The default listed set, the stack's `.applets`.
const APPLETS: [&str; 14] = [
    "ls", "cat", "head", "tail", "wc", "grep", "sort", "uniq", "cut", "tr", "date", "echo", "pwd",
    "mktemp",
];

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

/// A minimal 64-bit little-endian ELF header with one program header of
/// `p_type`.
fn elf_with(p_type: u32) -> Vec<u8> {
    let mut bytes = vec![0u8; 64 + 56];
    bytes[..4].copy_from_slice(b"\x7fELF");
    bytes[4] = 2;
    bytes[5] = 1;
    bytes[32..40].copy_from_slice(&64u64.to_le_bytes());
    bytes[54..56].copy_from_slice(&56u16.to_le_bytes());
    bytes[56..58].copy_from_slice(&1u16.to_le_bytes());
    bytes[64..68].copy_from_slice(&p_type.to_le_bytes());
    bytes
}

/// **A static image names no interpreter, and a dynamic one does** (Spec
/// section 4, `PT_INTERP`): a header with a `PT_LOAD` alone reads static, one
/// with a `PT_INTERP` reads dynamic, this suite's own executable (dynamically
/// linked) reads dynamic, and anything not a 64-bit little-endian ELF image
/// refuses.
///
/// Perturbation: compare against another type than `PT_INTERP` and the
/// dynamic cases read static.
#[test]
fn a_dynamic_binary_names_an_interpreter() {
    assert_eq!(names_an_interpreter(&elf_with(1)), Ok(false));
    assert_eq!(names_an_interpreter(&elf_with(PT_INTERP)), Ok(true));
    let own = std::fs::read(std::env::current_exe().unwrap()).unwrap();
    assert_eq!(
        names_an_interpreter(&own),
        Ok(true),
        "the test binary is dynamic"
    );
    assert!(names_an_interpreter(b"#!/bin/sh\n").is_err());
    let mut truncated = elf_with(1);
    truncated.truncate(66);
    assert!(
        names_an_interpreter(&truncated).is_err(),
        "a header past the end refuses"
    );
    let mut far = elf_with(1);
    far[32..40].copy_from_slice(&(u64::MAX - 1).to_le_bytes());
    assert!(
        names_an_interpreter(&far).is_err(),
        "an offset at the edge refuses, never panics"
    );
}

/// **A listing that does not answer within its bound is killed and
/// refused**: a program that blocks instead of listing cannot hold the
/// inventory, and the locks it runs under, open. The control: a program
/// that answers at once is read.
///
/// Perturbation: wait without the bound and the blocking case hangs past
/// the test's own timeout.
#[test]
fn a_listing_past_its_bound_is_killed_and_refused() {
    let dir = std::env::temp_dir().join(format!("weaver-admin-list-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let blocking = dir.join("blocking");
    std::fs::write(&blocking, "#!/bin/sh\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&blocking, std::fs::Permissions::from_mode(0o755)).unwrap();
    let answering = dir.join("answering");
    std::fs::write(&answering, "#!/bin/sh\nprintf 'sh\\nash\\nls\\n'\n").unwrap();
    std::fs::set_permissions(&answering, std::fs::Permissions::from_mode(0o755)).unwrap();
    let bound = std::time::Duration::from_millis(300);
    let started = std::time::Instant::now();
    let refused = compiled_set_within(&blocking, bound).unwrap_err();
    assert!(refused.contains("did not answer"), "{refused}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "killed at its bound"
    );
    assert_eq!(
        compiled_set_within(&answering, bound),
        Ok(set(&["sh", "ash", "ls"]))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// **The compiled set holds `sh` and `ash`, and the rest equals the list**
/// (Spec section 4): an applet compiled and not listed refuses (`busybox
/// <applet>` reaches it), a listed name not compiled refuses (a link to
/// nothing), and a build missing an interpreter name refuses (every call
/// would fail). The control: the exact set passes.
///
/// Perturbation: drop the interpreter check and the third case passes.
#[test]
fn the_compiled_set_must_be_the_list_and_the_interpreter() {
    let at = Path::new("/stack/busybox");
    let listed = set(&APPLETS);
    let exact: BTreeSet<String> = listed.iter().cloned().chain(set(&["sh", "ash"])).collect();
    assert_eq!(compare(at, exact.clone(), &listed), Ok(()));
    let mut with_nc = exact.clone();
    with_nc.insert("nc".into());
    assert!(
        compare(at, with_nc, &listed)
            .unwrap_err()
            .contains("\"nc\"")
    );
    let mut without_ls = exact.clone();
    without_ls.remove("ls");
    assert!(
        compare(at, without_ls, &listed)
            .unwrap_err()
            .contains("\"ls\"")
    );
    let mut without_sh = exact.clone();
    without_sh.remove("sh");
    assert!(
        compare(at, without_sh, &listed)
            .unwrap_err()
            .contains("no sh compiled in")
    );
}

/// **`commands/` holds one link per listed applet and no interpreter, at
/// `0755`, rebuilt afresh** (Spec section 6): a stale directory with a stale
/// entry is removed first, each applet links to the judged binary, and the
/// run end's removal takes it whole and reads an absent one as removed.
///
/// Perturbation: build into a standing directory without removing it and
/// the stale entry survives.
#[test]
fn commands_is_rebuilt_afresh_and_removed() {
    let run = std::env::temp_dir().join(format!("weaver-admin-commands-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&run);
    std::fs::create_dir_all(run.join(COMMANDS)).unwrap();
    std::fs::write(run.join(COMMANDS).join("stale"), b"").unwrap();
    let judged = Judged {
        binary: PathBuf::from("/stack/busybox"),
        applets: APPLETS.iter().map(|name| (*name).to_string()).collect(),
    };
    let commands = build_commands(&run, &judged).expect("built");
    assert_eq!(commands, run.join(COMMANDS));
    let mode = std::fs::metadata(&commands).unwrap().permissions().mode() & 0o7777;
    assert_eq!(mode, 0o755);
    let mut entries: Vec<String> = std::fs::read_dir(&commands)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    entries.sort();
    let mut expected: Vec<String> = APPLETS.iter().map(|name| (*name).to_string()).collect();
    expected.sort();
    assert_eq!(
        entries, expected,
        "the listed applets alone, no stale entry, no sh"
    );
    assert_eq!(
        std::fs::read_link(commands.join("ls")).unwrap(),
        PathBuf::from("/stack/busybox")
    );
    remove_commands(&run).expect("removed");
    assert!(!commands.exists());
    remove_commands(&run).expect("an absent one reads as removed");
    let _ = std::fs::remove_dir_all(&run);
}

/// A scratch stack: the busybox copied in, owned by this suite's uid, at
/// `0755`, its `.applets` beside it at `0644`.
struct Stack {
    base: PathBuf,
}

impl Stack {
    fn new(tag: &str, applets: &[&str]) -> Stack {
        let base =
            std::env::temp_dir().join(format!("weaver-admin-shell-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o755)).unwrap();
        let busybox = std::env::var_os("WEAVER_BUSYBOX")
            .map(PathBuf::from)
            .expect("WEAVER_BUSYBOX names a static busybox built with the listing");
        let binary = base.join("busybox");
        std::fs::copy(&busybox, &binary).unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        let list = applets_path(&binary);
        std::fs::write(&list, applets.join("\n") + "\n").unwrap();
        std::fs::set_permissions(&list, std::fs::Permissions::from_mode(0o644)).unwrap();
        Stack { base }
    }

    fn binary(&self) -> PathBuf {
        self.base.join("busybox")
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn owner() -> u32 {
    nix::unistd::geteuid().as_raw()
}

/// **The stack's busybox judges, and each judgment refuses with its one
/// fault** (Spec section 4): the pinned build with its exact list passes,
/// answering its canonical path and the list. Then, from that passing
/// control, one change each: a list missing an applet compiled in, a list
/// naming one not compiled, a list naming the interpreter, a binary writable
/// by its group, an applet list writable by its group, and a binary reached
/// through a symlink. Each refuses.
#[test]
#[ignore = "judges a built busybox: needs WEAVER_BUSYBOX"]
fn the_stacks_busybox_judges_and_each_fault_refuses() {
    let stack = Stack::new("judge", &APPLETS);
    let judged = judge(&stack.binary(), owner()).expect("the pinned build with its list judges");
    assert_eq!(
        judged.binary,
        std::fs::canonicalize(stack.binary()).unwrap()
    );
    assert_eq!(judged.applets, APPLETS);

    let short = Stack::new("short", &APPLETS[1..]);
    assert!(
        judge(&short.binary(), owner())
            .unwrap_err()
            .contains("\"ls\"")
    );
    let mut wider = APPLETS.to_vec();
    wider.push("wget");
    let wide = Stack::new("wide", &wider);
    assert!(
        judge(&wide.binary(), owner())
            .unwrap_err()
            .contains("\"wget\"")
    );
    let mut with_sh = APPLETS.to_vec();
    with_sh.push("sh");
    let interp = Stack::new("interp", &with_sh);
    assert!(
        judge(&interp.binary(), owner())
            .unwrap_err()
            .contains("not an applet")
    );

    let writable = Stack::new("writable", &APPLETS);
    std::fs::set_permissions(writable.binary(), std::fs::Permissions::from_mode(0o775)).unwrap();
    assert!(
        judge(&writable.binary(), owner())
            .unwrap_err()
            .contains("not root's alone")
    );
    let list = Stack::new("list", &APPLETS);
    std::fs::set_permissions(
        applets_path(&list.binary()),
        std::fs::Permissions::from_mode(0o664),
    )
    .unwrap();
    assert!(
        judge(&list.binary(), owner())
            .unwrap_err()
            .contains("not root's alone")
    );

    // Any file name: busybox answers `--list` only as `busybox`, which the
    // judgment runs it as whatever the stack named it.
    let renamed = Stack::new("renamed", &APPLETS);
    let weaver_sh = renamed.base.join("weaver-sh");
    std::fs::rename(renamed.binary(), &weaver_sh).unwrap();
    std::fs::rename(applets_path(&renamed.binary()), applets_path(&weaver_sh)).unwrap();
    judge(&weaver_sh, owner()).expect("a busybox under another name judges");

    let linked = Stack::new("linked", &APPLETS);
    let link = linked.base.join("link");
    std::os::unix::fs::symlink(linked.binary(), &link).unwrap();
    std::fs::copy(applets_path(&linked.binary()), applets_path(&link)).unwrap();
    assert!(
        judge(&link, owner())
            .unwrap_err()
            .contains("not a regular file")
    );
}

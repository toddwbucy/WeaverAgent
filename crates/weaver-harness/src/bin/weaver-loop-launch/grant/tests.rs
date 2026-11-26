//! The grant file's form and its judgment, a row per refusal. The suite's own
//! uid stands for root as the owner, as the gate's suite does for `commands/`.

use super::*;

fn own() -> u32 {
    nix::unistd::geteuid().as_raw()
}

const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn text(lines: &[&str]) -> Vec<u8> {
    let mut s = String::new();
    for line in lines {
        s.push_str(line);
        s.push('\n');
    }
    s.into_bytes()
}

fn good() -> Vec<u8> {
    text(&[
        VERSION_LINE,
        &format!("interpreter {DIGEST} /usr/bin/python3"),
        "execute /lib/ld.so",
        "read /lib/libc.so",
        "beneath /lib/python",
        "pycache /opt/empty",
    ])
}

#[test]
fn a_well_formed_grant_reads() {
    let grant = parse(&good()).unwrap();
    assert_eq!(grant.interpreter, PathBuf::from("/usr/bin/python3"));
    assert_eq!(grant.interpreter_sha256, DIGEST);
    assert_eq!(grant.execute, [PathBuf::from("/lib/ld.so")]);
    assert_eq!(grant.read, [PathBuf::from("/lib/libc.so")]);
    assert_eq!(grant.beneath, [PathBuf::from("/lib/python")]);
    assert_eq!(grant.pycache, PathBuf::from("/opt/empty"));
}

/// **A path is the rest of its line**, a space in it included.
#[test]
fn a_path_with_a_space_is_one_path() {
    let mut lines = vec![
        VERSION_LINE.to_string(),
        format!("interpreter {DIGEST} /usr/bin/python3"),
    ];
    lines.push("read /lib/a b.so".to_string());
    lines.push("pycache /opt/empty".to_string());
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    assert_eq!(
        parse(&text(&refs)).unwrap().read,
        [PathBuf::from("/lib/a b.so")]
    );
}

/// A row per refusal of the form, each naming why.
#[test]
fn each_malformed_grant_is_refused_naming_why() {
    let interp = format!("interpreter {DIGEST} /usr/bin/python3");
    let rows: Vec<(&str, Vec<u8>, &str)> = vec![
        (
            "no version line",
            text(&[&interp, "pycache /p"]),
            "line 1 is not",
        ),
        (
            "another version",
            text(&["weaver-loop-grant 2", &interp, "pycache /p"]),
            "line 1 is not",
        ),
        (
            "cut short",
            {
                let mut b = good();
                b.pop();
                b
            },
            "may be cut short",
        ),
        (
            "not ASCII",
            text(&[VERSION_LINE, &interp, "pycache /\u{e9}"]),
            "not ASCII",
        ),
        (
            "no interpreter",
            text(&[VERSION_LINE, "pycache /p"]),
            "names no interpreter",
        ),
        (
            "two interpreters",
            text(&[
                VERSION_LINE,
                &interp,
                &format!("interpreter {DIGEST} /x"),
                "pycache /p",
            ]),
            "line 3 names a second interpreter",
        ),
        (
            "a short digest",
            text(&[
                VERSION_LINE,
                "interpreter abc /usr/bin/python3",
                "pycache /p",
            ]),
            "line 2: the digest is not 64",
        ),
        (
            "an uppercase digest",
            text(&[
                VERSION_LINE,
                &format!("interpreter {} /usr/bin/python3", DIGEST.to_uppercase()),
                "pycache /p",
            ]),
            "line 2: the digest is not 64",
        ),
        (
            "no pycache",
            text(&[VERSION_LINE, &interp]),
            "names no pycache folder",
        ),
        (
            "two pycache",
            text(&[VERSION_LINE, &interp, "pycache /p", "pycache /q"]),
            "line 4 names a second pycache folder",
        ),
        (
            "a relative path",
            text(&[VERSION_LINE, &interp, "read lib/libc.so", "pycache /p"]),
            "line 3: lib/libc.so is not absolute",
        ),
        (
            "a path named twice",
            text(&[VERSION_LINE, &interp, "read /l", "beneath /l", "pycache /p"]),
            "line 4: /l is named twice",
        ),
        (
            "an unknown keyword",
            text(&[VERSION_LINE, &interp, "write /tmp", "pycache /p"]),
            "line 3: \"write\" is no grant keyword",
        ),
        (
            "a line with no path",
            text(&[VERSION_LINE, &interp, "read", "pycache /p"]),
            "line 3 has no path",
        ),
        (
            "an empty line",
            text(&[VERSION_LINE, &interp, "", "pycache /p"]),
            "line 3 has no path",
        ),
    ];
    for (name, bytes, want) in rows {
        let got = parse(&bytes).expect_err(name);
        assert!(got.contains(want), "{name}: {got:?} lacks {want:?}");
    }
}

/// A scratch layout as the stack lays it: the runner's folder with
/// `runner.py` and the grant file, an empty pycache folder, a loop folder.
struct Layout(PathBuf);

impl Layout {
    fn new(tag: &str) -> Layout {
        let root =
            std::env::temp_dir().join(format!("weaver-loop-grant-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["runner", "pycache", "loop", "lib"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        std::fs::write(root.join("runner/runner.py"), b"").unwrap();
        std::fs::write(root.join("lib/interp"), b"interpreter bytes").unwrap();
        std::fs::write(root.join("lib/libc"), b"").unwrap();
        // Modes set, not left to the umask: the judgment reads group and
        // other write.
        use std::os::unix::fs::PermissionsExt as _;
        for (path, mode) in [
            ("runner", 0o755),
            ("pycache", 0o755),
            ("runner/runner.py", 0o644),
            ("lib/interp", 0o755),
        ] {
            std::fs::set_permissions(root.join(path), std::fs::Permissions::from_mode(mode))
                .unwrap();
        }
        Layout(root)
    }

    fn grant(&self) -> Grant {
        Grant {
            interpreter: self.0.join("lib/interp"),
            interpreter_sha256: sha256_hex(b"interpreter bytes"),
            execute: vec![],
            read: vec![self.0.join("lib/libc")],
            beneath: vec![self.0.join("lib")],
            pycache: self.0.join("pycache"),
        }
    }

    fn judge(&self, grant: &Grant) -> Result<Vec<Rule>, String> {
        judge(
            grant,
            &Launch {
                runner_dir: &self.0.join("runner"),
                loop_dir: &self.0.join("loop"),
            },
            own(),
        )
    }
}

impl Drop for Layout {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The control: the layout as laid judges, the interpreter first.
#[test]
fn the_laid_grant_judges() {
    let layout = Layout::new("ok");
    let rules = layout.judge(&layout.grant()).unwrap();
    assert_eq!(rules[0].path, layout.0.join("lib/interp"));
    assert_eq!(rules[0].access, FS_EXECUTE | FS_READ_FILE);
    let paths: Vec<_> = rules.iter().map(|r| r.path.clone()).collect();
    assert!(paths.contains(&layout.0.join("runner")));
    assert!(paths.contains(&layout.0.join("loop")));
    assert!(
        !paths.contains(&layout.0.join("pycache")),
        "the pycache folder is granted nothing"
    );
}

/// **A stale digest refuses**: the interpreter changed after the grant was
/// computed. Perturbation: skip the comparison and the launch goes on.
#[test]
fn a_stale_digest_is_refused() {
    let layout = Layout::new("digest");
    std::fs::write(layout.0.join("lib/interp"), b"an updated interpreter").unwrap();
    let got = layout.judge(&layout.grant()).err().unwrap();
    assert!(got.contains("not the grant's"), "{got}");
    assert!(got.contains(&layout.0.join("lib/interp").display().to_string()));
}

/// A row per stale or wrong path, each refusal naming the path.
#[test]
fn each_stale_or_wrong_path_is_refused_naming_it() {
    type Change = fn(&Layout, &mut Grant) -> PathBuf;
    let rows: [(&str, Change, &str); 10] = [
        (
            "a library gone",
            |l, g| {
                let p = l.0.join("lib/libgone");
                g.read.push(p.clone());
                p
            },
            "does not open",
        ),
        (
            "the interpreter gone",
            |l, g| {
                let p = l.0.join("lib/interp");
                std::fs::remove_file(&p).unwrap();
                g.interpreter = p.clone();
                p
            },
            "does not open",
        ),
        (
            "a library that is a link",
            |l, g| {
                let p = l.0.join("lib/liblink");
                std::os::unix::fs::symlink(l.0.join("lib/libc"), &p).unwrap();
                g.read.push(p.clone());
                p
            },
            "is a link",
        ),
        (
            "a library that is a folder",
            |l, g| {
                let p = l.0.join("loop");
                g.read.push(p.clone());
                p
            },
            "is not a regular file",
        ),
        (
            "a folder beneath that is a file",
            |l, g| {
                let p = l.0.join("lib/libc");
                g.read.clear();
                g.beneath = vec![p.clone()];
                p
            },
            "is not a directory",
        ),
        (
            "the pycache folder not empty",
            |l, g| {
                std::fs::write(l.0.join("pycache/x.pyc"), b"").unwrap();
                g.pycache.clone()
            },
            "is not empty",
        ),
        (
            "the pycache folder a link to an empty folder",
            |l, _| {
                let p = l.0.join("pycache");
                std::fs::remove_dir(&p).unwrap();
                std::fs::create_dir(l.0.join("empty")).unwrap();
                std::os::unix::fs::symlink(l.0.join("empty"), &p).unwrap();
                p
            },
            "does not open to list",
        ),
        (
            "the pycache folder writable by others",
            |l, g| {
                std::fs::set_permissions(
                    l.0.join("pycache"),
                    std::os::unix::fs::PermissionsExt::from_mode(0o777),
                )
                .unwrap();
                g.pycache.clone()
            },
            "writable by its group or others",
        ),
        (
            "the runner absent",
            |l, _| {
                let p = l.0.join("runner/runner.py");
                std::fs::remove_file(&p).unwrap();
                p
            },
            "does not open",
        ),
        (
            "the loop folder a link",
            |l, _| {
                let p = l.0.join("loop");
                std::fs::remove_dir(&p).unwrap();
                std::os::unix::fs::symlink(l.0.join("lib"), &p).unwrap();
                p
            },
            "is a link",
        ),
    ];
    for (index, (name, change, want)) in rows.into_iter().enumerate() {
        let layout = Layout::new(&format!("row{index}"));
        let mut grant = layout.grant();
        let path = change(&layout, &mut grant);
        let got = layout
            .judge(&grant)
            .err()
            .unwrap_or_else(|| panic!("{name} judged"));
        assert!(got.contains(want), "{name}: {got:?} lacks {want:?}");
        assert!(
            got.contains(&path.display().to_string()),
            "{name}: {got:?} names no path"
        );
    }
}

/// **Every file in the runner's folder is judged as the grant file is**: a
/// row per refusal, each naming the path, and the board laid as the stack
/// lays it judges. Perturbation: drop the mode check, or the listing, and a
/// row is admitted.
#[test]
fn each_file_of_the_runners_folder_is_judged_as_the_grant_file_is() {
    use std::os::unix::fs::PermissionsExt as _;
    let set_mode = |path: &Path, mode: u32| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap()
    };
    type Change = fn(&Layout, &dyn Fn(&Path, u32)) -> PathBuf;
    let rows: [(&str, Change, &str); 3] = [
        (
            "runner.py written by its group",
            |l, mode| {
                let p = l.0.join("runner/runner.py");
                mode(&p, 0o664);
                p
            },
            "is writable by its group or others",
        ),
        (
            "the board written by others",
            |l, mode| {
                let p = l.0.join("runner/weaver_board.py");
                std::fs::write(&p, b"").unwrap();
                mode(&p, 0o646);
                p
            },
            "is writable by its group or others",
        ),
        (
            "a folder standing in the runner's folder",
            |l, _| {
                let p = l.0.join("runner/__pycache__");
                std::fs::create_dir(&p).unwrap();
                p
            },
            "is not a regular file",
        ),
    ];
    for (name, change, want) in rows {
        let layout = Layout::new("runner-files");
        let path = change(&layout, &set_mode);
        let got = layout
            .judge(&layout.grant())
            .err()
            .unwrap_or_else(|| panic!("{name} judged"));
        assert!(got.contains(want), "{name}: {got:?} lacks {want:?}");
        assert!(
            got.contains(&path.display().to_string()),
            "{name}: {got:?} names no path"
        );
    }
    // The control: the board laid beside the runner, 0644, judges.
    let layout = Layout::new("runner-board");
    let board = layout.0.join("runner/weaver_board.py");
    std::fs::write(&board, b"").unwrap();
    set_mode(&board, 0o644);
    set_mode(&layout.0.join("runner/runner.py"), 0o644);
    layout.judge(&layout.grant()).unwrap();
}

/// **The interpreter and each `execute` file are written by no one the launch
/// answers to**, a row each: the interpreter owned by the uid the launch runs
/// as where that is not the owner, the interpreter written by others, an
/// `execute` file written by its group. Perturbation: judge either as a
/// regular file alone and its rows are admitted.
#[test]
fn an_interpreter_or_execute_file_the_launch_could_write_is_refused() {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = |path: &Path, mode: u32| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap()
    };
    // The suite's uid as the running uid, judged for another owner: the
    // interpreter, judged first, is the running uid's.
    let layout = Layout::new("interp-owner");
    mode(&layout.0.join("lib/interp"), 0o755);
    let got = judge(
        &layout.grant(),
        &Launch {
            runner_dir: &layout.0.join("runner"),
            loop_dir: &layout.0.join("loop"),
        },
        own().wrapping_add(1),
    )
    .err()
    .unwrap();
    assert_eq!(
        got,
        format!(
            "{} is owned by uid {}, the uid the launch runs as",
            layout.0.join("lib/interp").display(),
            own()
        )
    );
    let layout = Layout::new("interp-mode");
    mode(&layout.0.join("lib/interp"), 0o757);
    let got = layout.judge(&layout.grant()).err().unwrap();
    assert!(
        got.ends_with("lib/interp is writable by its group or others"),
        "{got}"
    );
    let layout = Layout::new("execute-mode");
    let loader = layout.0.join("lib/loader");
    std::fs::write(&loader, b"").unwrap();
    mode(&loader, 0o775);
    let mut grant = layout.grant();
    grant.execute.push(loader.clone());
    let got = layout.judge(&grant).err().unwrap();
    assert_eq!(
        got,
        format!("{} is writable by its group or others", loader.display())
    );
    // The control: the loader 0755 judges.
    mode(&loader, 0o755);
    layout.judge(&grant).unwrap();
}

/// **The runner's folder must be the owner's**: here the suite's uid stands
/// for root, and another owner (root's own `/`) is refused.
#[test]
fn a_runner_folder_of_another_owner_is_refused() {
    if own() == 0 {
        eprintln!(
            "SKIP a_runner_folder_of_another_owner_is_refused: run as root, where /usr is the owner's"
        );
        return;
    }
    let layout = Layout::new("owner");
    let got = judge(
        &layout.grant(),
        &Launch {
            runner_dir: Path::new("/usr"),
            loop_dir: &layout.0.join("loop"),
        },
        own(),
    )
    .err()
    .unwrap();
    assert!(got.contains("/usr is owned by uid 0"), "{got}");
}

/// **The grant file itself is judged**: the owner's regular file, written by
/// no group and no other, within its bound. A row per refusal.
#[test]
fn the_grant_file_is_judged_before_it_is_read() {
    let layout = Layout::new("file");
    let path = layout.0.join("runner/grant");
    std::fs::write(&path, good()).unwrap();
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o644)).unwrap();
    assert!(read_grant_file(&path, own()).is_ok(), "the control reads");

    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o664)).unwrap();
    let got = read_grant_file(&path, own()).unwrap_err();
    assert!(got.contains("writable by its group or others"), "{got}");
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o644)).unwrap();

    let got = read_grant_file(&path, own() + 1).unwrap_err();
    assert!(got.contains(&format!("is owned by uid {}", own())), "{got}");

    let link = layout.0.join("runner/grant-link");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    let got = read_grant_file(&link, own()).unwrap_err();
    assert!(got.contains("does not open"), "{got}");

    let mut long = good();
    long.resize(GRANT_FILE_BOUND as usize + 1, b'\n');
    std::fs::write(&path, long).unwrap();
    let got = read_grant_file(&path, own()).unwrap_err();
    assert!(got.contains("longer than"), "{got}");

    let got = read_grant_file(&layout.0.join("runner"), own()).unwrap_err();
    assert!(got.contains("is not a regular file"), "{got}");
}

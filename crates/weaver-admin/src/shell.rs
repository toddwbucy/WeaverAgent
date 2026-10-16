//! **The shell's deployment** (#29, `weaver-admin-Spec` sections 4, 6 and
//! 9): where the declaration elects `sh`, the inventory judges the root's
//! `shell-binary` and its applet list and builds the shell's grant, the start
//! step builds the run directory's `commands/` from the list, and the run end
//! removes it. The wall is the gate's (`weaver-gate-Spec` section 8.1): this
//! module decides which file it grants and what `PATH` lists.

use std::collections::BTreeSet;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// The run directory's entry that holds one symlink per listed applet.
pub const COMMANDS: &str = "commands";

/// The interpreter's two names, compiled in and never listed.
const INTERPRETER: [&str; 2] = ["sh", "ash"];

/// The ELF program header type that names an interpreter.
const PT_INTERP: u32 = 3;

/// The busybox judged, by its canonical path, and its listed applets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judged {
    pub binary: PathBuf,
    pub applets: Vec<String>,
}

/// The applet list beside the binary, `<shell-binary>.applets`.
pub fn applets_path(binary: &Path) -> PathBuf {
    let mut name = binary.as_os_str().to_owned();
    name.push(".applets");
    PathBuf::from(name)
}

/// **The inventory's judgment of `shell-binary`** (Spec section 4), every
/// failure answering why, which the caller says and refuses
/// `BoundaryUnverified`:
/// - the binary and its applet list are regular files, not links, owned by
///   `owner` and writable by no group or other, their directories closed,
/// - the binary is statically linked, its ELF header carrying no
///   `PT_INTERP`,
/// - the list names each applet once, a plain file name, and never the
///   interpreter,
/// - the binary's own `--list`, run with an empty environment and no input,
///   holds `sh` and `ash`, and the rest equals the list exactly.
pub fn judge(binary: &Path, owner: u32) -> Result<Judged, String> {
    closed_file(binary, owner)?;
    let canonical = crate::judge_ancestors(binary, &[owner, 0])
        .map_err(|_| format!("a directory above {} is not closed", binary.display()))?;
    let list = applets_path(binary);
    closed_file(&list, owner)?;
    crate::judge_ancestors(&list, &[owner, 0])
        .map_err(|_| format!("a directory above {} is not closed", list.display()))?;

    if names_an_interpreter(&elf_headers(&canonical)?)? {
        return Err(format!(
            "{} is dynamically linked (its ELF header names an interpreter), and the shell's \
             wall needs a static binary",
            canonical.display()
        ));
    }

    let text = std::fs::read_to_string(&list)
        .map_err(|e| format!("{} does not read: {e}", list.display()))?;
    let mut applets = Vec::new();
    let mut seen = BTreeSet::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if !is_plain_name(line) || INTERPRETER.contains(&line) {
            return Err(format!(
                "{} lists {line:?}, not an applet's plain name",
                list.display()
            ));
        }
        if !seen.insert(line.to_string()) {
            return Err(format!("{} lists {line} twice", list.display()));
        }
        applets.push(line.to_string());
    }

    compare(&canonical, compiled_set(&canonical)?, &seen)?;
    Ok(Judged {
        binary: canonical,
        applets,
    })
}

/// **The compiled set against the list**: the interpreter's two names
/// compiled in, and the rest equal to the list exactly.
fn compare(
    binary: &Path,
    compiled: BTreeSet<String>,
    listed: &BTreeSet<String>,
) -> Result<(), String> {
    for name in INTERPRETER {
        if !compiled.contains(name) {
            return Err(format!(
                "{} has no {name} compiled in, so every shell call would fail",
                binary.display()
            ));
        }
    }
    let rest: BTreeSet<String> = compiled
        .into_iter()
        .filter(|name| !INTERPRETER.contains(&name.as_str()))
        .collect();
    if &rest != listed {
        let unlisted: Vec<_> = rest.difference(listed).collect();
        let absent: Vec<_> = listed.difference(&rest).collect();
        return Err(format!(
            "{}'s compiled set is not its list: compiled and not listed {unlisted:?}, listed \
             and not compiled {absent:?}",
            binary.display()
        ));
    }
    Ok(())
}

/// A regular file, not a link, owned by `owner` and writable by no group or
/// other.
fn closed_file(path: &Path, owner: u32) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|e| format!("{} does not stand: {e}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    if metadata.uid() != owner || metadata.mode() & 0o022 != 0 {
        return Err(format!(
            "{} is not root's alone, owned by uid {} at mode {:o}",
            path.display(),
            metadata.uid(),
            metadata.mode() & 0o7777
        ));
    }
    Ok(())
}

/// **The ELF header and the program header table, and nothing more**: read
/// from one descriptor opened without following a link, the header first and
/// then the table's span it names, so a judgment never reads the whole
/// binary. The bytes answer laid out as the image has them, zeroes between.
fn elf_headers(binary: &Path) -> Result<Vec<u8>, String> {
    use std::os::unix::fs::{FileExt, OpenOptionsExt};
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(binary)
        .map_err(|e| format!("{} does not open: {e}", binary.display()))?;
    let malformed = || {
        format!(
            "{} is not a 64-bit little-endian ELF image",
            binary.display()
        )
    };
    let mut header = vec![0u8; 64];
    file.read_exact_at(&mut header, 0)
        .map_err(|_| malformed())?;
    if &header[..4] != b"\x7fELF" || header[4] != 2 || header[5] != 1 {
        return Err(malformed());
    }
    let phoff = u64::from_le_bytes(header[32..40].try_into().map_err(|_| malformed())?);
    let span = u64::from(u16::from_le_bytes([header[54], header[55]]))
        .checked_mul(u64::from(u16::from_le_bytes([header[56], header[57]])))
        .ok_or_else(malformed)?;
    // **Every span is checked before it is touched** (Codex on #177): the
    // buffer holds at least the header, a table past 64 KiB is no busybox's
    // and is refused rather than allocated, and each slice is taken with
    // `get_mut`, so no offset a file names can panic this crate.
    let table_end = phoff.checked_add(span).ok_or_else(malformed)?;
    let end = table_end.max(64);
    if end > 1 << 16 {
        return Err(malformed());
    }
    let length = usize::try_from(end).map_err(|_| malformed())?;
    let mut bytes = vec![0u8; length];
    bytes
        .get_mut(..64)
        .ok_or_else(malformed)?
        .copy_from_slice(&header);
    if span > 0 {
        let start = usize::try_from(phoff).map_err(|_| malformed())?;
        let stop = usize::try_from(table_end).map_err(|_| malformed())?;
        let table = bytes.get_mut(start..stop).ok_or_else(malformed)?;
        // A table past the file's end fails the read and refuses.
        file.read_exact_at(table, phoff).map_err(|_| malformed())?;
    }
    Ok(bytes)
}

/// **Whether a 64-bit little-endian ELF image names an interpreter**: its
/// program headers walked for a `PT_INTERP` entry. Anything else refuses.
fn names_an_interpreter(bytes: &[u8]) -> Result<bool, String> {
    let malformed = || "the binary is not a 64-bit little-endian ELF image".to_string();
    if bytes.len() < 64 || &bytes[..4] != b"\x7fELF" || bytes[4] != 2 || bytes[5] != 1 {
        return Err(malformed());
    }
    // The header's fields are read through `get`, so a short image refuses
    // rather than panics.
    let u16_at = |at: usize| -> Result<usize, String> {
        let field = bytes.get(at..at + 2).ok_or_else(malformed)?;
        Ok(u16::from_le_bytes([field[0], field[1]]) as usize)
    };
    let phoff = bytes
        .get(32..40)
        .and_then(|field| field.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or_else(malformed)?;
    let phoff = usize::try_from(phoff).map_err(|_| malformed())?;
    let phentsize = u16_at(54)?;
    let phnum = u16_at(56)?;
    // An executable image has program headers: one that names none is not
    // the static binary the shell needs, and is refused rather than read as
    // static.
    if phentsize < 4 || phnum == 0 {
        return Err(malformed());
    }
    for index in 0..phnum {
        let at = phoff
            .checked_add(index.checked_mul(phentsize).ok_or_else(malformed)?)
            .ok_or_else(malformed)?;
        let field = bytes
            .get(at..at.checked_add(4).ok_or_else(malformed)?)
            .ok_or_else(malformed)?;
        if u32::from_le_bytes(field.try_into().map_err(|_| malformed())?) == PT_INTERP {
            return Ok(true);
        }
    }
    Ok(false)
}

/// An applet's name as a `commands/` entry: no separator, not `.` or `..`.
fn is_plain_name(name: &str) -> bool {
    !name.contains('/') && name != "." && name != ".." && !name.contains('\0')
}

/// The bound on `--list`'s answer: a binary that does not answer within it
/// is killed and refused, so a root-owned program that is not busybox cannot
/// hold the inventory, and the locks it runs under, open.
const LIST_BOUND: std::time::Duration = std::time::Duration::from_secs(10);

/// The cap on `--list`'s output: applet names, one per line, never approach
/// it, and a binary writing past it is refused and killed.
const LIST_CAP: usize = 64 * 1024;

/// The binary's own listing of what is compiled in, run as `busybox --list`
/// (busybox answers `--list` only under a name that starts `busybox`), with
/// an empty environment, no input and no error stream, bounded at
/// [`LIST_BOUND`].
fn compiled_set(binary: &Path) -> Result<BTreeSet<String>, String> {
    compiled_set_within(binary, LIST_BOUND)
}

/// [`compiled_set`] under a named bound, **the whole judgment bounded**: the
/// child runs in a process group of its own and its standard output is read
/// without blocking, so neither the child nor a descendant holding the pipe
/// can hold the inventory, and the locks it runs under, past the bound. The
/// listing is read when the pipe closes and the child has exited, and at the
/// bound the whole group is killed and the listing refused.
fn compiled_set_within(
    binary: &Path,
    bound: std::time::Duration,
) -> Result<BTreeSet<String>, String> {
    use std::io::Read;
    use std::os::fd::AsFd;
    use std::os::unix::process::CommandExt;
    let mut child = std::process::Command::new(binary)
        .arg0("busybox")
        .arg("--list")
        .env_clear()
        .process_group(0)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("{} --list does not run: {e}", binary.display()))?;
    let group = nix::unistd::Pid::from_raw(child.id() as i32);
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let kill_group = |child: &mut std::process::Child| {
        // SAFETY: a signal to the process group this function spawned.
        unsafe {
            nix::libc::kill(-group.as_raw(), nix::libc::SIGKILL);
        }
        let _ = child.wait();
    };
    let nonblocking =
        nix::fcntl::fcntl(stdout.as_fd(), nix::fcntl::FcntlArg::F_GETFL).and_then(|flags| {
            let flags = nix::fcntl::OFlag::from_bits_retain(flags) | nix::fcntl::OFlag::O_NONBLOCK;
            nix::fcntl::fcntl(stdout.as_fd(), nix::fcntl::FcntlArg::F_SETFL(flags))
        });
    if nonblocking.is_err() {
        kill_group(&mut child);
        return Err(format!(
            "{} --list: its pipe would not read without blocking",
            binary.display()
        ));
    }
    let deadline = std::time::Instant::now() + bound;
    let mut text = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut closed = false;
    let mut exited = None;
    // **Every iteration reaches the cap, the exit and the deadline** (Codex
    // on #177): one read per pass, never a loop of reads, so a binary that
    // writes forever meets the bound, and the listing's bytes are capped at
    // `LIST_CAP`, which applet names never approach.
    loop {
        let mut read_any = false;
        if !closed {
            match stdout.read(&mut chunk) {
                Ok(0) => closed = true,
                Ok(n) => {
                    text.extend_from_slice(chunk.get(..n).unwrap_or(&[]));
                    read_any = true;
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => {
                    kill_group(&mut child);
                    return Err(format!("{} --list: {e}", binary.display()));
                }
            }
        }
        if text.len() > LIST_CAP {
            kill_group(&mut child);
            return Err(format!(
                "{} --list wrote past {LIST_CAP} bytes, no listing of applet names",
                binary.display()
            ));
        }
        if exited.is_none() {
            match child.try_wait() {
                Ok(status) => exited = status,
                Err(e) => {
                    kill_group(&mut child);
                    return Err(format!("{} --list: {e}", binary.display()));
                }
            }
        }
        if closed && let Some(status) = exited {
            if !status.success() {
                return Err(format!(
                    "{} answers no listing ({status}): it must be built with CONFIG_BUSYBOX",
                    binary.display()
                ));
            }
            break;
        }
        if std::time::Instant::now() >= deadline {
            kill_group(&mut child);
            return Err(format!(
                "{} --list did not answer within {}ms",
                binary.display(),
                bound.as_millis()
            ));
        }
        if !read_any {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    let text =
        String::from_utf8(text).map_err(|_| format!("{} --list is not text", binary.display()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

/// **The start step's `commands/`** (Spec section 6): a stale one removed
/// first, then made afresh at `0755`, holding one symlink per listed applet
/// to the binary and no entry for the interpreter. Answers its path. A
/// failure answers why, which the caller refuses `BoundaryUnverified`.
pub fn build_commands(run_directory: &Path, judged: &Judged) -> Result<PathBuf, String> {
    use std::os::unix::fs::DirBuilderExt;
    let commands = run_directory.join(COMMANDS);
    remove_commands(run_directory)
        .map_err(|e| format!("a stale {} does not remove: {e}", commands.display()))?;
    std::fs::DirBuilder::new()
        .mode(0o755)
        .create(&commands)
        .map_err(|e| format!("{} cannot be made: {e}", commands.display()))?;
    // The mode set again past the umask, which only narrows the creation's.
    std::fs::set_permissions(
        &commands,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .map_err(|e| format!("{} cannot be set to 0755: {e}", commands.display()))?;
    for applet in &judged.applets {
        std::os::unix::fs::symlink(&judged.binary, commands.join(applet))
            .map_err(|e| format!("{}/{applet} cannot be linked: {e}", commands.display()))?;
    }
    Ok(commands)
}

/// **The run end's removal of `commands/`** where it stands (Spec section
/// 6.1), an absent one read as removed.
pub fn remove_commands(run_directory: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(run_directory.join(COMMANDS)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests;

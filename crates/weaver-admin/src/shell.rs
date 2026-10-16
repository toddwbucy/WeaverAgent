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
    let end = phoff.checked_add(span).ok_or_else(malformed)?;
    // A table past 64 KiB is no busybox's: refused rather than allocated.
    if end > 1 << 16 {
        return Err(malformed());
    }
    let mut bytes = vec![0u8; end as usize];
    bytes[..64].copy_from_slice(&header);
    if span > 0 {
        file.read_exact_at(&mut bytes[phoff as usize..end as usize], phoff)
            .map_err(|_| malformed())?;
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
    let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize;
    let phoff = u64::from_le_bytes(bytes[32..40].try_into().map_err(|_| malformed())?) as usize;
    let phentsize = u16_at(54);
    let phnum = u16_at(56);
    if phentsize < 4 {
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

/// The binary's own listing of what is compiled in, run as `busybox --list`
/// (busybox answers `--list` only under a name that starts `busybox`), with
/// an empty environment, no input and no error stream, bounded at
/// [`LIST_BOUND`].
fn compiled_set(binary: &Path) -> Result<BTreeSet<String>, String> {
    compiled_set_within(binary, LIST_BOUND)
}

/// [`compiled_set`] under a named bound.
fn compiled_set_within(
    binary: &Path,
    bound: std::time::Duration,
) -> Result<BTreeSet<String>, String> {
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    let mut child = std::process::Command::new(binary)
        .arg0("busybox")
        .arg("--list")
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("{} --list does not run: {e}", binary.display()))?;
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let reader = std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = stdout.read_to_end(&mut text);
        text
    });
    let deadline = std::time::Instant::now() + bound;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{} --list did not answer within {}ms",
                    binary.display(),
                    bound.as_millis()
                ));
            }
            Err(e) => return Err(format!("{} --list: {e}", binary.display())),
        }
    };
    let text = reader.join().unwrap_or_default();
    if !status.success() {
        return Err(format!(
            "{} answers no listing ({status}): it must be built with CONFIG_BUSYBOX",
            binary.display()
        ));
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

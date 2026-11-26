//! The grant file, per `weaver-harness-Spec` section 2.5: what the stack's
//! install computed on the machine that runs the loop, read by the launcher
//! and judged before any wall is laid. The enter carries none of it, and
//! admin does not read it.
//!
//! **Its form**, ASCII, one fact per line, each line ended by a newline:
//!
//! ```text
//! weaver-loop-grant 1
//! interpreter <sha256, 64 lowercase hex> <path>
//! execute <path>
//! read <path>
//! beneath <path>
//! pycache <path>
//! ```
//!
//! The first line is the version. `interpreter` and `pycache` stand once
//! each. `execute` names a file executed beside the interpreter (its loader),
//! `read` a file read (a library), `beneath` a folder read and listed beneath
//! (the standard library). Each path is absolute and is the rest of its line,
//! and no path stands twice. Anything else refuses the file, naming the line.

use std::os::fd::OwnedFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use nix::libc;
use sha2::{Digest, Sha256};

use crate::confine::{FS_EXECUTE, FS_READ_DIR, FS_READ_FILE, Rule, open_judged};

/// The grant file's version line.
pub const VERSION_LINE: &str = "weaver-loop-grant 1";

/// The grant file is read whole and at most this long: a computed grant is a
/// few hundred bytes.
pub const GRANT_FILE_BOUND: u64 = 64 << 10;

/// The grant file as read, before its paths are judged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub interpreter: PathBuf,
    pub interpreter_sha256: String,
    pub execute: Vec<PathBuf>,
    pub read: Vec<PathBuf>,
    pub beneath: Vec<PathBuf>,
    pub pycache: PathBuf,
}

/// **The grant file, judged and read**: a regular file, not a link, owned by
/// `owner` (root in the binary), written by no group and no other, at most
/// [`GRANT_FILE_BOUND`] bytes, then parsed. The file is read through the
/// descriptor that was judged.
pub fn read_grant_file(path: &Path, owner: u32) -> Result<Grant, String> {
    use std::io::Read as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|e| format!("the grant file {} does not open: {e}", path.display()))?;
    let meta = file
        .metadata()
        .map_err(|e| format!("the grant file {}: {e}", path.display()))?;
    judge_owned(&meta, owner, false)
        .map_err(|why| format!("the grant file {} {why}", path.display()))?;
    if meta.len() > GRANT_FILE_BOUND {
        return Err(format!(
            "the grant file {} is longer than {GRANT_FILE_BOUND} bytes",
            path.display()
        ));
    }
    let mut bytes = Vec::new();
    file.take(GRANT_FILE_BOUND + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("the grant file {}: {e}", path.display()))?;
    parse(&bytes).map_err(|why| format!("the grant file {}: {why}", path.display()))
}

/// A file or folder `owner` holds, written by no group and no other: `dir`
/// asks for a folder, else a regular file.
fn judge_owned(meta: &std::fs::Metadata, owner: u32, dir: bool) -> Result<(), String> {
    let kind = meta.mode() & libc::S_IFMT;
    if dir && kind != libc::S_IFDIR {
        return Err("is not a directory".to_string());
    }
    if !dir && kind != libc::S_IFREG {
        return Err("is not a regular file".to_string());
    }
    if meta.uid() != owner {
        return Err(format!("is owned by uid {}, not {owner}", meta.uid()));
    }
    if meta.mode() & 0o022 != 0 {
        return Err(format!(
            "is writable by its group or others (mode {:o})",
            meta.mode() & 0o7777
        ));
    }
    Ok(())
}

/// **The grant file's text, parsed**: every refusal names its line.
pub fn parse(bytes: &[u8]) -> Result<Grant, String> {
    if !bytes.is_ascii() {
        return Err("it is not ASCII".to_string());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "it is not ASCII".to_string())?;
    if !text.ends_with('\n') {
        return Err("its last line has no newline, so it may be cut short".to_string());
    }
    let mut lines = text[..text.len() - 1].split('\n').enumerate();
    match lines.next() {
        Some((_, VERSION_LINE)) => {}
        _ => return Err(format!("line 1 is not \"{VERSION_LINE}\"")),
    }
    let mut interpreter: Option<(PathBuf, String)> = None;
    let mut pycache: Option<PathBuf> = None;
    let (mut execute, mut read, mut beneath) = (Vec::new(), Vec::new(), Vec::new());
    let mut seen: Vec<PathBuf> = Vec::new();
    for (index, line) in lines {
        let number = index + 1;
        let (keyword, rest) = line
            .split_once(' ')
            .ok_or_else(|| format!("line {number} has no path"))?;
        let path_text = if keyword == "interpreter" {
            let (digest, path) = rest
                .split_once(' ')
                .ok_or_else(|| format!("line {number} has no digest and path"))?;
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(format!(
                    "line {number}: the digest is not 64 lowercase hex digits"
                ));
            }
            if interpreter.is_some() {
                return Err(format!("line {number} names a second interpreter"));
            }
            interpreter = Some((PathBuf::from(path), digest.to_string()));
            path
        } else {
            rest
        };
        let path = PathBuf::from(path_text);
        if !path.is_absolute() {
            return Err(format!("line {number}: {path_text} is not absolute"));
        }
        if seen.contains(&path) {
            return Err(format!("line {number}: {path_text} is named twice"));
        }
        seen.push(path.clone());
        match keyword {
            "interpreter" => {}
            "execute" => execute.push(path),
            "read" => read.push(path),
            "beneath" => beneath.push(path),
            "pycache" => {
                if pycache.is_some() {
                    return Err(format!("line {number} names a second pycache folder"));
                }
                pycache = Some(path);
            }
            other => return Err(format!("line {number}: \"{other}\" is no grant keyword")),
        }
    }
    let (interpreter, interpreter_sha256) =
        interpreter.ok_or_else(|| "it names no interpreter".to_string())?;
    let pycache = pycache.ok_or_else(|| "it names no pycache folder".to_string())?;
    Ok(Grant {
        interpreter,
        interpreter_sha256,
        execute,
        read,
        beneath,
        pycache,
    })
}

/// The paths a launch starts from, beside the grant.
pub struct Launch<'a> {
    /// The grant file's folder: it holds `runner.py` and the board.
    pub runner_dir: &'a Path,
    /// The agent's loop folder, which holds the wire file.
    pub loop_dir: &'a Path,
}

/// **Every granted path, judged as the shell's grant is judged** (opened
/// `O_PATH` without following a link at its last component, its type read by
/// `fstat` on that descriptor), with the interpreter's sha256 compared with
/// the grant's on the same file. The first path that fails ends the launch,
/// named. Answers the rules the ruleset takes, the interpreter's first.
pub fn judge(grant: &Grant, launch: &Launch, owner: u32) -> Result<Vec<Rule>, String> {
    let regular = |stat: &libc::stat| {
        (stat.st_mode & libc::S_IFMT != libc::S_IFREG).then(|| "is not a regular file".to_string())
    };
    let directory = |stat: &libc::stat| {
        (stat.st_mode & libc::S_IFMT != libc::S_IFDIR).then(|| "is not a directory".to_string())
    };
    let mut rules = Vec::new();
    let interpreter = open_judged(&grant.interpreter, regular)?;
    digest_matches(&grant.interpreter, &interpreter, &grant.interpreter_sha256)?;
    rules.push(rule(
        interpreter,
        FS_EXECUTE | FS_READ_FILE,
        &grant.interpreter,
    ));
    for path in &grant.execute {
        rules.push(rule(
            open_judged(path, regular)?,
            FS_EXECUTE | FS_READ_FILE,
            path,
        ));
    }
    for path in &grant.read {
        rules.push(rule(open_judged(path, regular)?, FS_READ_FILE, path));
    }
    for path in &grant.beneath {
        rules.push(rule(
            open_judged(path, directory)?,
            FS_READ_FILE | FS_READ_DIR,
            path,
        ));
    }
    // **The runner's folder and the pycache folder are the stack's**: owned
    // by `owner`, written by no group and no other. The pycache folder stands
    // empty and is granted nothing, so the interpreter reads no bytecode.
    let owned_dir = |stat: &libc::stat| {
        directory(stat).or_else(|| {
            if stat.st_uid != owner {
                Some(format!("is owned by uid {}, not {owner}", stat.st_uid))
            } else if stat.st_mode & 0o022 != 0 {
                Some("is writable by its group or others".to_string())
            } else {
                None
            }
        })
    };
    rules.push(rule(
        open_judged(launch.runner_dir, owned_dir)?,
        FS_READ_FILE | FS_READ_DIR,
        launch.runner_dir,
    ));
    // **Every file in the runner's folder is judged as the grant file is**:
    // `runner.py`, which must stand, the board beside it and whatever else
    // stands there, each a regular file of `owner`'s, written by no group and
    // no other. The folder is listed through its own judged descriptor.
    let owned_file = |stat: &libc::stat| {
        regular(stat).or_else(|| {
            if stat.st_uid != owner {
                Some(format!("is owned by uid {}, not {owner}", stat.st_uid))
            } else if stat.st_mode & 0o022 != 0 {
                Some("is writable by its group or others".to_string())
            } else {
                None
            }
        })
    };
    open_judged(&launch.runner_dir.join("runner.py"), owned_file)?;
    for name in list_judged(launch.runner_dir, owned_dir)? {
        open_judged(&launch.runner_dir.join(name), owned_file)?;
    }
    open_judged(&grant.pycache, owned_dir)?;
    let mut entries = std::fs::read_dir(&grant.pycache)
        .map_err(|e| format!("{} does not list: {e}", grant.pycache.display()))?;
    if entries.next().is_some() {
        return Err(format!("{} is not empty", grant.pycache.display()));
    }
    // The loop folder's owner and mode are admin's checks (section 2). The
    // launcher takes it as a directory that is not a link.
    rules.push(rule(
        open_judged(launch.loop_dir, directory)?,
        FS_READ_FILE | FS_READ_DIR,
        launch.loop_dir,
    ));
    Ok(rules)
}

/// **The names in a directory, read through a descriptor opened for reading
/// without following a link** and judged by `fault` on that descriptor before
/// any name is read, so the listing is of the directory judged. `.` and `..`
/// are left out.
fn list_judged(
    path: &Path,
    fault: impl Fn(&libc::stat) -> Option<String>,
) -> Result<Vec<std::ffi::OsString>, String> {
    use std::os::fd::IntoRawFd as _;
    use std::os::unix::ffi::OsStrExt as _;
    let fd = nix::fcntl::open(
        path,
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_DIRECTORY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )
    .map_err(|errno| format!("{} does not open to list: {errno}", path.display()))?;
    let stat = nix::sys::stat::fstat(&fd)
        .map_err(|errno| format!("{}: fstat: {errno}", path.display()))?;
    if let Some(why) = fault(&stat) {
        return Err(format!("{} {why}", path.display()));
    }
    let raw = fd.into_raw_fd();
    // SAFETY: `raw` is an open directory descriptor this function owns; the
    // stream takes it, and `closedir` below closes both.
    let stream = unsafe { libc::fdopendir(raw) };
    if stream.is_null() {
        let errno = nix::errno::Errno::last();
        // SAFETY: the stream did not take `raw`, which is still ours to close.
        unsafe { libc::close(raw) };
        return Err(format!("{} does not list: {errno}", path.display()));
    }
    let mut names = Vec::new();
    let read = loop {
        nix::errno::Errno::clear();
        // SAFETY: a live stream from `fdopendir`, read on this thread alone.
        let entry = unsafe { libc::readdir(stream) };
        if entry.is_null() {
            let errno = nix::errno::Errno::last_raw();
            break if errno == 0 {
                Ok(())
            } else {
                Err(nix::errno::Errno::from_raw(errno))
            };
        }
        // SAFETY: `readdir` answers an entry whose name is a NUL-ended string,
        // valid until the next call on this stream.
        let name = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) };
        let bytes = name.to_bytes();
        if bytes != b"." && bytes != b".." {
            names.push(std::ffi::OsStr::from_bytes(bytes).to_owned());
        }
    };
    // SAFETY: the stream from `fdopendir`, closed once.
    unsafe { libc::closedir(stream) };
    read.map_err(|errno| format!("{} does not list: {errno}", path.display()))?;
    Ok(names)
}

fn rule(fd: OwnedFd, access: u64, path: &Path) -> Rule {
    Rule {
        fd,
        access,
        path: path.to_path_buf(),
    }
}

/// **The interpreter's sha256, read from the file the rule names**: opened
/// for reading without following a link, its device and inode compared with
/// the judged descriptor's, so the digest is the granted file's. A file
/// replaced after its rule is a new inode, which the domain does not grant,
/// so its exec is refused.
fn digest_matches(path: &Path, judged: &OwnedFd, want: &str) -> Result<(), String> {
    use std::io::Read as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|e| format!("{} does not open for its digest: {e}", path.display()))?;
    let meta = file
        .metadata()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let stat =
        nix::sys::stat::fstat(judged).map_err(|e| format!("{}: fstat: {e}", path.display()))?;
    if meta.dev() != stat.st_dev as u64 || meta.ino() != stat.st_ino {
        return Err(format!("{} changed while it was judged", path.display()));
    }
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let got: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if got != want {
        return Err(format!(
            "{} has sha256 {got}, not the grant's {want}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;

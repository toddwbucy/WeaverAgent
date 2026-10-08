//! Admin's side of the save points, per `weaver-admin-Spec` sections 3, 4
//! and 6 on the operator's rulings of 2026-10-06 on #1 (the A3.0 items):
//! the publication of the member's finished save points into the operator's
//! directory under a root-owned manifest, the selection of the save point a
//! load restores and the descriptor the member inherits, the judgment of a
//! save point's bytes, and the clean-unload marker.
//!
//! **This crate links no engine**, so the save-point format it judges is
//! parsed here from the bytes alone, per `weaver-state-Spec` section 3: a
//! stamp line, a check line, and the image; the digest is sha256 over the
//! whole file; the published name is the stamp's wall clock in UTC before the
//! digest. What only the member can judge, the schema against the one it
//! runs, it answers on the enter's `restored` ask.

use std::io::{Read, Write};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::{Path, PathBuf};

use sha2::Digest;
use weaver_types::{LifecycleRefusal, SavePointReport};

/// The manifest's name in the territory's `save-points/`.
pub const MANIFEST: &str = "save-points.manifest";
/// The marker's name in the agent's config root.
pub const MARKER: &str = "run.marker";
/// The published name's suffix, the room's own.
pub const SUFFIX: &str = ".save-point";
/// The bound on a save point's size this crate reads whole, one gibibyte,
/// this act's election: a file past it is left in place and named, never
/// read into root's memory, since the room is the member's to write and the
/// directory the operator's.
pub const SAVE_POINT_BOUND: u64 = 1024 * 1024 * 1024;

/// **Open a directory once, to be held as a descriptor**, per
/// `weaver-admin-Spec` section 9: the path is used here and nowhere after,
/// a link at it refused, so every step that follows reaches the directory
/// as it was judged and never what the path resolves to later.
pub fn open_directory(path: &Path) -> std::io::Result<OwnedFd> {
    Ok(nix::fcntl::open(
        path,
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_DIRECTORY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )?)
}

/// Sync a directory's entries, so a rename or a creation in it is durable
/// before anything that depends on it is written, per `weaver-admin-Spec`
/// section 6.
fn sync_directory(directory: BorrowedFd<'_>) -> std::io::Result<()> {
    nix::unistd::fsync(directory)?;
    Ok(())
}

/// Sync a directory named by path: the config root's, this crate's own
/// root-owned directory where the marker lives, per Spec section 4, which
/// no other principal can swap.
fn sync_path(directory: &Path) -> std::io::Result<()> {
    sync_directory(open_directory(directory)?.as_fd())
}

/// The names the directory holds, read through its descriptor.
fn list_directory(directory: BorrowedFd<'_>) -> Vec<String> {
    let Ok(duplicate) = nix::unistd::dup(directory) else {
        return Vec::new();
    };
    let Ok(mut dir) = nix::dir::Dir::from_fd(duplicate) else {
        return Vec::new();
    };
    dir.iter()
        .flatten()
        .filter_map(|entry| entry.file_name().to_str().ok().map(str::to_string))
        .filter(|name| name != "." && name != "..")
        .collect()
}

/// Whether an entry of the name stands in the directory, a link included.
fn entry_stands(directory: BorrowedFd<'_>, name: &str) -> bool {
    nix::sys::stat::fstatat(directory, name, nix::fcntl::AtFlags::AT_SYMLINK_NOFOLLOW).is_ok()
}

/// Who the manifest belongs to: root in production, passed as the expected
/// owner so the judgment below is one comparison the tests can run as the
/// operator, planting a file the expected owner does not own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owner {
    pub uid: u32,
    pub gid: u32,
}

/// The production owner, this crate running as root.
pub const ROOT: Owner = Owner { uid: 0, gid: 0 };

/// **The manifest is judged on its descriptor before a byte is read or
/// written**, per `weaver-admin-Spec` section 6: a regular file, the expected
/// owner's uid and gid, mode `0644` exactly, link count one. The directory is
/// the operator's, who can pre-create or replace the entry, so an entry the
/// operator owns refuses by uid, a second link refuses by count, and a link
/// never opens, `O_NOFOLLOW` refusing it before this is reached; what is
/// found is named.
fn judge_manifest(file: &std::fs::File, owner: Owner) -> Result<(), LifecycleRefusal> {
    use std::os::unix::fs::MetadataExt;
    let refuse = |what: &str| {
        diag!("weaver-admin: the manifest {MANIFEST} {what}");
        LifecycleRefusal::BoundaryUnverified
    };
    let metadata = file.metadata().map_err(|_| refuse("does not stat"))?;
    if !metadata.is_file() {
        return Err(refuse("is not a regular file"));
    }
    if metadata.uid() != owner.uid {
        return Err(refuse(&format!(
            "is owned by uid {} and not by {}, so it is not this crate's own",
            metadata.uid(),
            owner.uid
        )));
    }
    if metadata.gid() != owner.gid {
        return Err(refuse(&format!(
            "has gid {} and not {}",
            metadata.gid(),
            owner.gid
        )));
    }
    if metadata.mode() & 0o7777 != 0o644 {
        return Err(refuse(&format!(
            "has mode {:04o} and not 0644",
            metadata.mode() & 0o7777
        )));
    }
    if metadata.nlink() != 1 {
        return Err(refuse(&format!(
            "has {} links and a manifest has one",
            metadata.nlink()
        )));
    }
    Ok(())
}

/// Open the manifest for reading through `O_NOFOLLOW`, judged: `None` where
/// no entry stands.
fn open_manifest(
    directory: BorrowedFd<'_>,
    owner: Owner,
) -> Result<Option<std::fs::File>, LifecycleRefusal> {
    let file = match nix::fcntl::openat(
        directory,
        MANIFEST,
        nix::fcntl::OFlag::O_RDONLY | nix::fcntl::OFlag::O_NOFOLLOW | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    ) {
        Ok(fd) => std::fs::File::from(fd),
        Err(nix::errno::Errno::ENOENT) => return Ok(None),
        Err(nix::errno::Errno::ELOOP) => {
            diag!("weaver-admin: the manifest {MANIFEST} is a link, which is never the manifest");
            return Err(LifecycleRefusal::BoundaryUnverified);
        }
        Err(e) => {
            diag!("weaver-admin: the manifest {MANIFEST} does not open: {e}");
            return Err(LifecycleRefusal::BoundaryUnverified);
        }
    };
    judge_manifest(&file, owner)?;
    Ok(Some(file))
}

/// How a manifest line arrived, per `weaver-admin-Spec` section 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    /// The leave's save point, named on `Left`.
    Leave,
    /// A save point on demand, named on `SavePointTaken`.
    Demand,
    /// A finished file the room still held at a load, after an unclean stop.
    Recovered,
    /// A file the operator named through the `restore` verb.
    Restore,
}

impl Arrival {
    fn word(self) -> &'static str {
        match self {
            Arrival::Leave => "leave",
            Arrival::Demand => "demand",
            Arrival::Recovered => "recovered",
            Arrival::Restore => "restore",
        }
    }

    fn parse(word: &str) -> Option<Arrival> {
        Some(match word {
            "leave" => Arrival::Leave,
            "demand" => Arrival::Demand,
            "recovered" => Arrival::Recovered,
            "restore" => Arrival::Restore,
            _ => return None,
        })
    }
}

/// A save point's stamp as this crate reads it from the stamp line: the
/// position it covers, the schema digest it stands under, and the wall clock
/// it was taken at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamp {
    pub run: String,
    pub sequence: u64,
    pub turn: u64,
    pub schema: String,
    pub wall_ns: u64,
}

/// A judged save point: its stamp and its digest, the bytes having passed
/// the check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judged {
    pub stamp: Stamp,
    pub digest: String,
}

/// One manifest line, per `weaver-admin-Spec` section 6.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestLine {
    pub ordinal: u64,
    pub digest: String,
    pub name: String,
    pub stamp: Stamp,
    /// The trace position of the `save_point` event that named it, the run
    /// and the sequence, where the harness reported it.
    pub position: Option<(String, u64)>,
    pub arrived: Arrival,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// **Judge a save point's bytes**, per `weaver-state-Spec` section 3: the
/// stamp line's seven members, the check line, the check over the stamp, its
/// newline and the image, and the image's length as the stamp states it.
/// Answers the stamp and the digest, or why the bytes are not a save point.
pub fn judge(bytes: &[u8]) -> Result<Judged, String> {
    let first = bytes
        .iter()
        .position(|&b| b == b'\n')
        .ok_or("no stamp line")?;
    let header = &bytes[..first];
    let rest = &bytes[first + 1..];
    let second = rest
        .iter()
        .position(|&b| b == b'\n')
        .ok_or("no check line")?;
    let check_line = &rest[..second];
    let image = &rest[second + 1..];
    let stamp: serde_json::Value = serde_json::from_slice(header)
        .map_err(|e| format!("the stamp line does not parse: {e}"))?;
    let object = stamp.as_object().ok_or("the stamp line is not an object")?;
    if object.len() != 7 {
        return Err(format!(
            "the stamp line carries {} members and the format has seven",
            object.len()
        ));
    }
    if object.get("weaver-save-point").and_then(|v| v.as_u64()) != Some(1) {
        return Err("the stamp line is not version 1 of the format".into());
    }
    let run = object
        .get("run")
        .and_then(|v| v.as_str())
        .ok_or("the stamp names no run")?
        .to_string();
    let sequence = object
        .get("sequence")
        .and_then(|v| v.as_u64())
        .ok_or("the stamp names no sequence")?;
    let turn = object
        .get("turn")
        .and_then(|v| v.as_u64())
        .ok_or("the stamp names no turn")?;
    let schema = object
        .get("schema")
        .and_then(|v| v.as_str())
        .ok_or("the stamp names no schema")?
        .to_string();
    let length = object
        .get("image")
        .and_then(|v| v.as_u64())
        .ok_or("the stamp names no image length")?;
    let taken = object
        .get("taken")
        .and_then(|v| v.as_object())
        .ok_or("the stamp names no taken")?;
    // **The nonce is judged as the member judges it**: exactly `pid` and
    // `ordinal` as numbers and `wall_ns` as a string, so a save point this
    // crate admits is one the member's own parse admits at the opener, and
    // a named restore never passes here to refuse there.
    if taken.len() != 3 {
        return Err("the stamp's taken does not carry exactly three members".into());
    }
    if taken.get("pid").and_then(|v| v.as_u64()).is_none() {
        return Err("the stamp's taken names no pid".into());
    }
    if taken.get("ordinal").and_then(|v| v.as_u64()).is_none() {
        return Err("the stamp's taken names no ordinal".into());
    }
    let wall_ns = taken
        .get("wall_ns")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
        // **Digits that fit an unsigned 64-bit count of nanoseconds**, the
        // member's rule read here too (Codex on #94, round 5, the corpus's
        // `nonce-wall-clock-overlong` case): what a clock can be.
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or("the stamp's taken names no wall clock")?;
    if image.len() as u64 != length {
        return Err(format!(
            "the image is {} bytes and the stamp says {length}",
            image.len()
        ));
    }
    // **The check line is the member's canonical rendering, byte for
    // byte**: `{"check":"<hex>"}` and nothing else, as the member's parse
    // compares it, so whitespace or a member beside it refuses here as
    // there.
    let mut hasher = sha2::Sha256::new();
    hasher.update(header);
    hasher.update(b"\n");
    hasher.update(image);
    let canonical = serde_json::json!({"check": hex(&hasher.finalize())}).to_string();
    if check_line != canonical.as_bytes() {
        return Err(
            "the check line is not the check over the bytes in its canonical rendering".into(),
        );
    }
    Ok(Judged {
        stamp: Stamp {
            run,
            sequence,
            turn,
            schema,
            wall_ns,
        },
        digest: hex(&sha2::Sha256::digest(bytes)),
    })
}

/// The published name, computable from the bytes alone, per
/// `weaver-admin-Spec` section 6: the stamp's wall clock in UTC to the second
/// before the digest.
pub fn published_name(judged: &Judged) -> String {
    let seconds = (judged.stamp.wall_ns / 1_000_000_000) as i64;
    format!("{}-{}{SUFFIX}", utc_stamp(seconds), judged.digest)
}

/// `YYYYMMDDTHHMMSSZ` for a count of seconds since the epoch, the civil
/// date by the proleptic Gregorian calendar, with no library.
fn utc_stamp(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Civil-from-days, Howard Hinnant's algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}{m:02}{d:02}T{hh:02}{mm:02}{ss:02}Z")
}

/// Whether a name is the room's finished form, `<digest>.save-point` with a
/// 64-hex digest: a part is dotted and refused here, and so is anything else.
pub fn is_finished_name(name: &str) -> bool {
    name.strip_suffix(SUFFIX)
        .is_some_and(|digest| digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()))
}

// ------------------------------------------------------------- the manifest

fn render_line(line: &ManifestLine) -> String {
    let mut object = serde_json::json!({
        "ordinal": line.ordinal,
        "digest": line.digest,
        "name": line.name,
        "stamp": {
            "run": line.stamp.run,
            "sequence": line.stamp.sequence,
            "turn": line.stamp.turn,
            "schema": line.stamp.schema,
        },
        "taken": {"wall_ns": line.stamp.wall_ns.to_string()},
        "arrived": line.arrived.word(),
    });
    if let Some((run, sequence)) = &line.position {
        object["position"] = serde_json::json!({"run": run, "sequence": sequence});
    }
    format!("{object}\n")
}

fn parse_line(text: &str) -> Option<ManifestLine> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let stamp = value.get("stamp")?;
    Some(ManifestLine {
        ordinal: value.get("ordinal")?.as_u64()?,
        digest: value.get("digest")?.as_str()?.to_string(),
        name: value.get("name")?.as_str()?.to_string(),
        stamp: Stamp {
            run: stamp.get("run")?.as_str()?.to_string(),
            sequence: stamp.get("sequence")?.as_u64()?,
            turn: stamp.get("turn")?.as_u64()?,
            schema: stamp.get("schema")?.as_str()?.to_string(),
            wall_ns: value.get("taken")?.get("wall_ns")?.as_str()?.parse().ok()?,
        },
        position: match value.get("position") {
            None => None,
            Some(position) => Some((
                position.get("run")?.as_str()?.to_string(),
                position.get("sequence")?.as_u64()?,
            )),
        },
        arrived: Arrival::parse(value.get("arrived")?.as_str()?)?,
    })
}

/// **Reads at most `bound` bytes**, the read stopping one byte past it: the
/// bytes where the source ended within the bound, `None` where it reached
/// past, so a file another principal grows while it is read never makes
/// this root process read without limit.
fn read_within(source: &mut impl Read, bound: u64) -> std::io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    source.take(bound + 1).read_to_end(&mut bytes)?;
    Ok((bytes.len() as u64 <= bound).then_some(bytes))
}

/// **Read the manifest**, per `weaver-admin-Spec` section 4: absent is an
/// empty manifest; one that does not read, or a line that does not parse,
/// refuses `BoundaryUnverified` naming it, since what is loadable can then
/// not be said.
pub fn read_manifest(
    directory: BorrowedFd<'_>,
    owner: Owner,
) -> Result<Vec<ManifestLine>, LifecycleRefusal> {
    let Some(mut file) = open_manifest(directory, owner)? else {
        return Ok(Vec::new());
    };
    let mut text = String::new();
    if let Err(e) = file.read_to_string(&mut text) {
        diag!("weaver-admin: the manifest {MANIFEST} does not read: {e}");
        return Err(LifecycleRefusal::BoundaryUnverified);
    }
    // **A last line without its newline is a torn append**, left by a write
    // or a power loss that ended mid-line: it names no save point, it is
    // dropped and named, and the next append truncates it away, so the
    // publication it was for is retried through the adoption of its target
    // rather than blocking every load behind a line nobody finished.
    let whole = match text.rfind('\n') {
        Some(end) => &text[..=end],
        None if text.is_empty() => "",
        None => {
            diag!("weaver-admin: the manifest {MANIFEST} ends in a torn line, which is dropped");
            ""
        }
    };
    if whole.len() < text.len() && !whole.is_empty() {
        diag!("weaver-admin: the manifest {MANIFEST} ends in a torn line, which is dropped");
    }
    let mut lines = Vec::new();
    for (at, line) in whole.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let Some(parsed) = parse_line(line) else {
            diag!(
                "weaver-admin: the manifest {MANIFEST} does not parse at line {}",
                at + 1
            );
            return Err(LifecycleRefusal::BoundaryUnverified);
        };
        lines.push(parsed);
    }
    Ok(lines)
}

/// The next ordinal: one past the highest line standing, whether or not that
/// line's file still stands, so an ordinal is never minted twice.
pub fn next_ordinal(lines: &[ManifestLine]) -> u64 {
    lines.iter().map(|line| line.ordinal).max().unwrap_or(0) + 1
}

/// **Append one line**, the manifest root-owned and mode `0644`, opened for
/// appending alone and never rewritten.
fn append_line(
    directory: BorrowedFd<'_>,
    owner: Owner,
    line: &ManifestLine,
) -> Result<(), LifecycleRefusal> {
    let refuse = |what: String| {
        diag!("weaver-admin: the manifest {MANIFEST} {what}");
        LifecycleRefusal::BoundaryUnverified
    };
    let mut file = if !entry_stands(directory, MANIFEST) {
        // **Created exclusively, mode 0644, and the directory synced**, so a
        // manifest exists only where this crate made it and the entry is
        // durable before the line is; through the directory's descriptor,
        // per Spec section 9, so it is made in the directory judged.
        let fd = nix::fcntl::openat(
            directory,
            MANIFEST,
            nix::fcntl::OFlag::O_RDWR
                | nix::fcntl::OFlag::O_CREAT
                | nix::fcntl::OFlag::O_EXCL
                | nix::fcntl::OFlag::O_NOFOLLOW
                | nix::fcntl::OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::from_bits_truncate(0o644),
        )
        .map_err(|e| refuse(format!("does not create: {e}")))?;
        let file = std::fs::File::from(fd);
        nix::sys::stat::fchmod(
            file.as_fd(),
            nix::sys::stat::Mode::from_bits_truncate(0o644),
        )
        .map_err(|e| refuse(format!("does not take mode 0644: {e}")))?;
        nix::unistd::fsync(directory)
            .map_err(|e| refuse(format!("'s directory does not sync: {e}")))?;
        file
    } else {
        // Read as well as append: the tail is read back and a torn one
        // truncated before the line goes, and a failed write is rolled back.
        match nix::fcntl::openat(
            directory,
            MANIFEST,
            nix::fcntl::OFlag::O_RDWR
                | nix::fcntl::OFlag::O_APPEND
                | nix::fcntl::OFlag::O_NOFOLLOW
                | nix::fcntl::OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::empty(),
        ) {
            Ok(fd) => std::fs::File::from(fd),
            Err(nix::errno::Errno::ELOOP) => {
                return Err(refuse("is a link, which is never the manifest".to_string()));
            }
            Err(e) => return Err(refuse(format!("does not open for appending: {e}"))),
        }
    };
    judge_manifest(&file, owner)?;
    // **A torn tail is truncated before the line goes**, and **a write that
    // fails part way is rolled back to the length it found**, so the
    // manifest holds whole lines or nothing of a failed one.
    let prior = (|| -> std::io::Result<u64> {
        let mut tail = Vec::new();
        file.read_to_end(&mut tail)?;
        let whole = match tail.iter().rposition(|&b| b == b'\n') {
            Some(end) => end as u64 + 1,
            None => 0,
        };
        if whole < tail.len() as u64 {
            nix::unistd::ftruncate(file.as_fd(), whole as i64)?;
            file.sync_all()?;
        }
        Ok(whole)
    })()
    .map_err(|e| refuse(format!("does not read back before the append: {e}")))?;
    let rendered = render_line(line);
    let written = file
        .write_all(rendered.as_bytes())
        .and_then(|()| file.sync_all());
    if let Err(e) = written {
        let _ = nix::unistd::ftruncate(file.as_fd(), prior as i64);
        let _ = file.sync_all();
        return Err(refuse(format!("does not take a line: {e}")));
    }
    Ok(())
}

// ------------------------------------------------------------ publication

/// A finished save point found in the member's room, judged.
struct RoomEntry {
    name: String,
    bytes: Vec<u8>,
    judged: Judged,
}

/// **Read the member's room through its own descriptor**, per
/// `weaver-admin-Spec` section 6: each entry opened beneath the directory
/// descriptor without following links, a regular file owned by the member's
/// uid under a finished name, its bytes judged; anything else is left in
/// place and named.
fn read_room(room: &Path, member_uid: u32) -> Option<(OwnedFd, Vec<RoomEntry>)> {
    use std::os::unix::fs::MetadataExt;
    let dir = nix::fcntl::open(
        room,
        nix::fcntl::OFlag::O_RDONLY | nix::fcntl::OFlag::O_DIRECTORY | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )
    .ok()?;
    // The listing by path yields names alone; every open and every removal
    // below goes through the descriptor, so a component the member swapped
    // under the path between the two cannot be followed.
    let listing = std::fs::read_dir(room).ok()?;
    let mut found = Vec::new();
    for entry in listing.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_finished_name(&name) {
            continue;
        }
        let Ok(fd) = nix::fcntl::openat(
            dir.as_fd(),
            name.as_str(),
            nix::fcntl::OFlag::O_RDONLY
                | nix::fcntl::OFlag::O_NOFOLLOW
                | nix::fcntl::OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::empty(),
        ) else {
            diag!("weaver-admin: the room's {name} does not open and is left in place");
            continue;
        };
        let mut file = std::fs::File::from(fd);
        let Ok(metadata) = file.metadata() else {
            continue;
        };
        if !metadata.is_file() || metadata.uid() != member_uid {
            diag!(
                "weaver-admin: the room's {name} is not the member's regular file and is left in place"
            );
            continue;
        }
        if metadata.len() > SAVE_POINT_BOUND {
            diag!(
                "weaver-admin: the room's {name} is {} bytes, past the bound of {SAVE_POINT_BOUND}, and is left in place",
                metadata.len()
            );
            continue;
        }
        // **The bound holds through the read** (Codex on #94): the member
        // owns this file and may still be running at a `save-point`, so it
        // can grow the file after the length above was read; the read stops
        // one byte past the bound and a file that reached it is refused as
        // the length check refuses it.
        let bytes = match read_within(&mut file, SAVE_POINT_BOUND) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => {
                diag!(
                    "weaver-admin: the room's {name} grew past the bound of {SAVE_POINT_BOUND} as it was read, and is left in place"
                );
                continue;
            }
            Err(_) => continue,
        };
        match judge(&bytes) {
            Ok(judged) if format!("{}{SUFFIX}", judged.digest) == name => found.push(RoomEntry {
                name,
                bytes,
                judged,
            }),
            Ok(_) => diag!("weaver-admin: the room's {name} is not the file its name claims"),
            Err(why) => diag!("weaver-admin: the room's {name} is not a save point ({why})"),
        }
    }
    // The listing's order is the filesystem's and means nothing; by name
    // it is the same on every box, and `publish` orders by the clock.
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Some((dir, found))
}

/// **Publish the member's finished save points into the operator's
/// directory**, per `weaver-admin-Spec` section 6, under the lock the caller
/// holds: each is copied under a temporary name owned by the operator and
/// mode `0600`, renamed to its published name, named on one manifest line,
/// and only then removed from the room. `reports` are what the harness
/// reported of the save points it recorded, so a file the report names
/// carries the event's position and the report's arrival, and any other file
/// is a recovered one. Answers the lines appended.
pub fn publish(
    room: &Path,
    member_uid: u32,
    directory: BorrowedFd<'_>,
    operator: (u32, u32),
    owner: Owner,
    reports: &[(SavePointReport, Arrival)],
) -> Result<Vec<ManifestLine>, LifecycleRefusal> {
    let mut lines = read_manifest(directory, owner)?;
    let mut appended = Vec::new();
    let Some((room_dir, mut entries)) = read_room(room, member_uid) else {
        return Ok(appended);
    };
    // **Several entries publish recovered first, then reported, the clock
    // ordering within a kind** (Codex on #94, rounds 5 and 6), per Spec
    // section 6: a reported save point was taken last by construction, so
    // it is minted last whatever the clock did between, a clock stepped
    // back included; within a kind `taken.wall_ns` ascending orders them,
    // the digest as the tiebreak, because the member's own count,
    // `taken.ordinal`, is per process and restarts with it, and the clock
    // is monotonic enough across processes for one agent's files. So the
    // latest the manifest names is the last taken, never a recovered older
    // file the listing happened to yield later.
    let reported = |entry: &RoomEntry| {
        reports
            .iter()
            .any(|(report, _)| report.save_point == entry.judged.digest)
    };
    entries.sort_by(|a, b| {
        reported(a)
            .cmp(&reported(b))
            .then_with(|| a.judged.stamp.wall_ns.cmp(&b.judged.stamp.wall_ns))
            .then_with(|| a.judged.digest.cmp(&b.judged.digest))
    });
    let remove_from_room = |name: &str| {
        let _ = nix::unistd::unlinkat(
            room_dir.as_fd(),
            name,
            nix::unistd::UnlinkatFlags::NoRemoveDir,
        );
    };
    for entry in entries {
        let (position, arrived) = reports
            .iter()
            .find(|(report, _)| report.save_point == entry.judged.digest)
            .map(|(report, arrival)| {
                // The manifest's position is the event's: its run and its
                // sequence, never the covered position's run beside the
                // event's sequence (Codex on #94, round 6).
                (
                    Some((report.event_run.0.clone(), report.position)),
                    *arrival,
                )
            })
            .unwrap_or((None, Arrival::Recovered));
        let name = published_name(&entry.judged);
        // **A line standing for the room's copy is judged before the copy
        // goes** (Codex on #94, round 9): the copy goes only where the line's
        // target stands and digests to the line; a target gone is recreated
        // beneath the standing line, the line being the record and no second
        // one appended; a target of other bytes is left with the copy, which
        // is then the last sound copy, and named in the log.
        let standing = lines
            .iter()
            .find(|line| line.digest == entry.judged.digest)
            .cloned();
        if let Some(line) = &standing {
            match open_published(directory, line, operator.0) {
                Ok(Some(_)) => {
                    remove_from_room(&entry.name);
                    continue;
                }
                Ok(None) => diag!(
                    "weaver-admin: the manifest names {}, which is gone; the room's copy recreates it under the standing line",
                    line.name
                ),
                Err(why) => {
                    diag!(
                        "weaver-admin: the manifest names {}, which is not the file its line says ({why}); the room's copy stays",
                        line.name
                    );
                    continue;
                }
            }
        }
        // **Every step goes through the directory's descriptor** (Codex on
        // #94, round 7), per Spec section 9: the temporary is made, renamed
        // and the directory synced against the descriptor opened at the
        // judgment, so a directory the operator swaps under the path between
        // steps is not followed and a link put at the path has this root
        // process create nothing where it points.
        let temporary = format!(".publishing-{}", entry.judged.digest);
        let remove_temporary = || {
            let _ = nix::unistd::unlinkat(
                directory,
                temporary.as_str(),
                nix::unistd::UnlinkatFlags::NoRemoveDir,
            );
        };
        remove_temporary();
        let written = (|| -> std::io::Result<()> {
            let fd = nix::fcntl::openat(
                directory,
                temporary.as_str(),
                nix::fcntl::OFlag::O_WRONLY
                    | nix::fcntl::OFlag::O_CREAT
                    | nix::fcntl::OFlag::O_EXCL
                    | nix::fcntl::OFlag::O_NOFOLLOW
                    | nix::fcntl::OFlag::O_CLOEXEC,
                nix::sys::stat::Mode::from_bits_truncate(0o640),
            )?;
            let mut file = std::fs::File::from(fd);
            // **The mode is set through the descriptor, whatever the umask**
            // (Codex on #94, round 8): the open's mode is narrowed by the
            // invoking shell's umask, and the copy must be exactly 0640.
            nix::sys::stat::fchmod(
                file.as_fd(),
                nix::sys::stat::Mode::from_bits_truncate(0o640),
            )
            .map_err(std::io::Error::from)?;
            file.write_all(&entry.bytes)?;
            nix::unistd::fchown(
                file.as_fd(),
                Some(nix::unistd::Uid::from_raw(operator.0)),
                Some(nix::unistd::Gid::from_raw(operator.1)),
            )
            .map_err(std::io::Error::from)?;
            // Synced after the ownership change, so the owner is as durable
            // as the bytes before the entry is named anywhere.
            file.sync_all()?;
            let metadata = file.metadata()?;
            use std::os::unix::fs::MetadataExt;
            if metadata.uid() != operator.0 || metadata.mode() & 0o777 != 0o640 {
                return Err(std::io::Error::other(
                    "the copy is not root's 0640, read by the access group",
                ));
            }
            // **The rename replaces nothing**: an entry the operator put
            // under the published name, a link among them, refuses the
            // rename rather than being replaced or followed, the temporary
            // then removed and the room's copy left for the next verb.
            match nix::fcntl::renameat2(
                directory,
                temporary.as_str(),
                directory,
                name.as_str(),
                nix::fcntl::RenameFlags::RENAME_NOREPLACE,
            ) {
                // **The directory is synced before the line names the entry**,
                // so a manifest line never outlives its target across a power
                // loss, and the room's copy goes only after both are durable.
                Ok(()) => sync_directory(directory),
                // **An entry already under the published name is adopted
                // where it is this save point**: a publication interrupted
                // between the rename and the manifest line left it, the
                // retry at the next verb appends the line; judged through
                // the directory as a load judges one, so an entry of other
                // bytes refuses the rename and is left in place, named.
                Err(nix::errno::Errno::EEXIST) => {
                    remove_temporary();
                    match open_judged(directory, &name, operator.0) {
                        Ok(Some((_, standing))) if standing.digest == entry.judged.digest => Ok(()),
                        Ok(Some(_)) => Err(std::io::Error::other(
                            "an entry of other bytes stands under the published name",
                        )),
                        Ok(None) => Err(std::io::Error::other(
                            "the published name's entry vanished between the rename and its judgment",
                        )),
                        Err(why) => Err(std::io::Error::other(format!(
                            "an entry stands under the published name and is not this save point: {why}"
                        ))),
                    }
                }
                Err(e) => Err(std::io::Error::from(e)),
            }
        })();
        if let Err(e) = written {
            diag!(
                "weaver-admin: the room's {} did not publish ({e}) and is left in place",
                entry.name
            );
            remove_temporary();
            continue;
        }
        if standing.is_some() {
            // The target stands again under its line; nothing to append.
            remove_from_room(&entry.name);
            continue;
        }
        let line = ManifestLine {
            ordinal: next_ordinal(&lines),
            digest: entry.judged.digest.clone(),
            name,
            stamp: entry.judged.stamp.clone(),
            position,
            arrived,
        };
        append_line(directory, owner, &line)?;
        lines.push(line.clone());
        appended.push(line);
        // Only now the member's copy goes, through the room's descriptor, so
        // a publication cut short leaves it standing and is retried at the
        // next verb.
        remove_from_room(&entry.name);
    }
    Ok(appended)
}

// -------------------------------------------------------------- selection

/// The save point a load restores, selected and judged: its open descriptor,
/// the line that names it, and the lineage the enter carries.
pub struct Selected {
    pub descriptor: OwnedFd,
    pub line: ManifestLine,
    pub lineage: weaver_types::Lineage,
}

/// Open and judge one file of the territory's save-points directory through
/// its descriptor, per `weaver-admin-Spec` section 4: a regular
/// file, not a link, the operator's, closed to group and other, under the
/// size bound, whose bytes judge sound and whose published name is the one
/// its bytes compute. `Ok(None)` is no entry. The file is answered rewound.
fn open_judged(
    directory: BorrowedFd<'_>,
    name: &str,
    operator: u32,
) -> Result<Option<(std::fs::File, Judged)>, String> {
    use std::os::unix::fs::MetadataExt;
    let fd = match nix::fcntl::openat(
        directory,
        name,
        nix::fcntl::OFlag::O_RDONLY | nix::fcntl::OFlag::O_NOFOLLOW | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(nix::errno::Errno::ENOENT) => return Ok(None),
        Err(e) => return Err(format!("{name} does not open: {e}")),
    };
    let mut file = std::fs::File::from(fd);
    let metadata = file
        .metadata()
        .map_err(|e| format!("{name} does not stat: {e}"))?;
    if !metadata.is_file() {
        return Err(format!("{name} is not a regular file"));
    }
    if metadata.uid() != operator {
        return Err(format!("{name} is not root's"));
    }
    if metadata.mode() & 0o7777 != 0o640 {
        return Err(format!(
            "{name} is not mode 0640, root's and read by the access group"
        ));
    }
    if metadata.len() > SAVE_POINT_BOUND {
        return Err(format!(
            "{name} is {} bytes, past the bound of {SAVE_POINT_BOUND}",
            metadata.len()
        ));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| format!("{name} does not read: {e}"))?;
    let judged = judge(&bytes).map_err(|why| format!("{name} is not a save point: {why}"))?;
    if published_name(&judged) != name {
        return Err(format!("{name} is not the name its bytes compute"));
    }
    nix::unistd::lseek(file.as_fd(), 0, nix::unistd::Whence::SeekSet)
        .map_err(|e| format!("{name} does not seek: {e}"))?;
    Ok(Some((file, judged)))
}

/// Open and judge the file a manifest line names, per `weaver-admin-Spec`
/// section 4: `open_judged`, and the bytes digest to the line's digest.
fn open_published(
    directory: BorrowedFd<'_>,
    line: &ManifestLine,
    operator: u32,
) -> Result<Option<OwnedFd>, String> {
    let Some((file, judged)) = open_judged(directory, &line.name, operator)? else {
        return Ok(None);
    };
    if judged.digest != line.digest {
        return Err(format!("{} does not digest to its line", line.name));
    }
    Ok(Some(OwnedFd::from(file)))
}

/// **Select the save point a load restores**, per `weaver-admin-Spec`
/// section 4: the one `restore` names by its published name or its digest,
/// which must have a manifest line, or the latest, the line of highest
/// ordinal whose file stands and judges to its digest, a line whose file is
/// gone or differs being passed over. `Ok(None)` is no save point: an empty
/// manifest with nothing named.
pub fn select(
    directory: BorrowedFd<'_>,
    operator: u32,
    restore: Option<&str>,
    owner: Owner,
) -> Result<Option<Selected>, LifecycleRefusal> {
    let lines = read_manifest(directory, owner)?;
    // **A manifest absent beside published files refuses**, per Spec
    // section 4: what is loadable cannot be said, and the files are not
    // loadable without it; no manifest and no file is the first load.
    if lines.is_empty()
        && !entry_stands(directory, MANIFEST)
        && list_directory(directory)
            .iter()
            .any(|name| name.ends_with(SUFFIX))
    {
        diag!(
            "weaver-admin: the territory's save-points directory holds save points and no {MANIFEST}; a file the manifest does not name is not loadable, and the restore verb is what names one"
        );
        return Err(LifecycleRefusal::BoundaryUnverified);
    }
    let refuse_config = || LifecycleRefusal::ConfigInvalid {
        field: Some(weaver_types::FieldName("restore".into())),
    };
    let lineage_of = |line: &ManifestLine| weaver_types::Lineage {
        save_point: line.digest.clone(),
        run: weaver_types::RunId(line.stamp.run.clone()),
        sequence: line.stamp.sequence,
        turn: line.stamp.turn,
        named_at_restore: line.arrived == Arrival::Restore,
        built_from: None,
    };
    if let Some(named) = restore {
        let Some(line) = lines
            .iter()
            .rev()
            .find(|line| line.name == named || line.digest == named)
        else {
            diag!(
                "weaver-admin: restore names {named}, which no manifest line names; a file the manifest does not name is not loadable, and the restore verb is what names one"
            );
            return Err(refuse_config());
        };
        return match open_published(directory, line, operator) {
            Ok(Some(descriptor)) => Ok(Some(Selected {
                descriptor,
                lineage: lineage_of(line),
                line: line.clone(),
            })),
            Ok(None) => {
                diag!(
                    "weaver-admin: restore names {named}, whose file is gone from the save-points directory"
                );
                Err(LifecycleRefusal::BoundaryUnverified)
            }
            Err(why) => {
                diag!("weaver-admin: the save point restore names refuses: {why}");
                Err(refuse_config())
            }
        };
    }
    let mut ordered: Vec<&ManifestLine> = lines.iter().collect();
    ordered.sort_by_key(|line| std::cmp::Reverse(line.ordinal));
    for line in ordered {
        match open_published(directory, line, operator) {
            Ok(Some(descriptor)) => {
                return Ok(Some(Selected {
                    descriptor,
                    lineage: lineage_of(line),
                    line: line.clone(),
                }));
            }
            Ok(None) => {
                diag!(
                    "weaver-admin: the manifest's ordinal {} names {}, which is gone; passed over",
                    line.ordinal,
                    line.name
                );
            }
            Err(why) => {
                diag!(
                    "weaver-admin: the manifest's ordinal {} is not the latest: {why}",
                    line.ordinal
                );
            }
        }
    }
    Ok(None)
}

/// **Name a save point at a restore**, the `restore` verb's judgment, per
/// `weaver-admin-Spec` section 4: a file the manifest already names answers
/// its line; otherwise the file is judged as a load judges one and a line
/// naming it is appended, marked as arrived by restore, with no position.
pub fn name_at_restore(
    directory: BorrowedFd<'_>,
    operator: u32,
    named: &str,
    owner: Owner,
) -> Result<ManifestLine, LifecycleRefusal> {
    let lines = read_manifest(directory, owner)?;
    let refuse_config = || LifecycleRefusal::ConfigInvalid {
        field: Some(weaver_types::FieldName("restore".into())),
    };
    // **A listed name is judged before the verb answers** (Codex on #94,
    // round 4), per Spec section 4: the operator owns the published file
    // and it may have gone or changed since its line, so the verb opens
    // and judges it as the load does, refusing as the load would, and
    // `RestoreNamed` means named and judged loadable now.
    if let Some(line) = lines
        .iter()
        .rev()
        .find(|line| line.name == named || line.digest == named)
    {
        return match open_published(directory, line, operator) {
            Ok(Some(_)) => Ok(line.clone()),
            Ok(None) => {
                diag!(
                    "weaver-admin: restore names {named}, whose file is gone from the save-points directory"
                );
                Err(LifecycleRefusal::BoundaryUnverified)
            }
            Err(why) => {
                diag!("weaver-admin: the save point restore names refuses: {why}");
                Err(refuse_config())
            }
        };
    }
    let candidate = ManifestLine {
        ordinal: next_ordinal(&lines),
        digest: String::new(),
        name: named.to_string(),
        stamp: Stamp {
            run: String::new(),
            sequence: 0,
            turn: 0,
            schema: String::new(),
            wall_ns: 0,
        },
        position: None,
        arrived: Arrival::Restore,
    };
    // **A digest names the file that computes to it**: a name of the
    // published form is opened as written, and a bare digest finds the one
    // entry `<stamp>-<digest>.save-point` in the directory, so an unlisted
    // file is loadable by either, as the declaration may name it.
    let entry_name = if named.ends_with(SUFFIX) {
        named.to_string()
    } else {
        let wanted = format!("-{named}{SUFFIX}");
        let mut matches: Vec<String> = list_directory(directory)
            .into_iter()
            .filter(|name| name.ends_with(&wanted))
            .collect();
        matches.sort();
        match matches.as_slice() {
            [one] => one.clone(),
            [] => {
                diag!(
                    "weaver-admin: restore names the digest {named}, and no published file in the save-points directory carries it"
                );
                return Err(refuse_config());
            }
            several => {
                diag!(
                    "weaver-admin: restore names the digest {named}, which {} files carry; name one by its published name",
                    several.len()
                );
                return Err(refuse_config());
            }
        }
    };
    let named = entry_name.as_str();
    let mut probe = candidate.clone();
    probe.name = named.to_string();
    // The file is judged through the directory as a load judges one; the
    // digest and the stamp are its own, and the name must be the one its
    // bytes compute, so a renamed file does not enter.
    let judged = match open_judged(directory, named, operator) {
        Ok(Some((_, judged))) => judged,
        Ok(None) => {
            diag!(
                "weaver-admin: restore names {named}, which does not stand in the save-points directory"
            );
            return Err(refuse_config());
        }
        Err(why) => {
            diag!("weaver-admin: restore names {named}, which refuses: {why}");
            return Err(refuse_config());
        }
    };
    if published_name(&judged) != named {
        diag!(
            "weaver-admin: restore names {named}, and its bytes compute {}; a renamed save point does not enter",
            published_name(&judged)
        );
        return Err(refuse_config());
    }
    probe.digest = judged.digest;
    probe.stamp = judged.stamp;
    append_line(directory, owner, &probe)?;
    Ok(probe)
}

// ------------------------------------------------------------- the marker

/// What the marker says of the last run, per `weaver-admin-Spec` section 4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Marker {
    /// The run stands open: no clean unload closed it.
    Open { run: String },
    /// A clean unload closed it.
    Closed { run: String },
    /// `force-unload` ended it without its leave save point.
    Forced { run: String },
}

/// Read the marker, `None` where none stands or it does not read as one.
pub fn read_marker(root: &Path) -> Option<Marker> {
    let text = std::fs::read_to_string(root.join(MARKER)).ok()?;
    let value: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    let run = value.get("run")?.as_str()?.to_string();
    Some(match value.get("state")?.as_str()? {
        "open" => Marker::Open { run },
        "closed" => Marker::Closed { run },
        "forced" => Marker::Forced { run },
        _ => return None,
    })
}

/// Write the marker whole, root-owned, through a temporary name and a rename,
/// or remove it where `None` is written, which restores an absent prior
/// state in a rollback.
pub fn write_marker(root: &Path, marker: Option<&Marker>) -> std::io::Result<()> {
    let path = root.join(MARKER);
    let Some(marker) = marker else {
        return match std::fs::remove_file(&path) {
            Ok(()) => sync_path(root),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        };
    };
    let (state, run) = match marker {
        Marker::Open { run } => ("open", run),
        Marker::Closed { run } => ("closed", run),
        Marker::Forced { run } => ("forced", run),
    };
    // **The marker is durable before it is reported**: the temporary is
    // written and synced, renamed into place, and the root synced, so the
    // marker survives the loss of power it exists to record.
    let temporary = root.join(".run.marker.new");
    {
        let mut file = std::fs::File::create(&temporary)?;
        // **Root's, 0644, whatever the umask** (Codex on #94, round 10): the
        // create takes the invoking shell's umask, and a permissive one
        // would leave the marker writable in the 0755 root, so the mode is
        // set through the descriptor before the sync.
        nix::sys::stat::fchmod(
            file.as_fd(),
            nix::sys::stat::Mode::from_bits_truncate(0o644),
        )
        .map_err(std::io::Error::from)?;
        file.write_all(
            format!("{}\n", serde_json::json!({"run": run, "state": state})).as_bytes(),
        )?;
        file.sync_all()?;
    }
    std::fs::rename(&temporary, &path)?;
    sync_path(root)
}

/// The reset a marker resolves, per `weaver-admin-Spec` section 4: an open
/// run is `NoCleanUnload`, a forced one `ForcedUnload`, a closed one or none
/// no reset.
pub fn reset_from(marker: Option<&Marker>) -> Option<weaver_types::Reset> {
    match marker {
        Some(Marker::Open { run }) => Some(weaver_types::Reset {
            prior_run: weaver_types::RunId(run.clone()),
            reason: weaver_types::ResetReason::NoCleanUnload,
        }),
        Some(Marker::Forced { run }) => Some(weaver_types::Reset {
            prior_run: weaver_types::RunId(run.clone()),
            reason: weaver_types::ResetReason::ForcedUnload,
        }),
        Some(Marker::Closed { .. }) | None => None,
    }
}

/// The member's room under its territory root, `<sink directory>/state/`,
/// per `weaver-state-Spec` section 2.
pub fn room_of(territory_root: &Path) -> PathBuf {
    territory_root.join("state")
}

#[cfg(test)]
mod tests {
    /// **The size bound holds through the read** (Codex on #94): a source
    /// that ends within the bound reads whole, one that reaches past it is
    /// refused, however long it runs, as a room file its member grows after
    /// the length check would, and reads no more of it than one byte past
    /// the bound. The bound is a parameter here so the test needs no
    /// gibibyte. Perturbation: read without the `take` and the whole long
    /// source is pulled into memory.
    #[test]
    fn a_read_past_the_bound_is_refused_however_long_the_source() {
        let bound = 16u64;
        let mut within = std::io::Cursor::new(vec![7u8; 16]);
        assert_eq!(
            read_within(&mut within, bound).unwrap(),
            Some(vec![7u8; 16])
        );
        let mut past = std::io::Cursor::new(vec![7u8; 17]);
        assert_eq!(read_within(&mut past, bound).unwrap(), None);
        // A source far longer than the bound, as a file its member keeps
        // growing: the read takes at most one byte past the bound.
        struct Counting {
            left: u64,
            pulled: u64,
        }
        impl Read for Counting {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                let n = (buffer.len() as u64).min(self.left) as usize;
                buffer[..n].fill(7);
                self.left -= n as u64;
                self.pulled += n as u64;
                Ok(n)
            }
        }
        let mut long = Counting {
            left: 1 << 20,
            pulled: 0,
        };
        assert_eq!(read_within(&mut long, bound).unwrap(), None);
        assert!(
            long.pulled <= bound + 1,
            "read {} bytes of a source past a bound of {bound}",
            long.pulled
        );
    }

    use super::*;

    /// A save point in the member's format, built here as the member builds
    /// one, so the judgment below reads what the member writes.
    pub(crate) fn save_point(
        run: &str,
        sequence: u64,
        turn: u64,
        wall_ns: u64,
        image: &[u8],
    ) -> Vec<u8> {
        let header = serde_json::json!({
            "weaver-save-point": 1,
            "run": run,
            "sequence": sequence,
            "turn": turn,
            "schema": "0".repeat(64),
            "image": image.len(),
            "taken": {"pid": 1, "ordinal": 0, "wall_ns": wall_ns.to_string()},
        })
        .to_string();
        let mut hasher = sha2::Sha256::new();
        hasher.update(header.as_bytes());
        hasher.update(b"\n");
        hasher.update(image);
        let check = serde_json::json!({"check": hex(&hasher.finalize())}).to_string();
        let mut out = header.into_bytes();
        out.push(b'\n');
        out.extend_from_slice(check.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(image);
        out
    }

    /// **The judgment reads the member's format and refuses what is not
    /// it**, per `weaver-state-Spec` section 3: a sound file answers its
    /// stamp and digest, and a torn image, a flipped byte, a stamp with a
    /// member missing and a check that does not hold each refuse naming
    /// why. Perturbation: skip the check's comparison and the flipped byte
    /// judges sound.
    #[test]
    fn the_judgment_reads_the_format_and_refuses_damage() {
        let bytes = save_point("r-1", 41, 2, 1_759_788_000_000_000_000, b"image bytes");
        let judged = judge(&bytes).expect("sound");
        assert_eq!(
            (
                judged.stamp.run.as_str(),
                judged.stamp.sequence,
                judged.stamp.turn
            ),
            ("r-1", 41, 2)
        );
        assert_eq!(judged.digest, hex(&sha2::Sha256::digest(&bytes)));
        assert_eq!(
            published_name(&judged),
            format!("20251006T220000Z-{}.save-point", judged.digest)
        );
        let mut flipped = bytes.clone();
        let last = flipped.len() - 1;
        flipped[last] ^= 1;
        assert!(judge(&flipped).unwrap_err().contains("check"));
        assert!(
            judge(&bytes[..bytes.len() - 3])
                .unwrap_err()
                .contains("bytes")
        );
        let short_stamp = bytes.to_vec();
        let header_end = short_stamp.iter().position(|&b| b == b'\n').unwrap();
        let mut header: serde_json::Value =
            serde_json::from_slice(&short_stamp[..header_end]).unwrap();
        header.as_object_mut().unwrap().remove("turn");
        let mut rebuilt = header.to_string().into_bytes();
        rebuilt.extend_from_slice(&short_stamp[header_end..]);
        assert!(judge(&rebuilt).unwrap_err().contains("seven"));
        // The nonce is judged as the member judges it: three members of the
        // wrong names, or a pid that is not a number, refuse (Codex on #94,
        // round 1). Perturbation: drop the pid and ordinal checks and both
        // judge sound.
        for taken in [
            r#"{"x":1,"y":2,"wall_ns":"1"}"#,
            r#"{"pid":"one","ordinal":0,"wall_ns":"1"}"#,
        ] {
            let mut header: serde_json::Value =
                serde_json::from_slice(&bytes[..header_end]).unwrap();
            header["taken"] = serde_json::from_str(taken).unwrap();
            let mut rebuilt = header.to_string().into_bytes();
            rebuilt.extend_from_slice(&bytes[header_end..]);
            let why = judge(&rebuilt).unwrap_err();
            assert!(why.contains("taken"), "{taken}: {why}");
        }
    }

    /// **Several room entries publish in the clock's order**, per Spec
    /// section 6 (Codex on #94, round 5): an older file left unreported and
    /// a newer one reported in the same verb take their ordinals by
    /// `taken.wall_ns`, so the newer is the latest the manifest names
    /// whatever order the room lists them in. The newer's digest is chosen
    /// to sort first, which is the order the room reader yields.
    /// Perturbation: drop the sort in `publish` and the older file is
    /// minted last and selected as latest.
    #[test]
    fn several_room_entries_publish_in_the_clocks_order() {
        let scratch = crate::scratch::Scratch(
            std::env::temp_dir().join(format!("weaver-admin-order-{}", std::process::id())),
        );
        let base = scratch.0.clone();
        let room = base.join("room");
        let dir = base.join("decl");
        std::fs::create_dir_all(&room).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let dir_fd = open_directory(&dir).unwrap();
        let me = nix::unistd::getuid().as_raw();
        let mine = Owner {
            uid: me,
            gid: nix::unistd::getgid().as_raw(),
        };
        let owner = (me, mine.gid);
        let older = save_point("r-1", 7, 1, 5_000_000_000, b"older, unreported");
        let older_digest = judge(&older).unwrap().digest;
        let (newer, newer_digest) = (0u32..)
            .map(|i| {
                let bytes = save_point("r-1", 9, 2, 6_000_000_000, format!("newer {i}").as_bytes());
                let digest = judge(&bytes).unwrap().digest;
                (bytes, digest)
            })
            .find(|(_, digest)| digest < &older_digest)
            .expect("a newer file whose digest sorts first");
        std::fs::write(room.join(format!("{newer_digest}{SUFFIX}")), &newer).unwrap();
        std::fs::write(room.join(format!("{older_digest}{SUFFIX}")), &older).unwrap();
        let report = weaver_types::SavePointReport {
            save_point: newer_digest.clone(),
            name: format!("{newer_digest}{SUFFIX}"),
            run: weaver_types::RunId("r-1".into()),
            sequence: 9,
            turn: 2,
            event_run: weaver_types::RunId("r-2".into()),
            position: 9,
        };
        let lines = publish(
            &room,
            me,
            dir_fd.as_fd(),
            owner,
            mine,
            &[(report, Arrival::Demand)],
        )
        .unwrap();
        let minted: Vec<(u64, &str, Arrival)> = lines
            .iter()
            .map(|line| (line.ordinal, line.digest.as_str(), line.arrived))
            .collect();
        assert_eq!(
            minted,
            vec![
                (1, older_digest.as_str(), Arrival::Recovered),
                (2, newer_digest.as_str(), Arrival::Demand),
            ],
            "ordinals follow the clock, the reported one last"
        );
        let latest = select(dir_fd.as_fd(), me, None, mine)
            .unwrap()
            .expect("a latest");
        assert_eq!(latest.line.digest, newer_digest);
        // The manifest's position is the event's run and sequence, never
        // the covered position's run (Codex on #94, round 6). Perturbation:
        // write `report.run` into the position and this reads r-1.
        assert_eq!(lines[1].position, Some(("r-2".into(), 9)));
        // **A reported save point is last whatever the clock did** (Codex
        // on #94, round 6): a recovered file whose clock is ahead of the
        // reported one's, as a clock stepped back between them leaves it,
        // is still minted below. Perturbation: sort by the clock alone and
        // the reported one is minted first, the recovered selected latest.
        let room = base.join("room-2");
        let dir = base.join("decl-2");
        std::fs::create_dir_all(&room).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let dir_fd = open_directory(&dir).unwrap();
        let ahead = save_point("r-1", 3, 1, 8_000_000_000, b"recovered, clock ahead");
        let ahead_digest = judge(&ahead).unwrap().digest;
        let behind = save_point("r-1", 4, 1, 7_000_000_000, b"reported, clock behind");
        let behind_digest = judge(&behind).unwrap().digest;
        std::fs::write(room.join(format!("{ahead_digest}{SUFFIX}")), &ahead).unwrap();
        std::fs::write(room.join(format!("{behind_digest}{SUFFIX}")), &behind).unwrap();
        let report = weaver_types::SavePointReport {
            save_point: behind_digest.clone(),
            name: format!("{behind_digest}{SUFFIX}"),
            run: weaver_types::RunId("r-1".into()),
            sequence: 4,
            turn: 1,
            event_run: weaver_types::RunId("r-1".into()),
            position: 4,
        };
        let lines = publish(
            &room,
            me,
            dir_fd.as_fd(),
            owner,
            mine,
            &[(report, Arrival::Demand)],
        )
        .unwrap();
        let minted: Vec<(u64, &str)> = lines
            .iter()
            .map(|line| (line.ordinal, line.digest.as_str()))
            .collect();
        assert_eq!(
            minted,
            vec![(1, ahead_digest.as_str()), (2, behind_digest.as_str())],
            "the reported one is last though its clock is behind"
        );
        let latest = select(dir_fd.as_fd(), me, None, mine)
            .unwrap()
            .expect("a latest");
        assert_eq!(latest.line.digest, behind_digest);
    }

    /// **A publication interrupted after the rename is adopted at the next
    /// verb**, per `weaver-admin-Spec` section 6's retry: the target stands
    /// under its published name with no manifest line, and the next
    /// publication judges it as this save point, appends the line and removes
    /// the room's copy; an entry of other bytes under that name refuses and
    /// the room's copy stays. Perturbation: treat `EEXIST` as a failure again
    /// and the first case never gets its line.
    #[test]
    fn an_interrupted_publication_is_adopted_and_an_impostor_is_not() {
        let scratch = crate::scratch::Scratch(
            std::env::temp_dir().join(format!("weaver-admin-adopt-{}", std::process::id())),
        );
        let base = scratch.0.clone();
        let room = base.join("room");
        let dir = base.join("decl");
        std::fs::create_dir_all(&room).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let dir_fd = open_directory(&dir).unwrap();
        let me = nix::unistd::getuid().as_raw();
        let mine = Owner {
            uid: me,
            gid: nix::unistd::getgid().as_raw(),
        };
        let owner = (me, mine.gid);
        let bytes = save_point("r-1", 7, 1, 5_000_000_000, b"interrupted");
        let judged = judge(&bytes).unwrap();
        let room_name = format!("{}{SUFFIX}", judged.digest);
        std::fs::write(room.join(&room_name), &bytes).unwrap();
        // The target stands already, as the interrupted rename left it.
        let target = dir.join(published_name(&judged));
        std::fs::write(&target, &bytes).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
        }
        let lines = publish(&room, me, dir_fd.as_fd(), owner, mine, &[]).unwrap();
        assert_eq!(
            lines.len(),
            1,
            "the line is appended for the adopted target"
        );
        assert_eq!(lines[0].digest, judged.digest);
        assert!(!room.join(&room_name).exists(), "the room's copy went");
        // An impostor under the published name: other bytes, the room's
        // copy stays and no line is appended.
        let other = save_point("r-1", 8, 1, 6_000_000_000, b"other");
        let other_judged = judge(&other).unwrap();
        std::fs::write(
            room.join(format!("{}{SUFFIX}", other_judged.digest)),
            &other,
        )
        .unwrap();
        let impostor = dir.join(published_name(&other_judged));
        std::fs::write(&impostor, b"not those bytes").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&impostor, std::fs::Permissions::from_mode(0o640)).unwrap();
        }
        let lines = publish(&room, me, dir_fd.as_fd(), owner, mine, &[]).unwrap();
        assert!(lines.is_empty(), "no line for the impostor's name");
        assert!(
            room.join(format!("{}{SUFFIX}", other_judged.digest))
                .exists(),
            "the room's copy stays for the next verb"
        );
        // **A line standing for the room's copy is judged before the copy
        // goes** (Codex on #94, round 9): the first save point's line stands;
        // its room copy put back and its target removed, the publication
        // recreates the target under the standing line, appends nothing and
        // removes the copy; the copy put back and the target replaced by
        // other bytes, the copy stays and no line is appended. Perturbation:
        // remove the copy on the digest alone and the first case loses the
        // last sound copy with the target still gone.
        std::fs::write(room.join(&room_name), &bytes).unwrap();
        std::fs::remove_file(&target).unwrap();
        let lines = publish(&room, me, dir_fd.as_fd(), owner, mine, &[]).unwrap();
        assert!(lines.is_empty(), "no second line for a standing one");
        assert!(target.exists(), "the target is recreated under its line");
        assert!(
            !room.join(&room_name).exists(),
            "the copy went once the target stood"
        );
        assert_eq!(read_manifest(dir_fd.as_fd(), mine).unwrap().len(), 1);
        std::fs::write(room.join(&room_name), &bytes).unwrap();
        std::fs::write(&target, b"damaged").unwrap();
        let lines = publish(&room, me, dir_fd.as_fd(), owner, mine, &[]).unwrap();
        assert!(lines.is_empty());
        assert!(room.join(&room_name).exists(), "the last sound copy stays");
        std::fs::write(&target, &bytes).unwrap();
    }

    /// **The corpus holds the two readers equal**, per `weaver-admin-Spec`
    /// section 4: every file of the workspace's save-point corpus judges to
    /// the verdict the corpus gives it, so a case this reader admits and the
    /// member's refuses, or the reverse, fails here or in the member's own
    /// corpus test. Perturbation: relax any one rule of `judge` and its case
    /// judges sound against a `refuses` verdict.
    #[test]
    fn the_corpus_holds_the_judgment_equal_to_the_members_parse() {
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../weaver-types/tests/fixtures/save-points");
        let verdicts = std::fs::read_to_string(corpus.join("verdicts.txt")).expect("the verdicts");
        let mut cases = 0;
        for line in verdicts.lines().filter(|l| !l.trim().is_empty()) {
            let (name, verdict) = line.split_once(' ').expect("name and verdict");
            let bytes = std::fs::read(corpus.join(name)).expect(name);
            let judged = judge(&bytes);
            match verdict {
                "sound" => assert!(judged.is_ok(), "{name} judges sound: {judged:?}"),
                "refuses" => assert!(judged.is_err(), "{name} refuses"),
                other => panic!("{name}: verdict {other} is not sound or refuses"),
            }
            cases += 1;
        }
        assert!(cases >= 10, "the corpus holds its cases: {cases}");
    }

    /// **The civil date is the epoch's own**: known instants render as the
    /// UTC stamps the name carries.
    #[test]
    fn the_utc_stamp_renders_known_instants() {
        assert_eq!(utc_stamp(0), "19700101T000000Z");
        assert_eq!(utc_stamp(1_759_788_000), "20251006T220000Z");
        assert_eq!(utc_stamp(951_782_400), "20000229T000000Z");
    }

    /// **The manifest's lines round-trip and the latest is read from it**,
    /// per `weaver-admin-Spec` sections 4 and 6: a line whose file is gone
    /// is passed over, one whose file's bytes do not digest to its line is
    /// not the latest, the ordinal is one past the highest line standing
    /// even when that line's file is gone, a file no line names is not
    /// loadable by name, and a manifest that does not parse refuses
    /// `BoundaryUnverified`. Perturbations: drop the digest comparison in
    /// `open_published` and the altered file is selected; mint the ordinal
    /// from the files standing and it repeats.
    #[test]
    fn the_latest_is_read_from_the_manifest_and_a_file_it_does_not_name_is_not_loadable() {
        let scratch = crate::scratch::Scratch(
            std::env::temp_dir().join(format!("weaver-admin-manifest-{}", std::process::id())),
        );
        let dir = scratch.0.clone();
        std::fs::create_dir_all(&dir).unwrap();
        let dir_fd = open_directory(&dir).unwrap();
        let me = nix::unistd::getuid().as_raw();
        let mine = Owner {
            uid: me,
            gid: nix::unistd::getgid().as_raw(),
        };
        let mut lines = Vec::new();
        for (sequence, wall) in [
            (1u64, 1_000_000_000u64),
            (2, 2_000_000_000),
            (3, 3_000_000_000),
        ] {
            let bytes = save_point("r-1", sequence, 0, wall, b"img");
            let judged = judge(&bytes).unwrap();
            let name = published_name(&judged);
            std::fs::write(dir.join(&name), &bytes).unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir.join(&name), std::fs::Permissions::from_mode(0o640))
                .unwrap();
            let line = ManifestLine {
                ordinal: next_ordinal(&lines),
                digest: judged.digest,
                name,
                stamp: judged.stamp,
                position: Some(("r-1".into(), sequence + 10)),
                arrived: if sequence == 2 {
                    Arrival::Restore
                } else {
                    Arrival::Leave
                },
            };
            append_line(dir_fd.as_fd(), mine, &line).unwrap();
            lines.push(line);
        }
        assert_eq!(
            read_manifest(dir_fd.as_fd(), mine).unwrap(),
            lines,
            "the lines round-trip"
        );
        let latest = select(dir_fd.as_fd(), me, None, mine)
            .unwrap()
            .expect("a latest");
        assert_eq!(latest.line.ordinal, 3);
        assert!(!latest.lineage.named_at_restore);
        // The third's file goes: the second is latest, named at a restore,
        // and the next ordinal is still four.
        std::fs::remove_file(dir.join(&lines[2].name)).unwrap();
        let latest = select(dir_fd.as_fd(), me, None, mine)
            .unwrap()
            .expect("a latest");
        assert_eq!(latest.line.ordinal, 2);
        assert!(latest.lineage.named_at_restore);
        assert_eq!(
            next_ordinal(&read_manifest(dir_fd.as_fd(), mine).unwrap()),
            4
        );
        // The second's file holds other sound bytes, a save point whose digest
        // is not its line's: not the latest, the first is.
        std::fs::write(
            dir.join(&lines[1].name),
            save_point("r-1", 2, 0, 2_000_000_000, b"other image"),
        )
        .unwrap();
        let latest = select(dir_fd.as_fd(), me, None, mine)
            .unwrap()
            .expect("a latest");
        assert_eq!(latest.line.ordinal, 1);
        // A file no line names is not loadable by name.
        let stray = save_point("r-9", 9, 0, 9_000_000_000, b"stray");
        let stray_name = published_name(&judge(&stray).unwrap());
        std::fs::write(dir.join(&stray_name), &stray).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                dir.join(&stray_name),
                std::fs::Permissions::from_mode(0o640),
            )
            .unwrap();
        }
        assert!(matches!(
            select(dir_fd.as_fd(), me, Some(&stray_name), mine),
            Err(LifecycleRefusal::ConfigInvalid { .. })
        ));
        // Named at a restore by its bare digest, the unlisted file is found
        // under its published name (Codex on #94, round 2), enters with the
        // next ordinal and is then loadable by name and by digest; a digest
        // no file carries refuses. Perturbation: open the digest as a name
        // and the first call refuses.
        let stray_digest = judge(&stray).unwrap().digest;
        assert!(name_at_restore(dir_fd.as_fd(), me, &"0".repeat(64), mine).is_err());
        let named = name_at_restore(dir_fd.as_fd(), me, &stray_digest, mine).unwrap();
        assert_eq!(named.name, stray_name);
        assert_eq!((named.ordinal, named.arrived), (4, Arrival::Restore));
        let by_name = select(dir_fd.as_fd(), me, Some(&stray_name), mine)
            .unwrap()
            .unwrap();
        assert_eq!(by_name.line.ordinal, 4);
        let by_digest = select(dir_fd.as_fd(), me, Some(&named.digest), mine)
            .unwrap()
            .unwrap();
        assert!(by_digest.lineage.named_at_restore);
        // **A listed name is judged at the verb** (Codex on #94, round 4):
        // the file holding other sound bytes refuses as the load would, gone
        // it refuses the boundary, and standing again it answers its line.
        // Perturbation: answer the listed line unjudged and the first two
        // calls answer.
        std::fs::write(
            dir.join(&stray_name),
            save_point("r-9", 9, 0, 9_000_000_001, b"swapped"),
        )
        .unwrap();
        assert!(matches!(
            name_at_restore(dir_fd.as_fd(), me, &stray_name, mine),
            Err(LifecycleRefusal::ConfigInvalid { .. })
        ));
        std::fs::rename(dir.join(&stray_name), dir.join("aside")).unwrap();
        assert!(matches!(
            name_at_restore(dir_fd.as_fd(), me, &stray_digest, mine),
            Err(LifecycleRefusal::BoundaryUnverified)
        ));
        std::fs::rename(dir.join("aside"), dir.join(&stray_name)).unwrap();
        std::fs::write(dir.join(&stray_name), &stray).unwrap();
        assert_eq!(
            name_at_restore(dir_fd.as_fd(), me, &stray_name, mine)
                .unwrap()
                .ordinal,
            4
        );
        // A renamed file does not enter.
        std::fs::rename(
            dir.join(&stray_name),
            dir.join("20200101T000000Z-aa.save-point"),
        )
        .unwrap();
        assert!(
            name_at_restore(dir_fd.as_fd(), me, "20200101T000000Z-aa.save-point", mine).is_err()
        );
        // **The manifest is judged as the owner's**: against another expected
        // owner the one that stands reads as the operator's and refuses, read
        // and appended, its planted lines selecting nothing; a second link
        // refuses by count; a symbolic link refuses as a link; the owner's
        // own reads. Perturbation: drop the uid comparison and the first
        // case reads, its lines selecting the files they name.
        let other = Owner {
            uid: me.wrapping_add(1),
            gid: mine.gid,
        };
        assert!(matches!(
            read_manifest(dir_fd.as_fd(), other),
            Err(LifecycleRefusal::BoundaryUnverified)
        ));
        assert!(matches!(
            select(dir_fd.as_fd(), me, None, other),
            Err(LifecycleRefusal::BoundaryUnverified)
        ));
        assert!(matches!(
            append_line(dir_fd.as_fd(), other, &lines[0]),
            Err(LifecycleRefusal::BoundaryUnverified)
        ));
        std::fs::hard_link(dir.join(MANIFEST), dir.join("manifest-link")).unwrap();
        assert!(matches!(
            read_manifest(dir_fd.as_fd(), mine),
            Err(LifecycleRefusal::BoundaryUnverified)
        ));
        std::fs::remove_file(dir.join("manifest-link")).unwrap();
        assert!(
            read_manifest(dir_fd.as_fd(), mine).is_ok(),
            "the owner's own reads again"
        );
        std::fs::rename(dir.join(MANIFEST), dir.join("manifest-real")).unwrap();
        std::os::unix::fs::symlink(dir.join("manifest-real"), dir.join(MANIFEST)).unwrap();
        assert!(matches!(
            read_manifest(dir_fd.as_fd(), mine),
            Err(LifecycleRefusal::BoundaryUnverified)
        ));
        std::fs::remove_file(dir.join(MANIFEST)).unwrap();
        std::fs::rename(dir.join("manifest-real"), dir.join(MANIFEST)).unwrap();
        // **A torn last line is dropped and the next append truncates it**
        // (Codex on #94, round 3): the lines before it read, the latest is
        // selected among them, and after an append the file holds whole
        // lines alone. Perturbation: parse the torn line and the read
        // refuses the whole manifest.
        {
            use std::io::Write as _;
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(dir.join(MANIFEST))
                .unwrap();
            file.write_all(b"{\"ordinal\":9,\"dig").unwrap();
        }
        let read = read_manifest(dir_fd.as_fd(), mine).unwrap();
        assert_eq!(read.len(), 4, "the torn line is dropped: {read:?}");
        assert!(select(dir_fd.as_fd(), me, None, mine).unwrap().is_some());
        append_line(dir_fd.as_fd(), mine, &lines[0]).unwrap();
        let text = std::fs::read_to_string(dir.join(MANIFEST)).unwrap();
        assert!(
            !text.contains("\"dig\n") && !text.contains("\"dig{"),
            "{text}"
        );
        assert!(text.ends_with('\n'));
        assert_eq!(read_manifest(dir_fd.as_fd(), mine).unwrap().len(), 5);
        // A manifest that does not parse refuses, and so does one absent
        // beside published files; absent beside none is the first load.
        std::fs::write(dir.join(MANIFEST), "not json\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir.join(MANIFEST), std::fs::Permissions::from_mode(0o644))
                .unwrap();
        }
        assert!(matches!(
            select(dir_fd.as_fd(), me, None, mine),
            Err(LifecycleRefusal::BoundaryUnverified)
        ));
        std::fs::remove_file(dir.join(MANIFEST)).unwrap();
        assert!(matches!(
            select(dir_fd.as_fd(), me, None, mine),
            Err(LifecycleRefusal::BoundaryUnverified)
        ));
        // A manifest of another mode is not this crate's and refuses, read
        // or appended.
        std::fs::write(dir.join(MANIFEST), "").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir.join(MANIFEST), std::fs::Permissions::from_mode(0o666))
                .unwrap();
        }
        assert!(matches!(
            read_manifest(dir_fd.as_fd(), mine),
            Err(LifecycleRefusal::BoundaryUnverified)
        ));
        assert!(matches!(
            append_line(dir_fd.as_fd(), mine, &lines[0]),
            Err(LifecycleRefusal::BoundaryUnverified)
        ));
        std::fs::remove_file(dir.join(MANIFEST)).unwrap();
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            std::fs::remove_file(entry.path()).unwrap();
        }
        assert!(
            select(dir_fd.as_fd(), me, None, mine).unwrap().is_none(),
            "nothing at all is the first load"
        );
    }

    /// **The marker's three states round-trip and resolve the reset**, per
    /// `weaver-admin-Spec` section 4: open is `NoCleanUnload`, forced is
    /// `ForcedUnload`, closed and absent are none, and writing `None`
    /// removes it.
    #[test]
    fn the_marker_round_trips_and_resolves_the_reset() {
        let scratch = crate::scratch::Scratch(
            std::env::temp_dir().join(format!("weaver-admin-marker-{}", std::process::id())),
        );
        let root = scratch.0.clone();
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(read_marker(&root), None);
        assert_eq!(reset_from(None), None);
        for marker in [
            Marker::Open { run: "r-1".into() },
            Marker::Closed { run: "r-1".into() },
            Marker::Forced { run: "r-2".into() },
        ] {
            write_marker(&root, Some(&marker)).unwrap();
            assert_eq!(read_marker(&root).as_ref(), Some(&marker));
        }
        assert_eq!(
            reset_from(Some(&Marker::Open { run: "r-1".into() })),
            Some(weaver_types::Reset {
                prior_run: weaver_types::RunId("r-1".into()),
                reason: weaver_types::ResetReason::NoCleanUnload,
            })
        );
        assert_eq!(
            reset_from(Some(&Marker::Forced { run: "r-2".into() })).map(|r| r.reason),
            Some(weaver_types::ResetReason::ForcedUnload)
        );
        assert_eq!(
            reset_from(Some(&Marker::Closed { run: "r-1".into() })),
            None
        );
        write_marker(&root, None).unwrap();
        assert_eq!(read_marker(&root), None);
    }

    /// **The marker is 0644 whatever the umask**, per Spec section 4 (Codex
    /// on #94, round 10): under a umask of 000 the create alone would leave
    /// it 0666 in the root; the mode is set through the descriptor.
    /// Perturbation: drop the `fchmod` and the marker reads 0666.
    #[test]
    fn the_marker_is_0644_whatever_the_umask() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = crate::scratch::Scratch(
            std::env::temp_dir().join(format!("weaver-admin-marker-umask-{}", std::process::id())),
        );
        std::fs::create_dir_all(&scratch.0).unwrap();
        let before = nix::sys::stat::umask(nix::sys::stat::Mode::empty());
        let written = write_marker(&scratch.0, Some(&Marker::Open { run: "r-1".into() }));
        nix::sys::stat::umask(before);
        written.unwrap();
        let mode = std::fs::metadata(scratch.0.join(MARKER))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o7777, 0o644);
    }

    /// **The published copy is 0640 whatever the umask**, per Spec section 6
    /// (Codex on #94, round 8): under a umask of 077 the open alone would
    /// make it 0600 and the exact-mode check would refuse every publication;
    /// the mode is set through the descriptor. Perturbation: drop the
    /// `fchmod` and nothing publishes under that umask.
    #[test]
    fn the_published_copy_is_0640_whatever_the_umask() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = crate::scratch::Scratch(
            std::env::temp_dir().join(format!("weaver-admin-umask-{}", std::process::id())),
        );
        let base = scratch.0.clone();
        let room = base.join("room");
        let dir = base.join("decl");
        std::fs::create_dir_all(&room).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let dir_fd = open_directory(&dir).unwrap();
        let me = nix::unistd::getuid().as_raw();
        let mine = Owner {
            uid: me,
            gid: nix::unistd::getgid().as_raw(),
        };
        let owner = (me, mine.gid);
        let bytes = save_point("r-1", 7, 1, 5_000_000_000, b"under a umask");
        let judged = judge(&bytes).unwrap();
        std::fs::write(room.join(format!("{}{SUFFIX}", judged.digest)), &bytes).unwrap();
        let before = nix::sys::stat::umask(nix::sys::stat::Mode::from_bits_truncate(0o077));
        let published = publish(&room, me, dir_fd.as_fd(), owner, mine, &[]);
        nix::sys::stat::umask(before);
        let lines = published.unwrap();
        assert_eq!(lines.len(), 1, "the copy published under the umask");
        let mode = std::fs::metadata(dir.join(&lines[0].name))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o7777, 0o640);
    }

    /// **The publication lands in the judged directory whatever the path
    /// does**, per `weaver-admin-Spec` section 9 (Codex on #94, round 7):
    /// the directory is opened once, then renamed away and another put at
    /// its path; the publication, its manifest line and the selection all go
    /// through the descriptor and land in the directory judged, the one at
    /// the path getting nothing. The pin is the type: every step takes the
    /// descriptor and no path reaches it, so the perturbation is the watch's
    /// own, opening the descriptor after the swap, which puts everything in
    /// the replacement and fails the assertion.
    #[test]
    fn the_publication_lands_in_the_judged_directory_whatever_the_path_does() {
        let scratch = crate::scratch::Scratch(
            std::env::temp_dir().join(format!("weaver-admin-swap-{}", std::process::id())),
        );
        let base = scratch.0.clone();
        let room = base.join("room");
        let dir = base.join("decl");
        std::fs::create_dir_all(&room).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let dir_fd = open_directory(&dir).unwrap();
        let me = nix::unistd::getuid().as_raw();
        let mine = Owner {
            uid: me,
            gid: nix::unistd::getgid().as_raw(),
        };
        let owner = (me, mine.gid);
        let bytes = save_point("r-1", 7, 1, 5_000_000_000, b"judged directory");
        let judged = judge(&bytes).unwrap();
        std::fs::write(room.join(format!("{}{SUFFIX}", judged.digest)), &bytes).unwrap();
        // The swap: the judged directory moves aside and a replacement
        // stands at the path.
        let aside = base.join("decl-judged");
        std::fs::rename(&dir, &aside).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let lines = publish(&room, me, dir_fd.as_fd(), owner, mine, &[]).unwrap();
        assert_eq!(lines.len(), 1);
        assert!(
            aside.join(&lines[0].name).exists() && aside.join(MANIFEST).exists(),
            "the target and the line land in the directory judged"
        );
        assert!(
            std::fs::read_dir(&dir).unwrap().next().is_none(),
            "the directory at the path gets nothing"
        );
        let latest = select(dir_fd.as_fd(), me, None, mine)
            .unwrap()
            .expect("a latest");
        assert_eq!(latest.line.digest, judged.digest);
        // A descriptor opened at the path now reaches the replacement, which
        // holds no manifest and no file: the first load's answer.
        let replacement = open_directory(&dir).unwrap();
        assert!(
            select(replacement.as_fd(), me, None, mine)
                .unwrap()
                .is_none()
        );
    }
}

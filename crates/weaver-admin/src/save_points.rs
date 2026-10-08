//! Admin's side of the save points, per `weaver-admin-Spec` sections 3, 4
//! and 6 on the operator's rulings of 2026-10-06 on #1 (the A3.0 items):
//! the publication of the member's finished save points into the territory's
//! save-points directory under a root-owned manifest, the selection of the save point a
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
use std::path::Path;

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
/// read into root's memory, since the room is the member's to write.
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

/// **Adopt an entry that stands in the save-points directory, durably**
/// (Codex on #94): judged through the directory as a load judges it, then,
/// where it stands, the directory synced, so the entry is durable before a
/// manifest line names it, as the publication's own rename is. A retry that
/// adopts what an interrupted publication renamed, and a `restore` naming a
/// file placed by hand, both come through here. `sync` is the directory's
/// sync, a parameter so a test can count it.
fn adopt_durably(
    directory: BorrowedFd<'_>,
    name: &str,
    file_owner: (u32, u32),
    sync: &mut dyn FnMut(BorrowedFd<'_>) -> std::io::Result<()>,
) -> Result<Option<(std::fs::File, Judged)>, String> {
    let judged = open_judged(directory, name, file_owner).map_err(|fault| fault.why)?;
    if judged.is_some() {
        sync(directory).map_err(|e| format!("the save-points directory does not sync: {e}"))?;
    }
    Ok(judged)
}

/// Sync a directory named by path: the config root's, this crate's own
/// root-owned directory where the marker lives, per Spec section 4, which
/// no other principal can swap.
fn sync_path(directory: &Path) -> std::io::Result<()> {
    sync_directory(open_directory(directory)?.as_fd())
}

/// The names the directory holds, read through its descriptor.
fn list_directory(directory: BorrowedFd<'_>) -> nix::Result<Vec<String>> {
    // **A listing that fails is never an empty directory** (the custody
    // audit's G9): a failed `dup`, open or read of an entry answers the error,
    // so a refusal that rests on the directory's contents fails closed.
    let duplicate = nix::unistd::dup(directory)?;
    let mut dir = nix::dir::Dir::from_fd(duplicate)?;
    let mut names = Vec::new();
    for entry in dir.iter() {
        let entry = entry?;
        if let Ok(name) = entry.file_name().to_str()
            && name != "."
            && name != ".."
        {
            names.push(name.to_string());
        }
    }
    Ok(names)
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
/// root's in the territory under the layout ruling of 2026-10-07; the
/// judgment stands all the same, so an entry another principal owns refuses
/// by uid, a second link refuses by count, and a link never opens,
/// `O_NOFOLLOW` refusing it before this is reached; what is found is named.
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
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_NONBLOCK
            | nix::fcntl::OFlag::O_CLOEXEC,
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
    let (stamp, length, _) = judge_stamp(header)?;
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
        stamp,
        digest: hex(&sha2::Sha256::digest(bytes)),
    })
}

/// **Judge a stamp line alone**, per `weaver-state-Spec` section 3: the
/// seven members, the version, the run and schema bounds, the nonce, and the
/// image length the stamp states. Answers the stamp, that length and the
/// nonce's process and count; the check and the digest are the whole file's
/// and `judge`'s.
fn judge_stamp(header: &[u8]) -> Result<(Stamp, u64, Taken), String> {
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
    // **The member's strings are bounded as the member's parse bounds them**
    // (the operator's ruling of 2026-10-08 on #1, the custody audit's G2):
    // a run id of at most 128 printable ASCII bytes and a schema digest of
    // exactly 64 lowercase hex, so a stamp cannot carry a string the size of
    // the file into root's memory, the manifest and the enter.
    if !run_id_sound(&run) {
        return Err("the stamp's run is not at most 128 printable ASCII bytes".into());
    }
    if !schema_sound(&schema) {
        return Err("the stamp's schema is not 64 lowercase hex".into());
    }
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
    let pid = taken
        .get("pid")
        .and_then(|v| v.as_u64())
        .ok_or("the stamp's taken names no pid")?;
    let ordinal = taken
        .get("ordinal")
        .and_then(|v| v.as_u64())
        .ok_or("the stamp's taken names no ordinal")?;
    let wall_ns = taken
        .get("wall_ns")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
        // **Digits that fit an unsigned 64-bit count of nanoseconds**, the
        // member's rule read here too (Codex on #94, round 5, the corpus's
        // `nonce-wall-clock-overlong` case): what a clock can be.
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or("the stamp's taken names no wall clock")?;
    Ok((
        Stamp {
            run,
            sequence,
            turn,
            schema,
            wall_ns,
        },
        length,
        Taken { pid, ordinal },
    ))
}

/// **The member process that took a save point, and its own count**, the
/// stamp's `taken.pid` and `taken.ordinal`: the count strictly increases
/// within one member process, so it orders that process's save points
/// whatever the clock or the covered position did (the operator's ruling of
/// 2026-10-08 on #99, N3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Taken {
    pid: u64,
    ordinal: u64,
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
/// 64-character lowercase hex digest, the one form the member writes and the
/// copy's digest comparison admits: a part is dotted and refused here, and
/// so is anything else. **Lowercase only** (the #99 area 1 review, K2): an
/// uppercase name passed the scan and failed at every copy, a deferral no
/// later load could drain.
pub fn is_finished_name(name: &str) -> bool {
    name.strip_suffix(SUFFIX).is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
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

/// A stamp's run id: at most 128 bytes, each printable ASCII, per
/// `weaver-state-Spec` section 3.
fn run_id_sound(run: &str) -> bool {
    run.len() <= 128 && run.bytes().all(|b| (0x20..=0x7e).contains(&b))
}

/// A stamp's schema digest: exactly 64 lowercase hex, per `weaver-state-Spec`
/// section 3.
fn schema_sound(schema: &str) -> bool {
    schema.len() == 64
        && schema
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
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
    append_line_with(directory, owner, line, &mut |fd, uid, gid| {
        nix::unistd::fchown(
            fd,
            Some(nix::unistd::Uid::from_raw(uid)),
            Some(nix::unistd::Gid::from_raw(gid)),
        )
    })
}

/// `append_line` with the new manifest's chown a parameter, so a test can
/// count it.
fn append_line_with(
    directory: BorrowedFd<'_>,
    owner: Owner,
    line: &ManifestLine,
    chown: &mut dyn FnMut(BorrowedFd<'_>, u32, u32) -> nix::Result<()>,
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
        // **Its owner and group set, never the invoker's** (Codex on #94):
        // `save-points/` is not setgid, so a new file takes the creating
        // process's effective gid, and a root shell whose egid is not 0 would
        // leave a manifest its own judgment refuses, stopping every later
        // publication.
        chown(file.as_fd(), owner.uid, owner.gid)
            .map_err(|e| refuse(format!("does not take its owner and group: {e}")))?;
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

/// A finished save point found in the member's room, judged: its name and
/// its judgment, never its bytes (Codex on #94), so a room of many files
/// never holds more than the one being copied in this root process's memory.
struct RoomEntry {
    name: String,
    /// The digest the finished name carries, which the copy's whole
    /// judgment holds the bytes to.
    digest: String,
    stamp: Stamp,
    taken: Taken,
}

/// **Read the member's room through its own descriptor**, per
/// `weaver-admin-Spec` section 6: each entry opened beneath the directory
/// descriptor without following links, a regular file owned by the member's
/// uid under a finished name, its bytes judged; anything else is left in
/// place and named.
fn read_room(room: BorrowedFd<'_>, member_uid: u32) -> Result<Vec<RoomEntry>, LifecycleRefusal> {
    // **Listed through the room's own descriptor** (the custody audit's
    // G12), the descriptor `open_room` opened without following a link, so
    // the listing and every open and removal below resolve the one
    // directory judged and never the path again.
    let names = list_directory(room).map_err(|e| {
        diag!("weaver-admin: the room does not list ({e}); nothing is published");
        LifecycleRefusal::BoundaryUnverified
    })?;
    let mut found = Vec::new();
    for name in names {
        if !is_finished_name(&name) {
            continue;
        }
        // **Only the stamp line is read at the scan** (Codex on #94, at
        // ab8acef, and the custody review): enough for the order and the
        // name, a few KiB at the most, so a room of many files costs the
        // scan little and the per-verb cap bounds the whole reads, which
        // happen at the copy alone, each judging the bytes it copies.
        // **A room file that does not read refuses the verb** (the #94
        // survey's S3 and S4): left out of the order, it would be published
        // by a later verb after newer files, outranking them. One that reads
        // and is no save point is left in place and named, as it can never
        // publish.
        if let Some((stamp, taken)) = scan_room_file(room, &name, member_uid)? {
            let digest = name.trim_end_matches(SUFFIX).to_string();
            found.push(RoomEntry {
                name,
                digest,
                stamp,
                taken,
            });
        }
    }
    // The listing's order is the filesystem's and means nothing; by name
    // it is the same on every box, and `publish` orders by the stamp's
    // sequence.
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

/// The room's name in the territory.
pub const ROOM: &str = "state";

/// **The member's room, opened through the territory's descriptor without
/// following a link** (the custody audit's G12), per Spec section 6: the
/// territory's `state` entry as a directory. Absent is no room, said in the
/// log; anything else that does not open, a link among them, refuses.
pub fn open_room(territory: BorrowedFd<'_>) -> Result<Option<OwnedFd>, LifecycleRefusal> {
    match nix::fcntl::openat(
        territory,
        ROOM,
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_DIRECTORY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    ) {
        Ok(fd) => Ok(Some(fd)),
        Err(nix::errno::Errno::ENOENT) => {
            diag!("weaver-admin: the territory holds no room; nothing is published");
            Ok(None)
        }
        Err(e) => {
            diag!(
                "weaver-admin: the territory's room does not open as a directory without following a link ({e})"
            );
            Err(LifecycleRefusal::BoundaryUnverified)
        }
    }
}

// Whole reads of room files on this thread, for the tests that bound them.
#[cfg(test)]
thread_local! {
    static WHOLE_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The bound on a stamp line the scan reads: the seven members with the run
/// and schema bounded, far under a KiB, and room to spare.
const STAMP_LINE_BOUND: u64 = 4096;

/// **One room file's stamp, read at the scan**: opened as the copy opens it,
/// without following a link and without blocking, a regular file of the
/// member's under the size bound, its first line read within
/// `STAMP_LINE_BOUND` and judged as a stamp. The image, the check and the
/// digest are judged at the copy, from the bytes it copies. A file that
/// does not open, stat or read, or stands past the save point's bound, which
/// the member never writes past (the operator's ruling of 2026-10-08 on #1),
/// refuses the verb `BoundaryUnverified` naming it (the #94 survey's S3 and
/// S4); a file gone since the listing, a link, a file not the member's or a
/// line that is no stamp is no save point, `Ok(None)`, left in place and
/// named.
fn scan_room_file(
    dir: BorrowedFd<'_>,
    name: &str,
    member_uid: u32,
) -> Result<Option<(Stamp, Taken)>, LifecycleRefusal> {
    use std::os::unix::fs::MetadataExt;
    let refuse = |why: String| {
        diag!("weaver-admin: the room's {name} {why}; nothing is published until it is cleared");
        LifecycleRefusal::BoundaryUnverified
    };
    let fd = match nix::fcntl::openat(
        dir,
        name,
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_NONBLOCK
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(nix::errno::Errno::ENOENT) => return Ok(None),
        Err(nix::errno::Errno::ELOOP) => {
            diag!("weaver-admin: the room's {name} is a link and is left in place");
            return Ok(None);
        }
        Err(e) => return Err(refuse(format!("does not open ({e})"))),
    };
    let mut file = std::fs::File::from(fd);
    let metadata = file
        .metadata()
        .map_err(|e| refuse(format!("does not stat ({e})")))?;
    if !metadata.is_file() {
        diag!("weaver-admin: the room's {name} is not a regular file and is left in place");
        return Ok(None);
    }
    // **A finished file the member does not own refuses** (the #99 area 1
    // review, K2): the room is the member's own `0700`, so a regular file
    // under a finished name and another uid is a re-made account's or a
    // planted one, and may be newer state; passed over silently, an older
    // save point would load in its place.
    if metadata.uid() != member_uid {
        return Err(refuse(format!(
            "is owned by uid {}, not the member's {member_uid}",
            metadata.uid()
        )));
    }
    if metadata.len() > SAVE_POINT_BOUND {
        return Err(refuse(format!(
            "is {} bytes, past the bound of {SAVE_POINT_BOUND} the member never writes past",
            metadata.len()
        )));
    }
    let mut head = Vec::new();
    (&mut file)
        .take(STAMP_LINE_BOUND)
        .read_to_end(&mut head)
        .map_err(|e| refuse(format!("does not read ({e})")))?;
    let Some(end) = head.iter().position(|&b| b == b'\n') else {
        diag!(
            "weaver-admin: the room's {name} carries no stamp line within {STAMP_LINE_BOUND} bytes and is left in place"
        );
        return Ok(None);
    };
    // **A save-point format this crate does not read refuses** (K2): a
    // stamp line naming another version is a member newer than this admin,
    // after a rollback of the install, and its file may be the newest
    // state; left in place silently, an older save point would load.
    if let Some(version) = serde_json::from_slice::<serde_json::Value>(&head[..end])
        .ok()
        .and_then(|stamp| stamp.get("weaver-save-point").and_then(|v| v.as_u64()))
        && version != 1
    {
        return Err(refuse(format!(
            "is save-point format {version}, which this admin does not read"
        )));
    }
    match judge_stamp(&head[..end]) {
        Ok((stamp, _, taken)) => Ok(Some((stamp, taken))),
        Err(why) => {
            diag!("weaver-admin: the room's {name} is not a save point ({why})");
            Ok(None)
        }
    }
}

/// **One room file, opened and judged through the room's descriptor**: a
/// regular file owned by the member's uid, under the bound through the read,
/// whose bytes judge sound and digest to its finished name. The bytes and the
/// judgment are answered together, from one read, so what is copied is what
/// was judged; anything else is left in place and named.
fn judge_room_file(dir: BorrowedFd<'_>, name: &str, member_uid: u32) -> Option<(Vec<u8>, Judged)> {
    #[cfg(test)]
    WHOLE_READS.with(|count| count.set(count.get() + 1));
    use std::os::unix::fs::MetadataExt;
    let Ok(fd) = nix::fcntl::openat(
        dir,
        name,
        // **Never blocking on the open** (the custody audit's G1): a FIFO
        // the member made under a finished name would hold this root open
        // until a writer came, wedging the verb; the type is judged below.
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_NONBLOCK
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    ) else {
        diag!("weaver-admin: the room's {name} does not open and is left in place");
        return None;
    };
    let mut file = std::fs::File::from(fd);
    let Ok(metadata) = file.metadata() else {
        diag!("weaver-admin: the room's {name} does not stat and is left in place");
        return None;
    };
    if !metadata.is_file() || metadata.uid() != member_uid {
        diag!(
            "weaver-admin: the room's {name} is not the member's regular file and is left in place"
        );
        return None;
    }
    if metadata.len() > SAVE_POINT_BOUND {
        diag!(
            "weaver-admin: the room's {name} is {} bytes, past the bound of {SAVE_POINT_BOUND}, and is left in place",
            metadata.len()
        );
        return None;
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
            return None;
        }
        Err(e) => {
            diag!("weaver-admin: the room's {name} does not read ({e}) and is left in place");
            return None;
        }
    };
    match judge(&bytes) {
        Ok(judged) if format!("{}{SUFFIX}", judged.digest) == name => Some((bytes, judged)),
        Ok(_) => {
            diag!("weaver-admin: the room's {name} is not the file its name claims");
            None
        }
        Err(why) => {
            diag!("weaver-admin: the room's {name} is not a save point ({why})");
            None
        }
    }
}

/// **Publish the member's finished save points into the territory's
/// save-points directory**, per `weaver-admin-Spec` section 6, under the lock
/// the caller holds: each is copied under a temporary name owned by
/// `file_owner`, root and the access group in production, and mode `0640`,
/// renamed to its published name, named on one manifest line,
/// and only then removed from the room. `reports` are what the harness
/// reported of the save points it recorded, so a file the report names
/// carries the event's position and the report's arrival, and any other file
/// is a recovered one. Answers the lines appended.
#[cfg(test)]
pub fn publish(
    room: BorrowedFd<'_>,
    member_uid: u32,
    directory: BorrowedFd<'_>,
    file_owner: (u32, u32),
    owner: Owner,
    reports: &[(SavePointReport, Arrival)],
) -> Result<Vec<ManifestLine>, LifecycleRefusal> {
    publish_noting_deferral(room, member_uid, directory, file_owner, owner, reports)
        .map(|(lines, _)| lines)
}

/// `publish`, answering besides the lines whether it left any room file for
/// a later verb, past the cap or the free space (Codex on #94, at ab8acef),
/// or at a judgment or copy that failed (the #94 survey's S3):
/// a load must not select while a newer save point waits in the room.
pub fn publish_noting_deferral(
    room: BorrowedFd<'_>,
    member_uid: u32,
    directory: BorrowedFd<'_>,
    file_owner: (u32, u32),
    owner: Owner,
    reports: &[(SavePointReport, Arrival)],
) -> Result<(Vec<ManifestLine>, bool), LifecycleRefusal> {
    publish_with(
        room,
        member_uid,
        directory,
        file_owner,
        owner,
        reports,
        &mut PublishHooks {
            after_scan: &mut || {},
            available: &mut available_bytes,
            cap: PUBLISH_CAP,
        },
    )
}

/// **At most this many room files are published by one verb** (the custody
/// audit's G23), this act's election: the rest stay in the room, named in
/// the log, for the next verb, so a member that fills its room cannot hold a
/// verb, and the invocation lock with it, for the time of reading thousands
/// of files.
pub const PUBLISH_CAP: usize = 32;

/// **The space a copy leaves free on the save-points filesystem** (the
/// custody audit's G23), this act's election: a copy is made only where the
/// file's size and this much more stand free, so a member's files never fill
/// the filesystem root's other writes share.
pub const PUBLISH_RESERVE: u64 = 64 * 1024 * 1024;

/// The space free to an unprivileged writer on the directory's filesystem.
fn available_bytes(directory: BorrowedFd<'_>) -> std::io::Result<u64> {
    let stat = nix::sys::statvfs::fstatvfs(directory).map_err(std::io::Error::from)?;
    Ok(stat.blocks_available() as u64 * stat.fragment_size() as u64)
}

/// What a test may vary in a publication: a hook run between the room's scan
/// and the first copy, the free-space look, and the per-verb cap.
struct PublishHooks<'a> {
    after_scan: &'a mut dyn FnMut(),
    available: &'a mut dyn FnMut(BorrowedFd<'_>) -> std::io::Result<u64>,
    cap: usize,
}

/// `publish` with its hooks a parameter, so a test can change the room
/// between the scan and the copies as a running member could, and set the
/// free space and the cap.
fn publish_with(
    room: BorrowedFd<'_>,
    member_uid: u32,
    directory: BorrowedFd<'_>,
    file_owner: (u32, u32),
    owner: Owner,
    reports: &[(SavePointReport, Arrival)],
    hooks: &mut PublishHooks<'_>,
) -> Result<(Vec<ManifestLine>, bool), LifecycleRefusal> {
    let mut lines = read_manifest(directory, owner)?;
    let mut appended = Vec::new();
    let mut entries = read_room(room, member_uid)?;
    (hooks.after_scan)();
    // **Several entries publish recovered first, then reported, the member
    // process's own count ordering within a kind** (Codex on #94, rounds 5
    // and 6, and the operator's ruling of 2026-10-08 on #99, N3), per Spec
    // section 6: a reported save point was taken last by construction, so
    // it is minted last; within a kind the stamp's `taken.ordinal`, which
    // strictly increases within one member process, orders them. Neither
    // the clock nor the covered position orders anything: the clock can
    // step back, and the covered position's run is the last landed event's,
    // which changes within one agent run after a restore.
    let reported = |entry: &RoomEntry| {
        reports
            .iter()
            .any(|(report, _)| report.save_point == entry.digest)
    };
    // **Save points of more than one member process refuse** (N3): the
    // room holds one member process's files, since a load publishes it
    // whole or refuses before a member stands, so a second process, or a
    // count repeated within one, is a room this crate cannot order. The
    // operator clears the room; until then nothing is published and a load
    // refuses rather than guess.
    let processes: std::collections::BTreeSet<u64> =
        entries.iter().map(|entry| entry.taken.pid).collect();
    let counts: std::collections::BTreeSet<Taken> =
        entries.iter().map(|entry| entry.taken).collect();
    if processes.len() > 1 || counts.len() != entries.len() {
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        diag!(
            "weaver-admin: the room holds save points of more than one member process, or two of one count ({}), which nothing orders; nothing is published until the room is cleared",
            names.join(", ")
        );
        return Err(LifecycleRefusal::BoundaryUnverified);
    }
    entries.sort_by(|a, b| {
        reported(a)
            .cmp(&reported(b))
            .then_with(|| a.taken.ordinal.cmp(&b.taken.ordinal))
    });
    let remove_from_room = |name: &str| {
        let _ = nix::unistd::unlinkat(room, name, nix::unistd::UnlinkatFlags::NoRemoveDir);
    };
    // **Every entry declined stops the verb, never skips it** (the custody
    // audit's G23 and the #94 survey's S3): entries go oldest first and the
    // reported one last, and the latest a load selects is the highest
    // ordinal, so a file left behind a newer one would outrank it when
    // published later. The cap, the free-space look, a judgment at the copy
    // that fails and a copy that fails each stop here and answer a deferral,
    // so every file published is older than every file left; a leave's own
    // save point so left leaves its unload refusing, and a load refuses too.
    // There is no skip: a standing line whose target is of other bytes stops
    // the verb too (Codex on #94 at 7a25db2).
    let total = entries.len();
    let mut deferred = false;
    for (at, entry) in entries.into_iter().enumerate() {
        if at == hooks.cap {
            deferred = true;
            diag!(
                "weaver-admin: the room holds {total} finished save points; {} are left for the next verb, past this one's cap of {}",
                total - at,
                hooks.cap
            );
            break;
        }
        let (position, arrived) = reports
            .iter()
            .find(|(report, _)| report.save_point == entry.digest)
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
        let name = published_name(&Judged {
            stamp: entry.stamp.clone(),
            digest: entry.digest.clone(),
        });
        // **A line standing for the room's copy is judged before the copy
        // goes** (Codex on #94, round 9): the copy goes only where the line's
        // target stands and digests to the line; a target gone is recreated
        // beneath the standing line, the line being the record and no second
        // one appended; a target of other bytes is left with the copy, which
        // is then the last sound copy, and named in the log.
        let standing = lines
            .iter()
            .find(|line| line.digest == entry.digest)
            .cloned();
        if let Some(line) = &standing {
            match open_published(directory, line, file_owner) {
                Ok(Some(_)) => {
                    remove_from_room(&entry.name);
                    continue;
                }
                Ok(None) => diag!(
                    "weaver-admin: the manifest names {}, which is gone; the room's copy recreates it under the standing line",
                    line.name
                ),
                // **A standing line whose target differs stops the verb**
                // (Codex on #94 at 7a25db2): the room's copy may be the
                // newest sound state, so publishing past it, or a load
                // selecting past it, would restore older state. It is a
                // deferral, so a load refuses until the target is repaired
                // or cleared.
                Err(why) => {
                    diag!(
                        "weaver-admin: the manifest names {}, which is not the file its line says ({why}); the room's copy stays and every later save point is left for the next verb",
                        line.name
                    );
                    deferred = true;
                    break;
                }
            }
        }
        // **Every step goes through the directory's descriptor** (Codex on
        // #94, round 7), per Spec section 9: the temporary is made, renamed
        // and the directory synced against the descriptor opened at the
        // judgment, so a directory swapped under the path between
        // steps is not followed and a link put at the path has this root
        // process create nothing where it points.
        // **The copy is made from bytes judged in the same read** (Codex on
        // #94): the scan kept no bytes, so the entry is opened again through
        // the room's descriptor and judged again here, and those bytes, and
        // no other read of them, are what is copied. The member can still
        // write its room, so a verdict from the scan is never trusted for a
        // later read; a file that changed since is refused and left in place.
        let Some((bytes, judged)) = judge_room_file(room, &entry.name, member_uid) else {
            diag!(
                "weaver-admin: the room's {} and every later save point are left for the next verb",
                entry.name
            );
            deferred = true;
            break;
        };
        if judged.digest != entry.digest {
            diag!(
                "weaver-admin: the room's {} changed after it was judged; it and every later save point are left for the next verb",
                entry.name
            );
            deferred = true;
            break;
        }
        match (hooks.available)(directory) {
            Ok(free) if free >= bytes.len() as u64 + PUBLISH_RESERVE => {}
            Ok(free) => {
                diag!(
                    "weaver-admin: the save-points filesystem has {free} bytes free, under the room's {} and the reserve of {PUBLISH_RESERVE}; it and every later save point are left for the next verb",
                    entry.name
                );
                deferred = true;
                break;
            }
            Err(e) => {
                diag!(
                    "weaver-admin: the save-points filesystem's free space does not read ({e}); the room's save points are left for the next verb"
                );
                deferred = true;
                break;
            }
        }
        let temporary = format!(".publishing-{}", entry.digest);
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
            file.write_all(&bytes)?;
            nix::unistd::fchown(
                file.as_fd(),
                Some(nix::unistd::Uid::from_raw(file_owner.0)),
                Some(nix::unistd::Gid::from_raw(file_owner.1)),
            )
            .map_err(std::io::Error::from)?;
            // Synced after the ownership change, so the owner is as durable
            // as the bytes before the entry is named anywhere.
            file.sync_all()?;
            let metadata = file.metadata()?;
            use std::os::unix::fs::MetadataExt;
            if metadata.uid() != file_owner.0
                || metadata.gid() != file_owner.1
                || metadata.mode() & 0o777 != 0o640
            {
                return Err(std::io::Error::other(
                    "the copy is not root's 0640, read by the access group",
                ));
            }
            // **The rename replaces nothing**: an entry anyone put
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
                    match adopt_durably(directory, &name, file_owner, &mut sync_directory) {
                        Ok(Some((_, standing))) if standing.digest == entry.digest => Ok(()),
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
                "weaver-admin: the room's {} did not publish ({e}); it and every later save point are left for the next verb",
                entry.name
            );
            remove_temporary();
            deferred = true;
            break;
        }
        if standing.is_some() {
            // The target stands again under its line; nothing to append.
            remove_from_room(&entry.name);
            continue;
        }
        let line = ManifestLine {
            ordinal: next_ordinal(&lines),
            digest: entry.digest.clone(),
            name,
            stamp: entry.stamp.clone(),
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
    Ok((appended, deferred))
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
/// file, not a link, `file_owner`'s (root's in production), mode `0640` and
/// read by the access group, under the size bound, whose bytes judge sound and whose published name is the one
/// its bytes compute. `Ok(None)` is no entry. The file is answered rewound.
fn open_judged(
    directory: BorrowedFd<'_>,
    name: &str,
    file_owner: (u32, u32),
) -> Result<Option<(std::fs::File, Judged)>, NotPublished> {
    use std::os::unix::fs::MetadataExt;
    let fault = |why: String| NotPublished { why };
    let fd = match nix::fcntl::openat(
        directory,
        name,
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_NONBLOCK
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(nix::errno::Errno::ENOENT) => return Ok(None),
        Err(e) => return Err(fault(format!("{name} does not open: {e}"))),
    };
    let mut file = std::fs::File::from(fd);
    let metadata = file
        .metadata()
        .map_err(|e| fault(format!("{name} does not stat: {e}")))?;
    if !metadata.is_file() {
        return Err(fault(format!("{name} is not a regular file")));
    }
    if metadata.uid() != file_owner.0 {
        return Err(fault(format!("{name} is not root's")));
    }
    // **The group is judged as the owner is** (Codex on #94): a file the
    // access group cannot read is not a published save point, however it
    // came to stand here, so the operator and the connector read every one
    // a load can restore.
    if metadata.gid() != file_owner.1 {
        return Err(fault(format!("{name} is not grouped to the access group")));
    }
    if metadata.mode() & 0o7777 != 0o640 {
        return Err(fault(format!(
            "{name} is not mode 0640, root's and read by the access group"
        )));
    }
    if metadata.len() > SAVE_POINT_BOUND {
        return Err(fault(format!(
            "{name} is {} bytes, past the bound of {SAVE_POINT_BOUND}",
            metadata.len()
        )));
    }
    // **Bounded through the read as well, as a defence** (Codex on #94): the
    // file is judged root's above and root alone writes it, so it cannot grow
    // under this read today; the bound holds through the read all the same,
    // so it stays true if the ownership ever changes.
    let bytes = read_within(&mut file, SAVE_POINT_BOUND)
        .map_err(|e| fault(format!("{name} does not read: {e}")))?
        .ok_or_else(|| {
            fault(format!(
                "{name} grew past the bound of {SAVE_POINT_BOUND} as it was read"
            ))
        })?;
    let judged =
        judge(&bytes).map_err(|why| fault(format!("{name} is not a save point: {why}")))?;
    if published_name(&judged) != name {
        return Err(fault(format!("{name} is not the name its bytes compute")));
    }
    nix::unistd::lseek(file.as_fd(), 0, nix::unistd::Whence::SeekSet)
        .map_err(|e| fault(format!("{name} does not seek: {e}")))?;
    Ok(Some((file, judged)))
}

/// **Why a published file is not the one its line names**: a fault
/// reaching or judging it, or bytes that are not the save point its line
/// names. Every one refuses the selection (the Planner's ruling of
/// 2026-10-08 on #94), so the cause is carried for the log alone.
#[derive(Debug)]
pub(crate) struct NotPublished {
    why: String,
}

impl std::fmt::Display for NotPublished {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.why)
    }
}

/// Open and judge the file a manifest line names, per `weaver-admin-Spec`
/// section 4: `open_judged`, and the bytes digest to the line's digest.
fn open_published(
    directory: BorrowedFd<'_>,
    line: &ManifestLine,
    file_owner: (u32, u32),
) -> Result<Option<OwnedFd>, NotPublished> {
    let Some((file, judged)) = open_judged(directory, &line.name, file_owner)? else {
        return Ok(None);
    };
    if judged.digest != line.digest {
        return Err(NotPublished {
            why: format!("{} does not digest to its line", line.name),
        });
    }
    Ok(Some(OwnedFd::from(file)))
}

/// **Select the save point a load restores**, per `weaver-admin-Spec`
/// section 4: the one `restore` names by its published name or its digest,
/// which must have a manifest line, or the latest, the line of highest
/// ordinal, whose file must stand and judge to its digest or the load
/// refuses, never passing over to an older line. `Ok(None)` is no save
/// point: an empty
/// manifest with nothing named.
pub fn select(
    directory: BorrowedFd<'_>,
    file_owner: (u32, u32),
    restore: Option<&str>,
    owner: Owner,
) -> Result<Option<Selected>, LifecycleRefusal> {
    select_with(directory, file_owner, restore, owner, &mut list_directory)
}

/// `select` with the directory's listing a parameter, so a test can make it
/// fail.
fn select_with(
    directory: BorrowedFd<'_>,
    file_owner: (u32, u32),
    restore: Option<&str>,
    owner: Owner,
    list: &mut dyn FnMut(BorrowedFd<'_>) -> nix::Result<Vec<String>>,
) -> Result<Option<Selected>, LifecycleRefusal> {
    let lines = read_manifest(directory, owner)?;
    // **A manifest absent beside published files refuses**, per Spec
    // section 4: what is loadable cannot be said, and the files are not
    // loadable without it; no manifest and no file is the first load. A
    // listing that fails refuses as well (the custody audit's G9), never
    // read as an empty directory and so a first load.
    if lines.is_empty()
        && !entry_stands(directory, MANIFEST)
        && list(directory)
            .map_err(|e| {
                diag!("weaver-admin: the save-points directory does not list ({e})");
                LifecycleRefusal::BoundaryUnverified
            })?
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
        return match open_published(directory, line, file_owner) {
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
    // **The latest is the highest ordinal, never passed over** (the
    // Planner's ruling of 2026-10-08 on #94, beyond the survey's S9): a
    // latest gone, differing or not judged refuses `BoundaryUnverified`,
    // naming the line and what was found, so no load restores older state
    // unasked. The operator names an older save point with `restore`, a
    // deliberate act the manifest records.
    let Some(line) = lines.iter().max_by_key(|line| line.ordinal) else {
        return Ok(None);
    };
    match open_published(directory, line, file_owner) {
        Ok(Some(descriptor)) => Ok(Some(Selected {
            descriptor,
            lineage: lineage_of(line),
            line: line.clone(),
        })),
        Ok(None) => {
            diag!(
                "weaver-admin: the manifest's latest, ordinal {}, names {}, which is gone; the load refuses rather than restore an older save point, and restore names one",
                line.ordinal,
                line.name
            );
            Err(LifecycleRefusal::BoundaryUnverified)
        }
        Err(why) => {
            diag!(
                "weaver-admin: the manifest's latest, ordinal {}, is not the file its line says ({why}); the load refuses rather than restore an older save point, and restore names one",
                line.ordinal
            );
            Err(LifecycleRefusal::BoundaryUnverified)
        }
    }
}

/// **Name a save point at a restore**, the `restore` verb's judgment, per
/// `weaver-admin-Spec` section 4: a file the manifest already names answers
/// its line; otherwise the file is judged as a load judges one and a line
/// naming it is appended, marked as arrived by restore, with no position.
pub fn name_at_restore(
    directory: BorrowedFd<'_>,
    file_owner: (u32, u32),
    named: &str,
    owner: Owner,
) -> Result<ManifestLine, LifecycleRefusal> {
    let lines = read_manifest(directory, owner)?;
    let refuse_config = || LifecycleRefusal::ConfigInvalid {
        field: Some(weaver_types::FieldName("restore".into())),
    };
    // **A listed name is judged before the verb answers** (Codex on #94,
    // round 4), per Spec section 4: the published file may have gone or
    // changed since its line, so the verb opens
    // and judges it as the load does, refusing as the load would, and
    // `RestoreNamed` means named and judged loadable now.
    if let Some(line) = lines
        .iter()
        .rev()
        .find(|line| line.name == named || line.digest == named)
    {
        return match open_published(directory, line, file_owner) {
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
            .map_err(|e| {
                diag!("weaver-admin: the save-points directory does not list ({e})");
                LifecycleRefusal::BoundaryUnverified
            })?
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
    let judged = match adopt_durably(directory, named, file_owner, &mut sync_directory) {
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

/// The bound on the marker's size this crate reads: a run reference and a
/// state, a few hundred bytes at the most.
const MARKER_BOUND: u64 = 4096;

/// **Read the marker, failing closed** (the custody audit's G8), per Spec
/// section 4: `Ok(None)` where none stands, the first load's case, and the
/// marker where it reads as one; a marker that stands and does not read, a
/// link, a torn or foreign text or one past the bound, is an error, never
/// read as absent, since an absent marker resolves no reset and a run left
/// open would load without its `NoCleanUnload`.
pub fn load_marker(root: &Path) -> Result<Option<Marker>, String> {
    let file = match nix::fcntl::open(
        &root.join(MARKER),
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_NONBLOCK
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    ) {
        Ok(fd) => std::fs::File::from(fd),
        Err(nix::errno::Errno::ENOENT) => return Ok(None),
        Err(e) => return Err(format!("the marker does not open: {e}")),
    };
    let mut file = file;
    let bytes = read_within(&mut file, MARKER_BOUND)
        .map_err(|e| format!("the marker does not read: {e}"))?
        .ok_or("the marker is past its bound")?;
    let text = String::from_utf8(bytes).map_err(|_| "the marker is not text")?;
    let value: serde_json::Value =
        serde_json::from_str(text.trim()).map_err(|e| format!("the marker does not parse: {e}"))?;
    let run = value
        .get("run")
        .and_then(|v| v.as_str())
        .ok_or("the marker names no run")?
        .to_string();
    Ok(Some(match value.get("state").and_then(|v| v.as_str()) {
        Some("open") => Marker::Open { run },
        Some("closed") => Marker::Closed { run },
        Some("forced") => Marker::Forced { run },
        _ => return Err("the marker names no state it can be".into()),
    }))
}

/// The marker as the tests read it: `None` where none stands or it does not
/// read.
#[cfg(test)]
pub fn read_marker(root: &Path) -> Option<Marker> {
    load_marker(root).ok().flatten()
}

/// Write the marker whole, root-owned, through a temporary name and a rename,
/// or remove it where `None` is written, which restores an absent prior
/// state in a rollback.
pub fn write_marker(root: &Path, marker: Option<&Marker>) -> std::io::Result<()> {
    write_marker_with(root, marker, &mut |_| {})
}

/// `write_marker` with a hook run on the temporary right after its creation,
/// before its owner and mode are set, so a test can read what a crash there
/// would leave.
fn write_marker_with(
    root: &Path,
    marker: Option<&Marker>,
    after_create: &mut dyn FnMut(&std::fs::File),
) -> std::io::Result<()> {
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
        // **Created narrow and never through a link** (the custody audit's
        // G14): at `0600` whatever the umask, so a crash before the mode is
        // set leaves no group- or world-writable entry in the root for
        // `judge_entries` to refuse every verb over, and `O_NOFOLLOW` keeps a
        // link at the name from being followed.
        let mut file = {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC)
                .open(&temporary)?
        };
        after_create(&file);
        // **Root's group set, never the invoker's** (Codex on #94), as a
        // defence: nothing judges the marker's group, but every file this
        // crate creates takes its owner explicitly, so none depends on the
        // invoking shell's effective gid. Root alone can set gid 0, and a
        // test running as its user writes the marker under its own.
        if nix::unistd::geteuid().is_root() {
            nix::unistd::fchown(
                file.as_fd(),
                Some(nix::unistd::Uid::from_raw(0)),
                Some(nix::unistd::Gid::from_raw(0)),
            )
            .map_err(std::io::Error::from)?;
        }
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

#[cfg(test)]
pub(crate) mod tests {
    /// **The marker's temporary is created narrow** (the custody audit's
    /// G14): read right after its creation, before its owner and mode are
    /// set, it carries no group or other bit, so a crash there leaves nothing
    /// a later verb's judgment of the root would refuse. Perturbation: create
    /// it with `File::create` again and it carries the umask's bits.
    #[test]
    fn the_marker_temporary_is_created_narrow() {
        let scratch = crate::scratch::Scratch(
            std::env::temp_dir().join(format!("weaver-admin-marker-narrow-{}", std::process::id())),
        );
        let root = scratch.0.clone();
        std::fs::create_dir_all(&root).unwrap();
        let mut seen = None;
        write_marker_with(
            &root,
            Some(&Marker::Open { run: "r-1".into() }),
            &mut |file| {
                use std::os::unix::fs::PermissionsExt;
                seen = Some(file.metadata().unwrap().permissions().mode() & 0o777);
            },
        )
        .unwrap();
        let mode = seen.expect("the hook ran");
        assert_eq!(mode & 0o077, 0, "created at {mode:o}");
        assert_eq!(read_marker(&root), Some(Marker::Open { run: "r-1".into() }));
    }

    /// **A marker that stands and does not read fails closed** (the custody
    /// audit's G8): every arm of `load_marker` that finds a marker it cannot
    /// read is an error, never no marker and never another marker; absent
    /// alone is none. Each damaged marker but the torn one would read as a
    /// marker were its arm gone: the unknown state, the missing run and the
    /// non-UTF-8 run are whole JSON, the link points at a sound marker, and
    /// the marker past the bound is a sound one padded with whitespace.
    /// Perturbations, one per arm: read the torn one as absent; read an
    /// unknown state as absent; read an open that fails, the link's `ELOOP`
    /// among them, as absent; read a missing run as the empty run; read the
    /// bytes lossily as text; read past the bound as a closed marker. Each
    /// fails the case it names.
    #[test]
    fn a_marker_that_does_not_read_is_an_error() {
        let scratch = crate::scratch::Scratch(
            std::env::temp_dir().join(format!("weaver-admin-marker-torn-{}", std::process::id())),
        );
        let root = scratch.0.clone();
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(load_marker(&root), Ok(None), "absent is none");
        let sound = br#"{"run":"r-1","state":"closed"}"#;
        let mut past_bound = sound.to_vec();
        past_bound.resize(MARKER_BOUND as usize + 1, b' ');
        let mut not_text = br#"{"run":"r-"#.to_vec();
        not_text.extend_from_slice(&[0xff, 0xfe]);
        not_text.extend_from_slice(br#"","state":"open"}"#);
        let cases: [(&str, Vec<u8>); 5] = [
            ("a torn marker", br#"{"run":"#.to_vec()),
            (
                "an unknown state",
                br#"{"run":"r-1","state":"bogus"}"#.to_vec(),
            ),
            ("a missing run", br#"{"state":"open"}"#.to_vec()),
            ("bytes that are not UTF-8", not_text),
            ("a marker past its bound", past_bound),
        ];
        for (what, bytes) in cases {
            std::fs::write(root.join(MARKER), &bytes).unwrap();
            assert!(
                load_marker(&root).is_err(),
                "{what}: expected an error, read {:?}",
                load_marker(&root)
            );
        }
        // A link at the marker's name, to a sound marker, is never followed.
        std::fs::remove_file(root.join(MARKER)).unwrap();
        std::fs::write(root.join("elsewhere"), sound).unwrap();
        std::os::unix::fs::symlink(root.join("elsewhere"), root.join(MARKER)).unwrap();
        assert!(
            load_marker(&root).is_err(),
            "a link is an error, read {:?}",
            load_marker(&root)
        );
        // The sound marker itself reads, so each case above failed on its
        // own arm.
        std::fs::remove_file(root.join(MARKER)).unwrap();
        std::fs::write(root.join(MARKER), sound).unwrap();
        assert_eq!(
            load_marker(&root),
            Ok(Some(Marker::Closed { run: "r-1".into() }))
        );
    }

    /// **A new manifest takes its owner and group explicitly** (Codex on
    /// #94): the first line creates the manifest and sets its owner and
    /// group to the expected owner's, whatever the creating process's
    /// effective gid, before its mode and the directory's sync; a later line,
    /// the manifest standing, sets nothing. The chown is counted through the
    /// seam, so that case needs no second group and never skips; the
    /// production closure in `append_line` is then pinned against a
    /// supplementary group of this test's user, which an unprivileged
    /// process may `fchown` its own file to, so the manifest's group can
    /// only be that group if the closure ran. A box whose test user holds
    /// no second group skips that half, saying so. Perturbations: drop the
    /// call and the count reads zero; make `append_line`'s closure do
    /// nothing and the new manifest keeps the effective gid.
    #[test]
    fn a_new_manifest_takes_its_owner_and_group_explicitly() {
        let scratch = crate::scratch::Scratch(std::env::temp_dir().join(format!(
            "weaver-admin-manifest-owner-{}",
            std::process::id()
        )));
        let dir = scratch.0.clone();
        std::fs::create_dir_all(&dir).unwrap();
        let dir_fd = open_directory(&dir).unwrap();
        let mine = Owner {
            uid: nix::unistd::getuid().as_raw(),
            gid: nix::unistd::getgid().as_raw(),
        };
        let line = ManifestLine {
            ordinal: 1,
            digest: "ab".into(),
            name: "ab.save-point".into(),
            stamp: Stamp {
                run: "r-1".into(),
                sequence: 1,
                turn: 0,
                schema: String::new(),
                wall_ns: 0,
            },
            position: None,
            arrived: Arrival::Leave,
        };
        let mut chowned = Vec::new();
        let mut counting = |_: BorrowedFd<'_>, uid: u32, gid: u32| -> nix::Result<()> {
            chowned.push((uid, gid));
            Ok(())
        };
        append_line_with(dir_fd.as_fd(), mine, &line, &mut counting).unwrap();
        let second = ManifestLine {
            ordinal: 2,
            ..line.clone()
        };
        append_line_with(dir_fd.as_fd(), mine, &second, &mut counting).unwrap();
        assert_eq!(
            chowned,
            [(mine.uid, mine.gid)],
            "set once, at the creation, to the expected owner"
        );
        assert_eq!(read_manifest(dir_fd.as_fd(), mine).unwrap().len(), 2);
        // The production closure: a group this user holds that is not its
        // effective gid, which a new file would take without the chown.
        let egid = nix::unistd::getegid();
        let Some(other) = nix::unistd::getgroups()
            .unwrap()
            .into_iter()
            .find(|group| *group != egid)
        else {
            eprintln!(
                "skipped: this test's user holds no group beside its effective gid, so the production chown cannot be told from none"
            );
            return;
        };
        let grouped = dir.join("grouped");
        std::fs::create_dir_all(&grouped).unwrap();
        let grouped_fd = open_directory(&grouped).unwrap();
        let theirs = Owner {
            uid: mine.uid,
            gid: other.as_raw(),
        };
        append_line(grouped_fd.as_fd(), theirs, &line).unwrap();
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(
                std::fs::metadata(grouped.join(MANIFEST)).unwrap().gid(),
                other.as_raw(),
                "the new manifest takes the owner's group, not the effective gid"
            );
        }
        assert_eq!(read_manifest(grouped_fd.as_fd(), theirs).unwrap().len(), 1);
    }

    /// **An adopted entry is judged, then made durable** (Codex on #94): an
    /// entry that stands is judged as a load judges it and the directory
    /// synced once before any manifest line can name it; a name with no
    /// entry is answered none and syncs nothing. The sync is counted through
    /// the helper's seam. Perturbation: drop the sync and the count reads
    /// zero for the standing entry.
    #[test]
    fn an_adopted_entry_is_judged_then_synced() {
        let scratch = crate::scratch::Scratch(
            std::env::temp_dir().join(format!("weaver-admin-adopt-durably-{}", std::process::id())),
        );
        let dir = scratch.0.clone();
        std::fs::create_dir_all(&dir).unwrap();
        let dir_fd = open_directory(&dir).unwrap();
        let mine = (
            nix::unistd::getuid().as_raw(),
            nix::unistd::getgid().as_raw(),
        );
        let bytes = save_point("r-8", 8, 0, 8_000_000_000, b"adopted");
        let name = published_name(&judge(&bytes).unwrap());
        std::fs::write(dir.join(&name), &bytes).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir.join(&name), std::fs::Permissions::from_mode(0o640))
                .unwrap();
        }
        let mut syncs = 0;
        let mut counting = |_: BorrowedFd<'_>| -> std::io::Result<()> {
            syncs += 1;
            Ok(())
        };
        let adopted = adopt_durably(dir_fd.as_fd(), &name, mine, &mut counting)
            .expect("the standing entry judges")
            .expect("it stands");
        assert_eq!(adopted.1.digest, judge(&bytes).unwrap().digest);
        assert!(
            adopt_durably(dir_fd.as_fd(), "absent.save-point", mine, &mut counting)
                .unwrap()
                .is_none()
        );
        assert_eq!(syncs, 1, "synced once, for the entry that stands");
    }

    /// **A published file is judged by its group as by its owner** (Codex on
    /// #94): a save point that stands under another group than the access
    /// group, which the operator and the connector could not read, is refused
    /// by `restore` naming it and by a load selecting it; the same file
    /// judged against its own group reads. The file is written under this
    /// test's own uid and gid and the judgment given a gid one off, so no
    /// supplementary group is needed and the case never skips. Perturbation:
    /// drop the gid comparison and the wrong group reads.
    #[test]
    fn a_published_file_under_another_group_is_refused() {
        let scratch = crate::scratch::Scratch(
            std::env::temp_dir().join(format!("weaver-admin-group-{}", std::process::id())),
        );
        let dir = scratch.0.clone();
        std::fs::create_dir_all(&dir).unwrap();
        let dir_fd = open_directory(&dir).unwrap();
        let mine = Owner {
            uid: nix::unistd::getuid().as_raw(),
            gid: nix::unistd::getgid().as_raw(),
        };
        let wrong = (mine.uid, mine.gid.wrapping_add(1));
        let bytes = save_point("r-7", 7, 0, 7_000_000_000, b"grouped");
        let name = published_name(&judge(&bytes).unwrap());
        std::fs::write(dir.join(&name), &bytes).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir.join(&name), std::fs::Permissions::from_mode(0o640))
                .unwrap();
        }
        assert!(
            name_at_restore(dir_fd.as_fd(), wrong, &name, mine).is_err(),
            "restore refuses a file under another group"
        );
        let named = name_at_restore(dir_fd.as_fd(), (mine.uid, mine.gid), &name, mine)
            .expect("its own group reads");
        assert!(
            select(dir_fd.as_fd(), wrong, Some(&named.name), mine).is_err(),
            "a load refuses it under another group"
        );
        assert!(
            select(
                dir_fd.as_fd(),
                (mine.uid, mine.gid),
                Some(&named.name),
                mine
            )
            .unwrap()
            .is_some(),
            "and selects it under its own"
        );
    }

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
        // One member process whose count follows the covered position, as
        // a member that took them in order would have.
        save_point_taken(run, sequence, turn, wall_ns, image, 1, sequence)
    }

    /// A save point as `save_point` builds it, taken by member process `pid`
    /// as its `ordinal`th.
    pub(crate) fn save_point_taken(
        run: &str,
        sequence: u64,
        turn: u64,
        wall_ns: u64,
        image: &[u8],
        pid: u64,
        ordinal: u64,
    ) -> Vec<u8> {
        let header = serde_json::json!({
            "weaver-save-point": 1,
            "run": run,
            "sequence": sequence,
            "turn": turn,
            "schema": "0".repeat(64),
            "image": image.len(),
            "taken": {"pid": pid, "ordinal": ordinal, "wall_ns": wall_ns.to_string()},
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

    /// **Several room entries publish in the stamp's order**, per Spec
    /// section 6 (Codex on #94, round 5): an older file left unreported and
    /// a newer one reported in the same verb take their ordinals by kind and
    /// then the member process's count, so the newer is the latest the
    /// manifest names whatever order the room lists them in. The newer's
    /// digest is chosen to sort first, which is the order the room reader
    /// yields. Perturbation: drop the sort in `publish` and the older file
    /// is minted last and selected as latest.
    #[test]
    fn several_room_entries_publish_in_the_stamps_order() {
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
            open_directory(&room).unwrap().as_fd(),
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
            "ordinals follow the sequence, the reported one last"
        );
        let latest = select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine)
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
            open_directory(&room).unwrap().as_fd(),
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
        let latest = select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine)
            .unwrap()
            .expect("a latest");
        assert_eq!(latest.line.digest, behind_digest);
        // **The reported key orders ahead of the count** (the #99 area 1
        // review): the reported save point is minted last even where its
        // `taken.ordinal` is below a recovered one's, so "reported last" is
        // pinned on its own and not through the count agreeing with it.
        // Perturbation: sort by the count alone and the recovered one is
        // minted last and selected latest.
        let room = base.join("room-3");
        let dir = base.join("decl-3");
        std::fs::create_dir_all(&room).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let dir_fd = open_directory(&dir).unwrap();
        let counted_high =
            save_point_taken("r-1", 9, 2, 9_000_000_000, b"recovered, count 9", 1, 9);
        let counted_high_digest = judge(&counted_high).unwrap().digest;
        let counted_low = save_point_taken("r-1", 4, 1, 4_000_000_000, b"reported, count 4", 1, 4);
        let counted_low_digest = judge(&counted_low).unwrap().digest;
        std::fs::write(
            room.join(format!("{counted_high_digest}{SUFFIX}")),
            &counted_high,
        )
        .unwrap();
        std::fs::write(
            room.join(format!("{counted_low_digest}{SUFFIX}")),
            &counted_low,
        )
        .unwrap();
        let report = weaver_types::SavePointReport {
            save_point: counted_low_digest.clone(),
            name: format!("{counted_low_digest}{SUFFIX}"),
            run: weaver_types::RunId("r-1".into()),
            sequence: 4,
            turn: 1,
            event_run: weaver_types::RunId("r-1".into()),
            position: 4,
        };
        let lines = publish(
            open_directory(&room).unwrap().as_fd(),
            me,
            dir_fd.as_fd(),
            owner,
            mine,
            &[(report, Arrival::Leave)],
        )
        .unwrap();
        let minted: Vec<(u64, &str, Arrival)> = lines
            .iter()
            .map(|line| (line.ordinal, line.digest.as_str(), line.arrived))
            .collect();
        assert_eq!(
            minted,
            vec![
                (1, counted_high_digest.as_str(), Arrival::Recovered),
                (2, counted_low_digest.as_str(), Arrival::Leave),
            ],
            "the reported one is last though its count is lower"
        );
        let latest = select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine)
            .unwrap()
            .expect("a latest");
        assert_eq!(latest.line.digest, counted_low_digest);
    }

    /// **A FIFO in the room does not wedge the publication** (the custody
    /// audit's G1): the member makes a FIFO under a finished name; the
    /// publication opens it without blocking, refuses it as no regular file,
    /// appends nothing and leaves it in place, answering at once. The scan
    /// refuses a FIFO it finds, so a second case has the member make the
    /// FIFO after the scan, at a name the scan judged a sound save point:
    /// the copy's own open must not block either, and the verb defers. Each
    /// runs on a thread with a bound, so a perturbed open fails the test
    /// rather than hanging it. Perturbations: open without `O_NONBLOCK` at
    /// the scan and the first thread never answers; at the copy, and the
    /// second never answers.
    #[test]
    fn a_fifo_in_the_room_does_not_wedge_the_publication() {
        let scratch = crate::scratch::Scratch(
            std::env::temp_dir().join(format!("weaver-admin-room-fifo-{}", std::process::id())),
        );
        let base = scratch.0.clone();
        let room = base.join("room");
        let dir = base.join("published");
        std::fs::create_dir_all(&room).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let fifo = room.join(format!("{}{SUFFIX}", "a".repeat(64)));
        nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::from_bits_truncate(0o600)).unwrap();
        let me = nix::unistd::getuid().as_raw();
        let mine = Owner {
            uid: me,
            gid: nix::unistd::getgid().as_raw(),
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let (room_path, dir_path) = (room.clone(), dir.clone());
        std::thread::spawn(move || {
            let room_fd = open_directory(&room_path).unwrap();
            let dir_fd = open_directory(&dir_path).unwrap();
            let answered = publish(
                room_fd.as_fd(),
                me,
                dir_fd.as_fd(),
                (mine.uid, mine.gid),
                mine,
                &[],
            )
            .map(|lines| lines.len());
            let _ = tx.send(answered);
        });
        let answered = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("the publication answers, never blocked on the FIFO");
        assert_eq!(answered, Ok(0), "nothing appended");
        assert!(fifo.exists(), "the FIFO is left in place");
        // The FIFO made after the scan, under a name it judged sound.
        let room = base.join("room-swapped");
        let dir = base.join("published-swapped");
        std::fs::create_dir_all(&room).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let bytes = save_point("r-1", 1, 0, 1_000_000_000, b"swapped for a FIFO");
        let swapped = room.join(format!("{}{SUFFIX}", judge(&bytes).unwrap().digest));
        std::fs::write(&swapped, &bytes).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let (room_path, dir_path, fifo) = (room.clone(), dir.clone(), swapped.clone());
        std::thread::spawn(move || {
            let room_fd = open_directory(&room_path).unwrap();
            let dir_fd = open_directory(&dir_path).unwrap();
            let answered = publish_with(
                room_fd.as_fd(),
                me,
                dir_fd.as_fd(),
                (mine.uid, mine.gid),
                mine,
                &[],
                &mut PublishHooks {
                    after_scan: &mut || {
                        std::fs::remove_file(&fifo).unwrap();
                        nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::from_bits_truncate(0o600))
                            .unwrap();
                    },
                    available: &mut available_bytes,
                    cap: PUBLISH_CAP,
                },
            )
            .map(|(lines, deferred)| (lines.len(), deferred));
            let _ = tx.send(answered);
        });
        let answered = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("the copy answers, never blocked on the FIFO made after the scan");
        assert_eq!(answered, Ok((0, true)), "nothing appended, and deferred");
        assert!(
            swapped.exists(),
            "the FIFO made after the scan is left in place"
        );
    }

    /// **A listing that fails refuses the selection** (the custody audit's
    /// G9): with no manifest, `select` lists the directory to tell a first
    /// load from files with no manifest, and a listing that fails is never
    /// an empty directory. Perturbation: read the failure as an empty listing
    /// and the selection answers a first load.
    #[test]
    fn a_listing_that_fails_refuses_the_selection() {
        let scratch = crate::scratch::Scratch(
            std::env::temp_dir().join(format!("weaver-admin-list-fails-{}", std::process::id())),
        );
        let dir = scratch.0.clone();
        std::fs::create_dir_all(&dir).unwrap();
        let dir_fd = open_directory(&dir).unwrap();
        let mine = Owner {
            uid: nix::unistd::getuid().as_raw(),
            gid: nix::unistd::getgid().as_raw(),
        };
        let mut failing =
            |_: BorrowedFd<'_>| -> nix::Result<Vec<String>> { Err(nix::errno::Errno::EIO) };
        assert!(
            select_with(
                dir_fd.as_fd(),
                (mine.uid, mine.gid),
                None,
                mine,
                &mut failing
            )
            .is_err()
        );
        assert!(matches!(
            select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine),
            Ok(None)
        ));
    }

    /// **The room opens through the territory without following a link**
    /// (the custody audit's G12): a `state` that is a link to a directory
    /// holding a save point refuses; a real one opens; an absent one is no
    /// room. Perturbation: drop `O_NOFOLLOW` and the link opens.
    #[test]
    fn the_room_opens_without_following_a_link() {
        let scratch = crate::scratch::Scratch(
            std::env::temp_dir().join(format!("weaver-admin-room-link-{}", std::process::id())),
        );
        let territory = scratch.0.join("territory");
        let elsewhere = scratch.0.join("elsewhere");
        std::fs::create_dir_all(&territory).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        let territory_fd = open_directory(&territory).unwrap();
        assert!(matches!(open_room(territory_fd.as_fd()), Ok(None)));
        std::os::unix::fs::symlink(&elsewhere, territory.join(ROOM)).unwrap();
        assert!(open_room(territory_fd.as_fd()).is_err(), "a link refuses");
        std::fs::remove_file(territory.join(ROOM)).unwrap();
        std::fs::create_dir(territory.join(ROOM)).unwrap();
        assert!(matches!(open_room(territory_fd.as_fd()), Ok(Some(_))));
    }

    /// A room of `count` finished save points, their clocks ascending, and a
    /// published directory beside it; answers the room, the directory, the
    /// owner and the room's names and digests oldest first.
    fn room_of_save_points(
        tag: &str,
        count: u64,
    ) -> (
        crate::scratch::Scratch,
        std::path::PathBuf,
        std::path::PathBuf,
        Owner,
        Vec<(String, String)>,
    ) {
        let scratch = crate::scratch::Scratch(
            std::env::temp_dir().join(format!("weaver-admin-{tag}-{}", std::process::id())),
        );
        let room = scratch.0.join("room");
        let dir = scratch.0.join("published");
        std::fs::create_dir_all(&room).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let mine = Owner {
            uid: nix::unistd::getuid().as_raw(),
            gid: nix::unistd::getgid().as_raw(),
        };
        let mut names = Vec::new();
        for sequence in 1..=count {
            let bytes = save_point("r-1", sequence, 0, sequence * 1_000_000_000, b"room");
            let judged = judge(&bytes).unwrap();
            let name = format!("{}{SUFFIX}", judged.digest);
            std::fs::write(room.join(&name), &bytes).unwrap();
            names.push((name, judged.digest));
        }
        (scratch, room, dir, mine, names)
    }

    /// **A verb publishes at most its cap, oldest first, and stops** (the
    /// custody audit's G23): a room of three under a cap of two publishes the
    /// two oldest and leaves the newest for the next verb, which publishes
    /// it; the newest is never published before an older one, so the
    /// highest ordinal stays the last taken. Perturbation: drop the cap and
    /// all three publish at once.
    #[test]
    fn a_verb_publishes_at_most_its_cap_oldest_first() {
        let (_scratch, room, dir, mine, names) = room_of_save_points("room-cap", 3);
        let dir_fd = open_directory(&dir).unwrap();
        let publish_capped = |cap| {
            publish_with(
                open_directory(&room).unwrap().as_fd(),
                mine.uid,
                dir_fd.as_fd(),
                (mine.uid, mine.gid),
                mine,
                &[],
                &mut PublishHooks {
                    after_scan: &mut || {},
                    available: &mut available_bytes,
                    cap,
                },
            )
            .unwrap()
        };
        let (first, deferred) = publish_capped(2);
        let first: Vec<String> = first.into_iter().map(|l| l.digest).collect();
        assert_eq!(first, [names[0].1.clone(), names[1].1.clone()]);
        assert!(deferred, "the verb says it left one");
        assert!(room.join(&names[2].0).exists(), "the newest waits");
        let (second, deferred) = publish_capped(2);
        let second: Vec<String> = second.into_iter().map(|l| l.digest).collect();
        assert_eq!(second, [names[2].1.clone()], "and goes next, highest");
        assert!(!deferred, "the room drained");
    }

    /// **The scan reads stamps, and the whole reads stop at the cap** (Codex
    /// on #94, at ab8acef, and the custody review): a room of forty finished
    /// files under the cap of 32 reads each one's stamp line at the scan and
    /// reads whole only the 32 it copies, so the time a member's files cost a
    /// verb is bounded by the cap and not by the room. Counted through a
    /// test-only tally of whole reads. Perturbation: judge each file whole at
    /// the scan again and the tally passes the cap.
    #[test]
    fn the_whole_reads_stop_at_the_cap() {
        let (_scratch, room, dir, mine, _) = room_of_save_points("room-forty", 40);
        let dir_fd = open_directory(&dir).unwrap();
        WHOLE_READS.with(|count| count.set(0));
        let (lines, deferred) = publish_noting_deferral(
            open_directory(&room).unwrap().as_fd(),
            mine.uid,
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            mine,
            &[],
        )
        .unwrap();
        assert_eq!(lines.len(), PUBLISH_CAP);
        assert!(deferred);
        let whole = WHOLE_READS.with(|count| count.get());
        assert!(
            whole <= PUBLISH_CAP,
            "{whole} whole reads for a cap of {PUBLISH_CAP}"
        );
    }

    /// **A copy is made only where the space stands free, and the first
    /// file that does not fit stops the verb** (the custody audit's G23):
    /// the older of two room files is the larger, and the filesystem reads
    /// exactly enough free for the newer and the reserve, so the older does
    /// not fit and the newer would; nothing is copied, the verb says it
    /// deferred, and both stay for the next verb, since the newer published
    /// past the older would let the older be minted above it later.
    /// Perturbations: drop the free-space look and both publish; skip the
    /// file that does not fit instead of stopping and the newer publishes.
    #[test]
    fn a_copy_is_made_only_where_the_space_stands_free() {
        let (_scratch, room, dir, mine, _) = room_of_save_points("room-space", 0);
        let older = save_point("r-1", 1, 0, 1_000_000_000, &[b'o'; 4096]);
        let newer = save_point("r-1", 2, 0, 2_000_000_000, b"newer, smaller");
        assert!(older.len() > newer.len());
        let mut names = Vec::new();
        for bytes in [&older, &newer] {
            let name = format!("{}{SUFFIX}", judge(bytes).unwrap().digest);
            std::fs::write(room.join(&name), bytes).unwrap();
            names.push(name);
        }
        let dir_fd = open_directory(&dir).unwrap();
        let free = newer.len() as u64 + PUBLISH_RESERVE;
        let (lines, deferred) = publish_with(
            open_directory(&room).unwrap().as_fd(),
            mine.uid,
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            mine,
            &[],
            &mut PublishHooks {
                after_scan: &mut || {},
                available: &mut |_| Ok(free),
                cap: PUBLISH_CAP,
            },
        )
        .unwrap();
        assert!(
            lines.is_empty(),
            "nothing copied, the newer not past the older"
        );
        assert!(deferred, "and the verb says so");
        assert!(names.iter().all(|name| room.join(name).exists()));
    }

    /// **A standing line whose target differs stops the publication** (Codex
    /// on #94 at 7a25db2): a crash between the line's append and the room
    /// copy's removal leaves the copy, and the target is later damaged; the
    /// room's copy may be the newest sound state, so the verb defers and
    /// the copy stays, and every later save point is left with it: a newer
    /// room file taken after it stays in the room and no line is appended.
    /// Perturbations: skip the entry, the deferral dropped, and nothing is
    /// deferred; skip it with the deferral kept and the newer file is
    /// published past it.
    #[test]
    fn a_standing_line_whose_target_differs_defers() {
        let scratch = crate::scratch::Scratch(std::env::temp_dir().join(format!(
            "weaver-admin-standing-differs-{}",
            std::process::id()
        )));
        let base = scratch.0.clone();
        let room = base.join("room");
        let dir = base.join("published");
        std::fs::create_dir_all(&room).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let dir_fd = open_directory(&dir).unwrap();
        let me = nix::unistd::getuid().as_raw();
        let mine = Owner {
            uid: me,
            gid: nix::unistd::getgid().as_raw(),
        };
        let bytes = save_point("r-1", 4, 1, 1_000_000_000, b"newest");
        let judged = judge(&bytes).unwrap();
        let room_name = format!("{}{SUFFIX}", judged.digest);
        std::fs::write(room.join(&room_name), &bytes).unwrap();
        let publish_now = || {
            publish_noting_deferral(
                open_directory(&room).unwrap().as_fd(),
                me,
                dir_fd.as_fd(),
                (mine.uid, mine.gid),
                mine,
                &[],
            )
            .unwrap()
        };
        let (lines, _) = publish_now();
        assert_eq!(lines.len(), 1);
        // The crash left the room's copy, and the target is damaged; a newer
        // save point, taken after it, waits in the room beside it.
        std::fs::write(room.join(&room_name), &bytes).unwrap();
        std::fs::write(
            dir.join(&lines[0].name),
            save_point("r-1", 4, 1, 1_000_000_000, b"damaged"),
        )
        .unwrap();
        let later = save_point("r-1", 5, 1, 2_000_000_000, b"taken after the standing one");
        let later_name = format!("{}{SUFFIX}", judge(&later).unwrap().digest);
        std::fs::write(room.join(&later_name), &later).unwrap();
        let (appended, deferred) = publish_now();
        assert!(appended.is_empty(), "nothing published past it");
        assert!(deferred, "the verb defers, so a load refuses");
        assert!(room.join(&room_name).exists(), "the room's copy stays");
        assert!(room.join(&later_name).exists(), "the newer file is left");
        assert_eq!(
            read_manifest(dir_fd.as_fd(), mine).unwrap().len(),
            1,
            "no line appended"
        );
    }

    /// **Recovered save points order by the member process's own count, and
    /// those of two processes refuse** (the operator's ruling of 2026-10-08
    /// on #99, N3): one process's two files, the second taken after a
    /// restore's first distillate landed so that its covered run changed
    /// and its sequence fell, and the clock stepped back between them,
    /// publish in count order, the later the latest; files of two member
    /// processes, or two of one count, refuse and stay in the room. A
    /// reported file counts as any other: a recovered file of one process
    /// beside a reported one of another refuses too, the process set being
    /// built from every entry. Perturbations: order by the covered sequence
    /// and the earlier is minted last; key the refusal on the covered run
    /// and the first room refuses; drop the process refusal and the second
    /// room publishes; build the process set from the recovered files alone
    /// and the reported room publishes.
    #[test]
    fn recovered_save_points_order_by_the_members_count_and_two_processes_refuse() {
        let scratch = crate::scratch::Scratch(
            std::env::temp_dir().join(format!("weaver-admin-count-order-{}", std::process::id())),
        );
        let base = scratch.0.clone();
        let me = nix::unistd::getuid().as_raw();
        let mine = Owner {
            uid: me,
            gid: nix::unistd::getgid().as_raw(),
        };
        let room_of = |tag: &str, files: &[Vec<u8>]| {
            let room = base.join(format!("room-{tag}"));
            let dir = base.join(format!("published-{tag}"));
            std::fs::create_dir_all(&room).unwrap();
            std::fs::create_dir_all(&dir).unwrap();
            let mut digests = Vec::new();
            for bytes in files {
                let digest = judge(bytes).unwrap().digest;
                std::fs::write(room.join(format!("{digest}{SUFFIX}")), bytes).unwrap();
                digests.push(digest);
            }
            (room, dir, digests)
        };
        let publish_room = |room: &std::path::Path, dir_fd: BorrowedFd<'_>| {
            publish(
                open_directory(room).unwrap().as_fd(),
                me,
                dir_fd,
                (mine.uid, mine.gid),
                mine,
                &[],
            )
        };
        // Taken first: the restored prior run's position, a late clock.
        let earlier = save_point_taken("r-old", 900, 4, 9_000_000_000, b"earlier", 77, 0);
        // Taken second: the new run's first position, an early clock.
        let later = save_point_taken("r-new", 3, 1, 1_000_000_000, b"later", 77, 1);
        let (room, dir, digests) = room_of("one-process", &[earlier, later]);
        let dir_fd = open_directory(&dir).unwrap();
        let lines = publish_room(&room, dir_fd.as_fd()).expect("one process publishes");
        let minted: Vec<&str> = lines.iter().map(|line| line.digest.as_str()).collect();
        assert_eq!(minted, [digests[0].as_str(), digests[1].as_str()]);
        let latest = select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine)
            .unwrap()
            .expect("a latest");
        assert_eq!(
            latest.line.digest, digests[1],
            "the later taken is the latest"
        );
        // Two member processes: nothing orders them.
        let one = save_point_taken("r-1", 5, 1, 1_000_000_000, b"process one", 77, 0);
        let two = save_point_taken("r-1", 6, 1, 2_000_000_000, b"process two", 78, 0);
        let (room, dir, digests) = room_of("two-processes", &[one, two]);
        let dir_fd = open_directory(&dir).unwrap();
        assert_eq!(
            publish_room(&room, dir_fd.as_fd()).err(),
            Some(LifecycleRefusal::BoundaryUnverified)
        );
        for digest in &digests {
            assert!(room.join(format!("{digest}{SUFFIX}")).exists());
        }
        // A recovered file of one process and a reported file of another:
        // the report does not exempt its file from the process count.
        let recovered = save_point_taken("r-1", 5, 1, 1_000_000_000, b"recovered, 77", 77, 0);
        let reported = save_point_taken("r-1", 6, 1, 2_000_000_000, b"reported, 78", 78, 0);
        let (room, dir, digests) = room_of("reported-other-process", &[recovered, reported]);
        let dir_fd = open_directory(&dir).unwrap();
        let report = weaver_types::SavePointReport {
            save_point: digests[1].clone(),
            name: format!("{}{SUFFIX}", digests[1]),
            run: weaver_types::RunId("r-1".into()),
            sequence: 6,
            turn: 1,
            event_run: weaver_types::RunId("r-1".into()),
            position: 6,
        };
        assert_eq!(
            publish(
                open_directory(&room).unwrap().as_fd(),
                me,
                dir_fd.as_fd(),
                (mine.uid, mine.gid),
                mine,
                &[(report, Arrival::Leave)],
            )
            .err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a reported file of another process refuses"
        );
        for digest in &digests {
            assert!(room.join(format!("{digest}{SUFFIX}")).exists());
        }
        // One process, one count twice: a reused pid, unorderable.
        let first = save_point_taken("r-1", 5, 1, 1_000_000_000, b"count once", 77, 4);
        let again = save_point_taken("r-1", 6, 1, 2_000_000_000, b"count again", 77, 4);
        let (room, dir, _) = room_of("one-count-twice", &[first, again]);
        let dir_fd = open_directory(&dir).unwrap();
        assert_eq!(
            publish_room(&room, dir_fd.as_fd()).err(),
            Some(LifecycleRefusal::BoundaryUnverified)
        );
    }

    /// **The scan and the copy agree on what a room file is** (the #99 area
    /// 1 review, K2): an uppercase-hex name is no finished name, ignored at
    /// the scan and never a deferral; a sound file another uid owns, and a
    /// save-point format this admin does not read, refuse rather than being
    /// passed over while an older save point loads. Perturbations: admit
    /// uppercase hex and the first room defers; leave another uid's file in
    /// place and the second publishes nothing silently; drop the format look
    /// and the third is passed over.
    #[test]
    fn the_scan_ignores_no_finished_name_and_refuses_what_may_be_newer() {
        let scratch = crate::scratch::Scratch(
            std::env::temp_dir().join(format!("weaver-admin-scan-verdicts-{}", std::process::id())),
        );
        let base = scratch.0.clone();
        let me = nix::unistd::getuid().as_raw();
        let mine = Owner {
            uid: me,
            gid: nix::unistd::getgid().as_raw(),
        };
        let room_with = |tag: &str, name: &str, bytes: &[u8]| {
            let room = base.join(format!("room-{tag}"));
            let dir = base.join(format!("published-{tag}"));
            std::fs::create_dir_all(&room).unwrap();
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(room.join(name), bytes).unwrap();
            (room, dir)
        };
        let publish_as = |room: &std::path::Path, dir: &std::path::Path, member: u32| {
            let dir_fd = open_directory(dir).unwrap();
            publish_noting_deferral(
                open_directory(room).unwrap().as_fd(),
                member,
                dir_fd.as_fd(),
                (mine.uid, mine.gid),
                mine,
                &[],
            )
        };
        let bytes = save_point("r-1", 1, 0, 1_000_000_000, b"sound");
        let digest = judge(&bytes).unwrap().digest;
        // Uppercase: no finished name, so nothing is published or deferred.
        let upper = format!("{}{SUFFIX}", digest.to_uppercase());
        let (room, dir) = room_with("upper", &upper, &bytes);
        let (lines, deferred) = publish_as(&room, &dir, me).unwrap();
        assert!(lines.is_empty() && !deferred, "ignored, never deferred");
        assert!(room.join(&upper).exists());
        // Another uid's sound file: may be newer state, so it refuses.
        let lower = format!("{digest}{SUFFIX}");
        let (room, dir) = room_with("other-uid", &lower, &bytes);
        assert_eq!(
            publish_as(&room, &dir, me.wrapping_add(1)).err(),
            Some(LifecycleRefusal::BoundaryUnverified)
        );
        // A format this admin does not read: refuses.
        let newer = String::from_utf8(bytes.clone()).unwrap().replacen(
            "\"weaver-save-point\":1",
            "\"weaver-save-point\":2",
            1,
        );
        assert!(newer.contains("\"weaver-save-point\":2"));
        let (room, dir) = room_with("newer-format", &lower, newer.as_bytes());
        assert_eq!(
            publish_as(&room, &dir, me).err(),
            Some(LifecycleRefusal::BoundaryUnverified)
        );
    }

    /// **The save point's bound is one gibibyte in both readers** (the
    /// operator's ruling of 2026-10-08 on #1): the member's own constant is
    /// pinned to the same number in weaver-state, so the two are held equal
    /// by naming it. Perturbation: change either constant.
    #[test]
    fn the_save_point_bound_is_one_gibibyte() {
        assert_eq!(SAVE_POINT_BOUND, 1_073_741_824);
    }

    /// **A room file past the bound refuses the publication** (the #94
    /// survey's S4): the member never writes one, so one standing is a fault
    /// to clear, and left out of the order it would sit in the room while a
    /// load restored older state. The file is sparse, so the test writes no
    /// gibibyte. Perturbation: leave it in place and answer no entry again,
    /// and the publication reads.
    #[test]
    fn a_room_file_past_the_bound_refuses_the_publication() {
        let scratch = crate::scratch::Scratch(std::env::temp_dir().join(format!(
            "weaver-admin-room-past-bound-{}",
            std::process::id()
        )));
        let base = scratch.0.clone();
        let room = base.join("room");
        let dir = base.join("published");
        std::fs::create_dir_all(&room).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let large =
            std::fs::File::create(room.join(format!("{}{SUFFIX}", "a".repeat(64)))).unwrap();
        large.set_len(SAVE_POINT_BOUND + 1).unwrap();
        drop(large);
        let me = nix::unistd::getuid().as_raw();
        let mine = Owner {
            uid: me,
            gid: nix::unistd::getgid().as_raw(),
        };
        let dir_fd = open_directory(&dir).unwrap();
        assert_eq!(
            publish(
                open_directory(&room).unwrap().as_fd(),
                me,
                dir_fd.as_fd(),
                (mine.uid, mine.gid),
                mine,
                &[],
            )
            .err(),
            Some(LifecycleRefusal::BoundaryUnverified)
        );
    }

    /// **The room is copied one file at a time, from bytes judged in the same
    /// read** (Codex on #94): each file is read and judged again at its own
    /// copy, the scan having kept none of its bytes (`RoomEntry` holds a
    /// name and a judgment, so it cannot). The oldest of three, replaced by
    /// the member between the scan and its copy, its name kept and its
    /// bytes another save point's, is refused at the re-judgment, and the
    /// verb stops there: nothing is published, the verb defers, and all
    /// three stay in the room. Two layers refuse it, deliberately: the
    /// re-judgment's name check in `judge_room_file`, and `publish_with`'s
    /// comparison of the judged digest with the scan's. The scan's digest is
    /// the name's, so while the name check stands the comparison cannot fire
    /// and no room reaches it alone; it is a defence kept beside the check.
    /// The name check is pinned on its own by judging the replaced file
    /// directly. Perturbations: drop the name check and the direct judgment
    /// answers the replaced bytes; drop both and the replaced bytes publish
    /// under the name the scan judged.
    #[test]
    fn the_room_is_copied_from_bytes_judged_in_the_same_read() {
        let scratch = crate::scratch::Scratch(std::env::temp_dir().join(format!(
            "weaver-admin-room-one-at-a-time-{}",
            std::process::id()
        )));
        let base = scratch.0.clone();
        let room = base.join("room");
        let dir = base.join("published");
        std::fs::create_dir_all(&room).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let dir_fd = open_directory(&dir).unwrap();
        let me = nix::unistd::getuid().as_raw();
        let mine = Owner {
            uid: me,
            gid: nix::unistd::getgid().as_raw(),
        };
        let mut names = Vec::new();
        for (sequence, wall) in [
            (1u64, 1_000_000_000u64),
            (2, 2_000_000_000),
            (3, 3_000_000_000),
        ] {
            let bytes = save_point("r-1", sequence, 0, wall, b"room");
            let judged = judge(&bytes).unwrap();
            let name = format!("{}{SUFFIX}", judged.digest);
            std::fs::write(room.join(&name), &bytes).unwrap();
            names.push((name, judged.digest));
        }
        // Between the scan and the copies the member replaces the first file
        // with another save point's bytes under the same name.
        // The newest is the one the harness reported.
        let swapped = room.join(&names[0].0);
        let other = save_point("r-1", 9, 0, 9_000_000_000, b"other");
        let report = weaver_types::SavePointReport {
            save_point: names[2].1.clone(),
            name: names[2].0.clone(),
            run: weaver_types::RunId("r-1".into()),
            sequence: 3,
            turn: 0,
            event_run: weaver_types::RunId("r-1".into()),
            position: 7,
        };
        let (lines, deferred) = publish_with(
            open_directory(&room).unwrap().as_fd(),
            me,
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            mine,
            &[(report, Arrival::Demand)],
            &mut PublishHooks {
                after_scan: &mut || std::fs::write(&swapped, &other).unwrap(),
                available: &mut available_bytes,
                cap: PUBLISH_CAP,
            },
        )
        .unwrap();
        // **The oldest failing its judgment stops the verb** (the #94
        // survey's S3): published past, it would leave the oldest to be
        // minted later above the reported one. Perturbation: skip the
        // failed entry again and the two newer publish.
        assert!(lines.is_empty(), "nothing newer is published past it");
        assert!(deferred, "and the verb answers the deferral");
        for (name, _) in &names {
            assert!(room.join(name).exists(), "{name} is left in the room");
        }
        // The name check on its own: the replaced file, judged directly, is
        // no save point under its name, and an untouched one judges sound.
        let room_fd = open_directory(&room).unwrap();
        assert!(
            judge_room_file(room_fd.as_fd(), &names[0].0, me).is_none(),
            "bytes that are not the name's are refused at the re-judgment"
        );
        assert_eq!(
            judge_room_file(room_fd.as_fd(), &names[1].0, me).map(|(_, judged)| judged.digest),
            Some(names[1].1.clone())
        );
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
        let lines = publish(
            open_directory(&room).unwrap().as_fd(),
            me,
            dir_fd.as_fd(),
            owner,
            mine,
            &[],
        )
        .unwrap();
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
        let lines = publish(
            open_directory(&room).unwrap().as_fd(),
            me,
            dir_fd.as_fd(),
            owner,
            mine,
            &[],
        )
        .unwrap();
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
        let lines = publish(
            open_directory(&room).unwrap().as_fd(),
            me,
            dir_fd.as_fd(),
            owner,
            mine,
            &[],
        )
        .unwrap();
        assert!(lines.is_empty(), "no second line for a standing one");
        assert!(target.exists(), "the target is recreated under its line");
        assert!(
            !room.join(&room_name).exists(),
            "the copy went once the target stood"
        );
        assert_eq!(read_manifest(dir_fd.as_fd(), mine).unwrap().len(), 1);
        std::fs::write(room.join(&room_name), &bytes).unwrap();
        std::fs::write(&target, b"damaged").unwrap();
        let lines = publish(
            open_directory(&room).unwrap().as_fd(),
            me,
            dir_fd.as_fd(),
            owner,
            mine,
            &[],
        )
        .unwrap();
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
    /// per `weaver-admin-Spec` sections 4 and 6: a latest whose file is
    /// gone, or whose bytes do not digest to its line, refuses the load and
    /// an older one loads only by name, the ordinal is one past the highest line standing
    /// even when that line's file is gone, a file no line names is not
    /// loadable by name, and a manifest that does not parse refuses
    /// `BoundaryUnverified`. The checks behind "differs" are pinned each on
    /// its own at the end: a line naming a sound file by its published name
    /// and carrying another's digest refuses, and a manifest of ordinals 1
    /// and 5 mints 6. Perturbations: drop the digest comparison in
    /// `open_published` and the line of another's digest selects its file;
    /// mint the ordinal from the count of lines and the gapped manifest
    /// mints 3.
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
        let latest = select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine)
            .unwrap()
            .expect("a latest");
        assert_eq!(latest.line.ordinal, 3);
        assert!(!latest.lineage.named_at_restore);
        // **A latest that cannot be judged refuses, never passed over** (the
        // #94 survey's S9): its mode changed by hand to 0644 is a fault, not
        // a file gone or differing, so the selection refuses rather than
        // restore the second. Perturbation: pass over every fault again and
        // the second is selected.
        {
            use std::os::unix::fs::PermissionsExt;
            let third = dir.join(&lines[2].name);
            std::fs::set_permissions(&third, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert_eq!(
                select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine).err(),
                Some(LifecycleRefusal::BoundaryUnverified)
            );
            std::fs::set_permissions(&third, std::fs::Permissions::from_mode(0o640)).unwrap();
        }
        // **The latest gone or differing refuses, never passed over** (the
        // Planner's ruling of 2026-10-08 on #94): the third's file goes and
        // the load refuses, the next ordinal still four; `restore` naming
        // the second loads it; the third standing again with other sound
        // bytes refuses too, and `restore` naming the first by its digest
        // loads it. Perturbation: pass over to an older line again and the
        // two refusals answer the second and the first.
        let third = dir.join(&lines[2].name);
        std::fs::remove_file(&third).unwrap();
        assert_eq!(
            select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a latest gone refuses"
        );
        assert_eq!(
            next_ordinal(&read_manifest(dir_fd.as_fd(), mine).unwrap()),
            4
        );
        let named = select(
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            Some(&lines[1].name),
            mine,
        )
        .unwrap()
        .expect("the named one");
        assert_eq!(named.line.ordinal, 2);
        std::fs::write(
            &third,
            save_point("r-1", 3, 0, 3_000_000_000, b"other image"),
        )
        .unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&third, std::fs::Permissions::from_mode(0o640)).unwrap();
        }
        assert_eq!(
            select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a latest that differs refuses"
        );
        let named = select(
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            Some(&lines[0].digest),
            mine,
        )
        .unwrap()
        .expect("the named one");
        assert_eq!(named.line.ordinal, 1);
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
            select(
                dir_fd.as_fd(),
                (mine.uid, mine.gid),
                Some(&stray_name),
                mine
            ),
            Err(LifecycleRefusal::ConfigInvalid { .. })
        ));
        // Named at a restore by its bare digest, the unlisted file is found
        // under its published name (Codex on #94, round 2), enters with the
        // next ordinal and is then loadable by name and by digest; a digest
        // no file carries refuses. Perturbation: open the digest as a name
        // and the first call refuses.
        let stray_digest = judge(&stray).unwrap().digest;
        assert!(
            name_at_restore(dir_fd.as_fd(), (mine.uid, mine.gid), &"0".repeat(64), mine).is_err()
        );
        let named =
            name_at_restore(dir_fd.as_fd(), (mine.uid, mine.gid), &stray_digest, mine).unwrap();
        assert_eq!(named.name, stray_name);
        assert_eq!((named.ordinal, named.arrived), (4, Arrival::Restore));
        let by_name = select(
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            Some(&stray_name),
            mine,
        )
        .unwrap()
        .unwrap();
        assert_eq!(by_name.line.ordinal, 4);
        let by_digest = select(
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            Some(&named.digest),
            mine,
        )
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
            name_at_restore(dir_fd.as_fd(), (mine.uid, mine.gid), &stray_name, mine),
            Err(LifecycleRefusal::ConfigInvalid { .. })
        ));
        std::fs::rename(dir.join(&stray_name), dir.join("aside")).unwrap();
        assert!(matches!(
            name_at_restore(dir_fd.as_fd(), (mine.uid, mine.gid), &stray_digest, mine),
            Err(LifecycleRefusal::BoundaryUnverified)
        ));
        std::fs::rename(dir.join("aside"), dir.join(&stray_name)).unwrap();
        std::fs::write(dir.join(&stray_name), &stray).unwrap();
        assert_eq!(
            name_at_restore(dir_fd.as_fd(), (mine.uid, mine.gid), &stray_name, mine)
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
            name_at_restore(
                dir_fd.as_fd(),
                (mine.uid, mine.gid),
                "20200101T000000Z-aa.save-point",
                mine
            )
            .is_err()
        );
        // Back under its name, the latest stands again for what follows.
        std::fs::rename(
            dir.join("20200101T000000Z-aa.save-point"),
            dir.join(&stray_name),
        )
        .unwrap();
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
            select(dir_fd.as_fd(), (mine.uid, mine.gid), None, other),
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
        assert!(
            select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine)
                .unwrap()
                .is_some()
        );
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
            select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine),
            Err(LifecycleRefusal::BoundaryUnverified)
        ));
        std::fs::remove_file(dir.join(MANIFEST)).unwrap();
        assert!(matches!(
            select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine),
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
            select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine)
                .unwrap()
                .is_none(),
            "nothing at all is the first load"
        );
        // **A line whose digest is not its file's refuses** (the #99 area 1
        // review), apart from the name check: the line names a sound file
        // by the published name its bytes compute, so `open_judged` passes
        // it, and carries another save point's digest.
        let sound = save_point("r-1", 1, 0, 1_000_000_000, b"sound");
        let sound_judged = judge(&sound).unwrap();
        let sound_name = published_name(&sound_judged);
        std::fs::write(dir.join(&sound_name), &sound).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                dir.join(&sound_name),
                std::fs::Permissions::from_mode(0o640),
            )
            .unwrap();
        }
        let elsewhere = judge(&save_point("r-1", 2, 0, 2_000_000_000, b"elsewhere"))
            .unwrap()
            .digest;
        append_line(
            dir_fd.as_fd(),
            mine,
            &ManifestLine {
                ordinal: 1,
                digest: elsewhere,
                name: sound_name.clone(),
                stamp: sound_judged.stamp.clone(),
                position: None,
                arrived: Arrival::Leave,
            },
        )
        .unwrap();
        assert_eq!(
            select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine).err(),
            Some(LifecycleRefusal::BoundaryUnverified),
            "a file of its name and not its line's digest refuses"
        );
        // **The ordinal is one past the highest line, not the count of
        // lines**: lines 1 and 5 stand, and the next publication mints 6.
        std::fs::remove_file(dir.join(MANIFEST)).unwrap();
        for ordinal in [1, 5] {
            append_line(
                dir_fd.as_fd(),
                mine,
                &ManifestLine {
                    ordinal,
                    ..lines[0].clone()
                },
            )
            .unwrap();
        }
        assert_eq!(
            next_ordinal(&read_manifest(dir_fd.as_fd(), mine).unwrap()),
            6
        );
        let room = dir.join("room");
        std::fs::create_dir(&room).unwrap();
        let next = save_point("r-1", 6, 0, 6_000_000_000, b"next");
        std::fs::write(
            room.join(format!("{}{SUFFIX}", judge(&next).unwrap().digest)),
            &next,
        )
        .unwrap();
        let appended = publish(
            open_directory(&room).unwrap().as_fd(),
            me,
            dir_fd.as_fd(),
            (mine.uid, mine.gid),
            mine,
            &[],
        )
        .unwrap();
        assert_eq!(
            appended.iter().map(|line| line.ordinal).collect::<Vec<_>>(),
            [6],
            "minted one past the highest line"
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
        let published = publish(
            open_directory(&room).unwrap().as_fd(),
            me,
            dir_fd.as_fd(),
            owner,
            mine,
            &[],
        );
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
        let lines = publish(
            open_directory(&room).unwrap().as_fd(),
            me,
            dir_fd.as_fd(),
            owner,
            mine,
            &[],
        )
        .unwrap();
        assert_eq!(lines.len(), 1);
        assert!(
            aside.join(&lines[0].name).exists() && aside.join(MANIFEST).exists(),
            "the target and the line land in the directory judged"
        );
        assert!(
            std::fs::read_dir(&dir).unwrap().next().is_none(),
            "the directory at the path gets nothing"
        );
        let latest = select(dir_fd.as_fd(), (mine.uid, mine.gid), None, mine)
            .unwrap()
            .expect("a latest");
        assert_eq!(latest.line.digest, judged.digest);
        // A descriptor opened at the path now reaches the replacement, which
        // holds no manifest and no file: the first load's answer.
        let replacement = open_directory(&dir).unwrap();
        assert!(
            select(replacement.as_fd(), (mine.uid, mine.gid), None, mine)
                .unwrap()
                .is_none()
        );
    }
}

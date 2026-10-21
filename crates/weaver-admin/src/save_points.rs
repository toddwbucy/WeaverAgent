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

// The replacement artifact primitive is isolated until the coupled serving
// cutover. The existing admin verbs above that boundary retain their protocol.
#[allow(dead_code)]
pub(crate) mod serving;

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
/// section 6. A sync a signal interrupted is retried (#140, Codex on #148).
fn sync_directory(directory: BorrowedFd<'_>) -> std::io::Result<()> {
    loop {
        match nix::unistd::fsync(directory) {
            Err(nix::errno::Errno::EINTR) => {}
            done => return done.map_err(std::io::Error::from),
        }
    }
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
    let mut seen = 0;
    list_kept(
        directory,
        &|name| name != "." && name != "..",
        usize::MAX,
        &mut seen,
    )
}

/// **The names `keep` admits, read through the directory's descriptor, at
/// most `bound` of them kept**: `kept` counts every admitted name read, and
/// the listing stops at the first past the bound, so `kept` is then
/// `bound + 1`. A name `keep` refuses is read and passed over, never kept.
/// **A listing that fails is never an empty directory** (the custody audit's
/// G9): a failed `dup`, open or read of an entry answers the error, so a
/// refusal that rests on the directory's contents fails closed.
fn list_kept(
    directory: BorrowedFd<'_>,
    keep: &dyn Fn(&str) -> bool,
    bound: usize,
    kept: &mut usize,
) -> nix::Result<Vec<String>> {
    let duplicate = nix::unistd::dup(directory)?;
    let mut dir = nix::dir::Dir::from_fd(duplicate)?;
    let mut names = Vec::new();
    *kept = 0;
    for entry in dir.iter() {
        let entry = entry?;
        if let Ok(name) = entry.file_name().to_str()
            && keep(name)
        {
            *kept += 1;
            if *kept > bound {
                break;
            }
            names.push(name.to_string());
        }
    }
    Ok(names)
}

/// **The scan's bound on the room's finished names** (S16, the operator's
/// ruling of 2026-10-14 on #140): provisional, as the trace queue's is, and
/// measured, every scan recording its count in `admin.log`.
pub const ROOM_SCAN_BOUND: usize = 4096;

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
    let created = !entry_stands(directory, MANIFEST);
    let mut file = if created {
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
        let laid = chown(file.as_fd(), owner.uid, owner.gid)
            .map_err(|e| refuse(format!("does not take its owner and group: {e}")))
            .and_then(|()| {
                nix::sys::stat::fchmod(
                    file.as_fd(),
                    nix::sys::stat::Mode::from_bits_truncate(0o644),
                )
                .map_err(|e| refuse(format!("does not take mode 0644: {e}")))
            })
            .and_then(|()| {
                sync_directory(directory)
                    .map_err(|e| refuse(format!("'s directory does not sync: {e}")))
            });
        if let Err(refusal) = laid {
            unmake(directory);
            return Err(refusal);
        }
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
    let appended = append_to(&mut file, owner, line, &refuse);
    // **A manifest this call made and could not finish is unmade** (#99, W1;
    // #107): left standing empty, or under the invoker's group, it would be
    // a manifest every later publication and load refuses. The name was
    // made exclusively in the directory this crate alone writes, so the
    // unlink takes only what this call made.
    if appended.is_err() && created {
        unmake(directory);
    }
    appended
}

/// **The manifest this call created, unlinked and the directory synced**,
/// per `append_line_with`'s undo: best effort, a failure leaving the entry
/// for the judgment to refuse as before.
fn unmake(directory: BorrowedFd<'_>) {
    let _ = nix::unistd::unlinkat(directory, MANIFEST, nix::unistd::UnlinkatFlags::NoRemoveDir);
    let _ = sync_directory(directory);
}

/// The append itself, on the opened manifest: judged, its torn tail
/// truncated, the line written whole or rolled back.
fn append_to(
    file: &mut std::fs::File,
    owner: Owner,
    line: &ManifestLine,
    refuse: &dyn Fn(String) -> LifecycleRefusal,
) -> Result<(), LifecycleRefusal> {
    judge_manifest(file, owner)?;
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
///
/// **At most `bound` finished names** (S16, the operator's ruling of
/// 2026-10-14 on #140; Spec section 6): the listing keeps finished names
/// alone and stops past the bound, refusing `BoundaryUnverified` naming the
/// room before any is scanned, so the entries the scan opens and reads are
/// bounded whatever the room holds. Every other name is still read by the
/// listing, as a directory entry, and is never opened or kept. `finished` is set
/// to the count seen once the listing reads, `bound + 1` where it stopped,
/// for the caller to record; a listing that fails sets nothing.
fn read_room(
    room: BorrowedFd<'_>,
    member_uid: u32,
    bound: usize,
    finished: &mut Option<usize>,
) -> Result<Vec<RoomEntry>, LifecycleRefusal> {
    let mut count = 0;
    // **Listed through the room's own descriptor** (the custody audit's
    // G12), the descriptor `open_room` opened without following a link, so
    // the listing and every open and removal below resolve the one
    // directory judged and never the path again.
    let names = list_kept(room, &is_finished_name, bound, &mut count).map_err(|e| {
        diag!("weaver-admin: the room does not list ({e}); nothing is published");
        LifecycleRefusal::BoundaryUnverified
    })?;
    *finished = Some(count);
    if count > bound {
        diag!(
            "weaver-admin: the room holds more than {bound} finished save points ({count} read before the listing stopped), past the scan's bound; nothing is published until it is cleared"
        );
        return Err(LifecycleRefusal::BoundaryUnverified);
    }
    let mut found = Vec::new();
    for name in names {
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
// #118: a test-only hook, kept beside the production code it observes.
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
    // #118: a test-only hook, kept beside the production code it observes.
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
// #118: a test-only hook, kept beside the production code it observes.
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
/// Production calls [`publish_counting`], which also answers the room's count.
// #118: a test-only hook, kept beside the production code it observes.
#[cfg(test)]
pub fn publish_noting_deferral(
    room: BorrowedFd<'_>,
    member_uid: u32,
    directory: BorrowedFd<'_>,
    file_owner: (u32, u32),
    owner: Owner,
    reports: &[(SavePointReport, Arrival)],
) -> Result<(Vec<ManifestLine>, bool), LifecycleRefusal> {
    publish_counting(
        room, member_uid, directory, file_owner, owner, reports, &mut None,
    )
}

/// `publish_noting_deferral`, setting `finished` to the room's finished-name
/// count the scan saw, `ROOM_SCAN_BOUND + 1` where it refused past the bound,
/// and nothing where the room did not list, for the caller to record (S16).
pub fn publish_counting(
    room: BorrowedFd<'_>,
    member_uid: u32,
    directory: BorrowedFd<'_>,
    file_owner: (u32, u32),
    owner: Owner,
    reports: &[(SavePointReport, Arrival)],
    finished: &mut Option<usize>,
) -> Result<(Vec<ManifestLine>, bool), LifecycleRefusal> {
    let mut hooks = PublishHooks {
        after_scan: &mut || {},
        available: &mut available_bytes,
        cap: PUBLISH_CAP,
        room_bound: ROOM_SCAN_BOUND,
        finished: None,
        before_read_back: &mut |_, _| {},
        removal_sync: &mut sync_directory,
        removal: None,
    };
    let published = publish_with(
        room, member_uid, directory, file_owner, owner, reports, &mut hooks,
    );
    *finished = hooks.finished;
    published
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
/// and the first copy, the free-space look, the per-verb cap and the scan's
/// bound, a hook run on each published name between its directory's sync
/// and its read-back, and the sync after a copy that did not read back is
/// removed (#157); and what the scan saw, the room's finished-name count,
/// and what that removal left.
struct PublishHooks<'a> {
    after_scan: &'a mut dyn FnMut(),
    before_read_back: &'a mut dyn FnMut(BorrowedFd<'_>, &str),
    /// The directory's sync after a copy that did not read back is
    /// removed, a parameter so a test can fail it.
    removal_sync: &'a mut dyn FnMut(BorrowedFd<'_>) -> std::io::Result<()>,
    /// What that removal left, for a test to read.
    removal: Option<Removal>,
    available: &'a mut dyn FnMut(BorrowedFd<'_>) -> std::io::Result<u64>,
    cap: usize,
    room_bound: usize,
    finished: Option<usize>,
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
    let mut entries = read_room(room, member_uid, hooks.room_bound, &mut hooks.finished)?;
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
        // Answers whether the entry under the published name was adopted
        // rather than renamed into place by this verb.
        let written = (|| -> std::io::Result<bool> {
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
                Ok(()) => sync_directory(directory).map(|()| false),
                // **An entry already under the published name is adopted
                // where it is this save point**: a publication interrupted
                // between the rename and the manifest line left it, the
                // retry at the next verb appends the line; judged through
                // the directory as a load judges one, so an entry of other
                // bytes refuses the rename and is left in place, named.
                Err(nix::errno::Errno::EEXIST) => {
                    remove_temporary();
                    match adopt_durably(directory, &name, file_owner, &mut sync_directory) {
                        Ok(Some((_, standing))) if standing.digest == entry.digest => Ok(true),
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
        let adopted = match written {
            Ok(adopted) => adopted,
            Err(e) => {
                diag!(
                    "weaver-admin: the room's {} did not publish ({e}); it and every later save point are left for the next verb",
                    entry.name
                );
                remove_temporary();
                deferred = true;
                break;
            }
        };
        // The copy's bytes are done with, so the read-back below never holds
        // two whole save points at once.
        drop(bytes);
        // **The published copy is read back before its line is written**
        // (#157), per Spec section 6: opened again through the directory's
        // descriptor and judged as a load judges it, which refuses a file
        // whose bytes do not compute its name, and the name carries the
        // room's digest; so the line, and a `SavePointTaken` or an
        // `Unloaded` built on it, names bytes the kernel serves at that
        // name. It proves nothing of the device, a read right after a write
        // being served from the page cache; durability rests on the syncs
        // above. An adopted entry was just judged the same way by
        // `adopt_durably`, which is its read-back. A copy that does not read
        // back is unlinked, the directory synced, and the verb stops as a
        // deferral, the room's copy kept.
        if !adopted {
            (hooks.before_read_back)(directory, &name);
            let read_back = match open_judged(directory, &name, file_owner) {
                Ok(Some(_)) => Ok(()),
                Ok(None) => Err("it is gone".to_string()),
                Err(fault) => Err(fault.why),
            };
            if let Err(why) = read_back {
                let removal = remove_published(directory, &name, hooks.removal_sync);
                diag!(
                    "weaver-admin: the published {name} does not read back to its digest ({why}); {}",
                    removal.says(&entry.name)
                );
                hooks.removal = Some(removal);
                deferred = true;
                break;
            }
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

/// **What removing a copy that did not read back left** (#157, Codex on
/// #158), per Spec section 6, each stated apart because each leaves a
/// different state and a different recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Removal {
    /// The name is gone, by this unlink or before it (`ENOENT`), and the
    /// directory synced: the next verb publishes afresh.
    Synced,
    /// The name is gone and the directory did not sync: the next verb
    /// publishes afresh, unless a power loss first brings the name back.
    Unsynced(String),
    /// The unlink failed and the copy stands under the published name.
    Standing(String),
}

impl Removal {
    /// The rest of `admin.log`'s line: what stands, and what the next verb
    /// does.
    fn says(&self, room_name: &str) -> String {
        match self {
            Removal::Synced => format!(
                "it is removed and the directory synced, so the next verb publishes the room's {room_name} afresh; it and every later save point are left for that verb"
            ),
            Removal::Unsynced(e) => format!(
                "it is removed but the directory did not sync ({e}): the next verb publishes the room's {room_name} afresh, unless a power loss first brings the name back, which that verb then refuses as an entry of other bytes until the operator removes it; it and every later save point are left for the next verb"
            ),
            Removal::Standing(e) => format!(
                "it was not removed ({e}) and stands under its published name: every later verb refuses on it until the operator removes it, the room's {room_name} and every later save point waiting until then"
            ),
        }
    }
}

/// Remove a published copy that did not read back, through the directory's
/// descriptor, and sync the directory: a name already gone (`ENOENT`) is
/// removed, and its absence is synced all the same.
fn remove_published(
    directory: BorrowedFd<'_>,
    name: &str,
    sync: &mut dyn FnMut(BorrowedFd<'_>) -> std::io::Result<()>,
) -> Removal {
    match nix::unistd::unlinkat(directory, name, nix::unistd::UnlinkatFlags::NoRemoveDir) {
        Ok(()) | Err(nix::errno::Errno::ENOENT) => match sync(directory) {
            Ok(()) => Removal::Synced,
            Err(e) => Removal::Unsynced(e.to_string()),
        },
        Err(e) => Removal::Standing(e.to_string()),
    }
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
    // **And no access entry beyond its mode** (#99 area 2, H5; #107): an
    // entry granting another principal read hides under the mask.
    match crate::carries_access_entries_fd(std::os::fd::AsFd::as_fd(&file)) {
        Ok(false) => {}
        Ok(true) => {
            return Err(fault(format!(
                "{name} carries an access-control entry beyond its mode"
            )));
        }
        Err(e) => {
            return Err(fault(format!(
                "{name} does not say whether it carries an access-control entry: {e}"
            )));
        }
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
        // **`[restore]` is honoured only while its save point is the latest**
        // (the operator's ruling of 2026-10-08 on #99, N6): the `restore`
        // verb makes the named save point the manifest's latest, and a newer
        // save point published since means the key was left behind; loading
        // the named one then would silently restore older state. The load
        // refuses and names both: remove `[restore]`, or run `restore` again.
        if let Some(latest) = lines.iter().max_by_key(|line| line.ordinal)
            && latest.ordinal != line.ordinal
        {
            diag!(
                "weaver-admin: restore names {} (ordinal {}), and the manifest's latest is {} (ordinal {}); remove [restore] to load the latest, or run restore again to continue from the named one",
                line.name,
                line.ordinal,
                latest.name,
                latest.ordinal
            );
            return Err(refuse_config());
        }
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
/// its line where that line is the latest, and is named again under the next
/// ordinal where a newer line outranks it (the operator's ruling of
/// 2026-10-08 on #99, N6); otherwise the file is judged as a load judges one
/// and a line naming it is appended, marked as arrived by restore, with no
/// position. Either way the named save point is the latest after the verb.
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
            // **The named save point becomes the latest** (N6): a listed one
            // that a newer line outranks is named again under the next
            // ordinal, marked as named at a restore, so the load that follows
            // continues from it and a `[restore]` naming it is honoured.
            Ok(Some(_)) if lines.iter().all(|other| other.ordinal <= line.ordinal) => {
                Ok(line.clone())
            }
            Ok(Some(_)) => {
                let again = ManifestLine {
                    ordinal: next_ordinal(&lines),
                    position: None,
                    arrived: Arrival::Restore,
                    ..line.clone()
                };
                append_line(directory, owner, &again)?;
                Ok(again)
            }
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
// #118: a test-only hook, kept beside the production code it observes.
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
pub(crate) mod tests;

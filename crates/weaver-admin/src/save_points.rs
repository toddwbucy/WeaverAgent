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
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Path, PathBuf};

use sha2::Digest;
use weaver_types::{LifecycleRefusal, SavePointReport};

/// The manifest's name in the operator's declaration directory.
pub const MANIFEST: &str = "save-points.manifest";
/// The marker's name in the agent's config root.
pub const MARKER: &str = "run.marker";
/// The published name's suffix, the room's own.
pub const SUFFIX: &str = ".save-point";

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
    pub wall_ns: u128,
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
    if taken.len() != 3 {
        return Err("the stamp's taken does not carry exactly three members".into());
    }
    let wall_ns = taken
        .get("wall_ns")
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse::<u128>().ok())
        .ok_or("the stamp's taken names no wall clock")?;
    if image.len() as u64 != length {
        return Err(format!(
            "the image is {} bytes and the stamp says {length}",
            image.len()
        ));
    }
    let check: serde_json::Value = serde_json::from_slice(check_line)
        .map_err(|e| format!("the check line does not parse: {e}"))?;
    let claimed = check
        .get("check")
        .and_then(|v| v.as_str())
        .ok_or("the check line names no check")?;
    let mut hasher = sha2::Sha256::new();
    hasher.update(header);
    hasher.update(b"\n");
    hasher.update(image);
    if hex(&hasher.finalize()) != claimed {
        return Err("the check over the bytes does not hold".into());
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

/// **Read the manifest**, per `weaver-admin-Spec` section 4: absent is an
/// empty manifest; one that does not read, or a line that does not parse,
/// refuses `BoundaryUnverified` naming it, since what is loadable can then
/// not be said.
pub fn read_manifest(directory: &Path) -> Result<Vec<ManifestLine>, LifecycleRefusal> {
    let path = directory.join(MANIFEST);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            diag!(
                "weaver-admin: the manifest {} does not read: {e}",
                path.display()
            );
            return Err(LifecycleRefusal::BoundaryUnverified);
        }
    };
    let mut lines = Vec::new();
    for (at, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let Some(parsed) = parse_line(line) else {
            diag!(
                "weaver-admin: the manifest {} does not parse at line {}",
                path.display(),
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
fn append_line(directory: &Path, line: &ManifestLine) -> Result<(), LifecycleRefusal> {
    use std::os::unix::fs::OpenOptionsExt;
    let path = directory.join(MANIFEST);
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o644)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC)
        .open(&path)
        .map_err(|e| {
            diag!(
                "weaver-admin: the manifest {} does not open for appending: {e}",
                path.display()
            );
            LifecycleRefusal::BoundaryUnverified
        })?;
    let rendered = render_line(line);
    file.write_all(rendered.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|e| {
            diag!(
                "weaver-admin: the manifest {} does not take a line: {e}",
                path.display()
            );
            LifecycleRefusal::BoundaryUnverified
        })
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
fn read_room(room: &Path, member_uid: u32) -> Vec<RoomEntry> {
    use std::os::unix::fs::MetadataExt;
    let Ok(dir) = nix::fcntl::open(
        room,
        nix::fcntl::OFlag::O_RDONLY | nix::fcntl::OFlag::O_DIRECTORY | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    ) else {
        return Vec::new();
    };
    let Ok(listing) = std::fs::read_dir(room) else {
        return Vec::new();
    };
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
        let mut bytes = Vec::new();
        if file.read_to_end(&mut bytes).is_err() {
            continue;
        }
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
    found
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
    directory: &Path,
    operator: (u32, u32),
    reports: &[(SavePointReport, Arrival)],
) -> Result<Vec<ManifestLine>, LifecycleRefusal> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut lines = read_manifest(directory)?;
    let mut appended = Vec::new();
    for entry in read_room(room, member_uid) {
        let (position, arrived) = reports
            .iter()
            .find(|(report, _)| report.save_point == entry.judged.digest)
            .map(|(report, arrival)| (Some((report.run.0.clone(), report.position)), *arrival))
            .unwrap_or((None, Arrival::Recovered));
        let name = published_name(&entry.judged);
        if lines.iter().any(|line| line.digest == entry.judged.digest) {
            // Published already, under an earlier verb cut short before the
            // room's copy was removed: the copy goes, the line stands.
            let _ = std::fs::remove_file(room.join(&entry.name));
            continue;
        }
        let temporary = directory.join(format!(".publishing-{}", entry.judged.digest));
        let _ = std::fs::remove_file(&temporary);
        let written = (|| -> std::io::Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(nix::libc::O_CLOEXEC)
                .open(&temporary)?;
            file.write_all(&entry.bytes)?;
            file.sync_all()?;
            nix::unistd::fchown(
                file.as_fd(),
                Some(nix::unistd::Uid::from_raw(operator.0)),
                Some(nix::unistd::Gid::from_raw(operator.1)),
            )
            .map_err(std::io::Error::from)?;
            let metadata = file.metadata()?;
            use std::os::unix::fs::MetadataExt;
            if metadata.uid() != operator.0 || metadata.mode() & 0o777 != 0o600 {
                return Err(std::io::Error::other("the copy is not the operator's 0600"));
            }
            let target = directory.join(&name);
            if target.exists() {
                return Err(std::io::Error::other(
                    "a file stands under the published name",
                ));
            }
            std::fs::rename(&temporary, &target)
        })();
        if let Err(e) = written {
            diag!(
                "weaver-admin: the room's {} did not publish ({e}) and is left in place",
                entry.name
            );
            let _ = std::fs::remove_file(&temporary);
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
        append_line(directory, &line)?;
        lines.push(line.clone());
        appended.push(line);
        // Only now the member's copy goes, so a publication cut short leaves
        // it standing and is retried at the next verb.
        let _ = std::fs::remove_file(room.join(&entry.name));
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

/// Open and judge one published file through the directory, per
/// `weaver-admin-Spec` section 4: a regular file, not a link, the operator's,
/// closed to group and other, whose bytes judge to the line's digest and
/// whose published name is the one its bytes compute.
fn open_published(
    directory: &Path,
    line: &ManifestLine,
    operator: u32,
) -> Result<Option<OwnedFd>, String> {
    use std::os::unix::fs::MetadataExt;
    let dir = nix::fcntl::open(
        directory,
        nix::fcntl::OFlag::O_RDONLY | nix::fcntl::OFlag::O_DIRECTORY | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )
    .map_err(|e| format!("the declaration directory does not open: {e}"))?;
    let fd = match nix::fcntl::openat(
        dir.as_fd(),
        line.name.as_str(),
        nix::fcntl::OFlag::O_RDONLY | nix::fcntl::OFlag::O_NOFOLLOW | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(nix::errno::Errno::ENOENT) => return Ok(None),
        Err(e) => return Err(format!("{} does not open: {e}", line.name)),
    };
    let mut file = std::fs::File::from(fd);
    let metadata = file
        .metadata()
        .map_err(|e| format!("{} does not stat: {e}", line.name))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", line.name));
    }
    if metadata.uid() != operator {
        return Err(format!("{} is not the operator's", line.name));
    }
    if metadata.mode() & 0o077 != 0 {
        return Err(format!(
            "{} grants a permission to group or other",
            line.name
        ));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| format!("{} does not read: {e}", line.name))?;
    let judged =
        judge(&bytes).map_err(|why| format!("{} is not a save point: {why}", line.name))?;
    if judged.digest != line.digest {
        return Err(format!("{} does not digest to its line", line.name));
    }
    if published_name(&judged) != line.name {
        return Err(format!("{} is not the name its bytes compute", line.name));
    }
    // Rewound for the member: the read above consumed the offset, and the
    // member reads from zero.
    nix::unistd::lseek(file.as_fd(), 0, nix::unistd::Whence::SeekSet)
        .map_err(|e| format!("{} does not seek: {e}", line.name))?;
    Ok(Some(OwnedFd::from(file)))
}

/// **Select the save point a load restores**, per `weaver-admin-Spec`
/// section 4: the one `restore` names by its published name or its digest,
/// which must have a manifest line, or the latest, the line of highest
/// ordinal whose file stands and judges to its digest, a line whose file is
/// gone or differs being passed over. `Ok(None)` is no save point: an empty
/// manifest with nothing named.
pub fn select(
    directory: &Path,
    operator: u32,
    restore: Option<&str>,
) -> Result<Option<Selected>, LifecycleRefusal> {
    let lines = read_manifest(directory)?;
    // **A manifest absent beside published files refuses**, per Spec
    // section 4: what is loadable cannot be said, and the files are not
    // loadable without it; no manifest and no file is the first load.
    if lines.is_empty()
        && !directory.join(MANIFEST).exists()
        && std::fs::read_dir(directory)
            .map(|entries| {
                entries.flatten().any(|entry| {
                    entry
                        .file_name()
                        .to_str()
                        .is_some_and(|name| name.ends_with(SUFFIX))
                })
            })
            .unwrap_or(false)
    {
        diag!(
            "weaver-admin: {} holds save points and no {MANIFEST}; a file the manifest does not name is not loadable, and the restore verb is what names one",
            directory.display()
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
                    "weaver-admin: restore names {named}, whose file is gone from the declaration directory"
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
    directory: &Path,
    operator: u32,
    named: &str,
) -> Result<ManifestLine, LifecycleRefusal> {
    let lines = read_manifest(directory)?;
    if let Some(line) = lines
        .iter()
        .rev()
        .find(|line| line.name == named || line.digest == named)
    {
        return Ok(line.clone());
    }
    let refuse_config = || LifecycleRefusal::ConfigInvalid {
        field: Some(weaver_types::FieldName("restore".into())),
    };
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
    // The file is judged through the directory as a load judges one; the
    // digest and the stamp are its own, and the name must be the one its
    // bytes compute, so a renamed file does not enter.
    let mut probe = candidate.clone();
    let judged = {
        use std::os::unix::fs::MetadataExt;
        let dir = nix::fcntl::open(
            directory,
            nix::fcntl::OFlag::O_RDONLY
                | nix::fcntl::OFlag::O_DIRECTORY
                | nix::fcntl::OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::empty(),
        )
        .map_err(|_| LifecycleRefusal::BoundaryUnverified)?;
        let fd = nix::fcntl::openat(
            dir.as_fd(),
            named,
            nix::fcntl::OFlag::O_RDONLY
                | nix::fcntl::OFlag::O_NOFOLLOW
                | nix::fcntl::OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::empty(),
        )
        .map_err(|e| {
            diag!("weaver-admin: restore names {named}, which does not open: {e}");
            refuse_config()
        })?;
        let mut file = std::fs::File::from(fd);
        let metadata = file.metadata().map_err(|_| refuse_config())?;
        if !metadata.is_file() || metadata.uid() != operator || metadata.mode() & 0o077 != 0 {
            diag!(
                "weaver-admin: restore names {named}, which is not the operator's regular file closed to group and other"
            );
            return Err(LifecycleRefusal::BoundaryUnverified);
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|_| refuse_config())?;
        judge(&bytes).map_err(|why| {
            diag!("weaver-admin: restore names {named}, which is not a save point: {why}");
            refuse_config()
        })?
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
    append_line(directory, &probe)?;
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
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        };
    };
    let (state, run) = match marker {
        Marker::Open { run } => ("open", run),
        Marker::Closed { run } => ("closed", run),
        Marker::Forced { run } => ("forced", run),
    };
    let temporary = root.join(".run.marker.new");
    std::fs::write(
        &temporary,
        format!("{}\n", serde_json::json!({"run": run, "state": state})),
    )?;
    std::fs::rename(&temporary, &path)
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
    use super::*;

    /// A save point in the member's format, built here as the member builds
    /// one, so the judgment below reads what the member writes.
    pub(crate) fn save_point(
        run: &str,
        sequence: u64,
        turn: u64,
        wall_ns: u128,
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
        let me = nix::unistd::getuid().as_raw();
        let mut lines = Vec::new();
        for (sequence, wall) in [
            (1u64, 1_000_000_000u128),
            (2, 2_000_000_000),
            (3, 3_000_000_000),
        ] {
            let bytes = save_point("r-1", sequence, 0, wall, b"img");
            let judged = judge(&bytes).unwrap();
            let name = published_name(&judged);
            std::fs::write(dir.join(&name), &bytes).unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir.join(&name), std::fs::Permissions::from_mode(0o600))
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
            append_line(&dir, &line).unwrap();
            lines.push(line);
        }
        assert_eq!(read_manifest(&dir).unwrap(), lines, "the lines round-trip");
        let latest = select(&dir, me, None).unwrap().expect("a latest");
        assert_eq!(latest.line.ordinal, 3);
        assert!(!latest.lineage.named_at_restore);
        // The third's file goes: the second is latest, named at a restore,
        // and the next ordinal is still four.
        std::fs::remove_file(dir.join(&lines[2].name)).unwrap();
        let latest = select(&dir, me, None).unwrap().expect("a latest");
        assert_eq!(latest.line.ordinal, 2);
        assert!(latest.lineage.named_at_restore);
        assert_eq!(next_ordinal(&read_manifest(&dir).unwrap()), 4);
        // The second's file holds other sound bytes, a save point whose digest
        // is not its line's: not the latest, the first is.
        std::fs::write(
            dir.join(&lines[1].name),
            save_point("r-1", 2, 0, 2_000_000_000, b"other image"),
        )
        .unwrap();
        let latest = select(&dir, me, None).unwrap().expect("a latest");
        assert_eq!(latest.line.ordinal, 1);
        // A file no line names is not loadable by name.
        let stray = save_point("r-9", 9, 0, 9_000_000_000, b"stray");
        let stray_name = published_name(&judge(&stray).unwrap());
        std::fs::write(dir.join(&stray_name), &stray).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                dir.join(&stray_name),
                std::fs::Permissions::from_mode(0o600),
            )
            .unwrap();
        }
        assert!(matches!(
            select(&dir, me, Some(&stray_name)),
            Err(LifecycleRefusal::ConfigInvalid { .. })
        ));
        // Named at a restore, it enters with the next ordinal and is then
        // loadable by name and by digest.
        let named = name_at_restore(&dir, me, &stray_name).unwrap();
        assert_eq!((named.ordinal, named.arrived), (4, Arrival::Restore));
        let by_name = select(&dir, me, Some(&stray_name)).unwrap().unwrap();
        assert_eq!(by_name.line.ordinal, 4);
        let by_digest = select(&dir, me, Some(&named.digest)).unwrap().unwrap();
        assert!(by_digest.lineage.named_at_restore);
        // A renamed file does not enter.
        std::fs::rename(
            dir.join(&stray_name),
            dir.join("20200101T000000Z-aa.save-point"),
        )
        .unwrap();
        assert!(name_at_restore(&dir, me, "20200101T000000Z-aa.save-point").is_err());
        // A manifest that does not parse refuses, and so does one absent
        // beside published files; absent beside none is the first load.
        std::fs::write(dir.join(MANIFEST), "not json\n").unwrap();
        assert!(matches!(
            select(&dir, me, None),
            Err(LifecycleRefusal::BoundaryUnverified)
        ));
        std::fs::remove_file(dir.join(MANIFEST)).unwrap();
        assert!(matches!(
            select(&dir, me, None),
            Err(LifecycleRefusal::BoundaryUnverified)
        ));
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            std::fs::remove_file(entry.path()).unwrap();
        }
        assert!(
            select(&dir, me, None).unwrap().is_none(),
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
}

//! The save point and the room it is written in, per `weaver-state-Spec`
//! sections 2 and 3 on the operator's rulings of 2026-10-02 on #58: the whole
//! database serialized, stamped with the trace position it covers and the
//! schema it was taken under, checked over its own bytes, written as a new
//! file in the member's room under a name given only once the write is
//! whole, and never overwritten.
//!
//! **The layout, the name, the check and the digest are this act's
//! elections**, under the Spec's section 6. A save point is three parts in
//! one file: a stamp line carrying exactly the seven members below, its
//! `taken` nonce exactly the process, the process's own count and the wall
//! clock, a check line, and the engine's image.
//!
//! ```text
//! {"weaver-save-point":1,"run":"r-1","sequence":41,"turn":2,"schema":"<hex>","image":<n>,"taken":{"pid":<p>,"ordinal":<k>,"wall_ns":"<t>"}}
//! {"check":"<hex>"}
//! <n bytes of image>
//! ```
//!
//! The check is sha256 over the stamp line, its newline, and the image, so a
//! stamp altered, an image flipped or a file torn short all read as corrupt,
//! and the check line has one spelling, which `parse` requires byte for byte
//! so that a parsed save point's bytes are the file's and its digest the
//! file's digest,
//! and the check line itself is covered by the digest: **the digest is sha256
//! over the whole file** and is the save point's identity on the trace and
//! in the territory's `save-points/`. **The name is the digest**, so two save points
//! with different bytes can never share one, and the stamp's `taken` member,
//! the writing process, a counter that process never repeats, and the wall
//! clock, makes two save points of the same holdings at the same position
//! different bytes whatever the clock does, so two `snapshot` asks on
//! unchanged holdings give two files by construction, which settles the
//! collision the Planner carried on #1 from #58's round 17. **A whole write earns its finished name by a link**: the
//! bytes go to a part name created exclusively, are synced, and are linked
//! under the finished name, a link refusing an existing entry where a rename
//! would replace it, so no path through this module overwrites a file.
//!
//! **What the write guarantees**: no finished name is answered that the room
//! has not durably held, the part having been synced before the link and
//! the room after it. A room sync that fails removes the finished name and
//! syncs the room again, and where that removal or its sync fails the name
//! is said on standard error, never silent, the ask still unanswered. A
//! crash between the link and its sync can leave a name the answer never
//! gave; its bytes are whole, the part having been synced first, so the
//! next load's judgment treats it as a save point like any other.

use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};

use sha2::Digest;

/// The trace position a save point covers, per `weaver-state-Spec` section
/// 3: the run and sequence of the last distillate landed in it, and the last
/// turn that run's holdings carry, zero where the run holds no turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamp {
    pub run: String,
    pub sequence: u64,
    pub turn: u64,
}

/// Why a save point does not read, each a reason the member says on its
/// standard error and never a shape it answers across the seam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SavePointFault {
    /// The bytes are not a save point: no stamp line, no check line, a
    /// version this build does not read, or an image shorter than the
    /// stamp says. A torn write reads here.
    Malformed(String),
    /// The check over the bytes does not hold: damaged since it was written.
    CheckFailed,
    /// The file could not be opened, read or written.
    Io(String),
    /// A name that is not a plain entry of the room.
    NotAPlainName,
    /// The bytes under the name are a sound save point whose own name,
    /// its digest, is another: an alias, which is not this save point.
    NameDisagrees,
}

impl std::fmt::Display for SavePointFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SavePointFault::Malformed(why) => write!(f, "malformed save point: {why}"),
            SavePointFault::CheckFailed => write!(f, "save point fails its check"),
            SavePointFault::Io(why) => write!(f, "save point io: {why}"),
            SavePointFault::NotAPlainName => write!(f, "save point name is not a plain entry"),
            SavePointFault::NameDisagrees => {
                write!(f, "save point name is not the digest's, an alias")
            }
        }
    }
}

/// One save point, whole: its stamp, the schema digest it was taken under,
/// and the engine's image. Built from holdings or parsed from bytes, and
/// rendered to the same bytes either way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavePoint {
    pub stamp: Stamp,
    /// The digest of the schema text the image stands under, sha256 hex.
    pub schema: String,
    pub image: Vec<u8>,
    header: String,
}

impl SavePoint {
    /// Stamp an image taken now. The `taken` nonce is the process, its own
    /// count of save points taken, which never repeats within a process, and
    /// the wall clock, so no two save points this process takes share bytes
    /// even where the clock stands still or moves back.
    pub fn take(stamp: Stamp, schema_text: &str, image: Vec<u8>) -> SavePoint {
        static TAKEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let ordinal = TAKEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let wall_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let schema = schema_digest(schema_text);
        // The stamp line's members, rendered once here and read back by
        // `parse`, which is the one other place their spelling lives.
        let header = serde_json::json!({
            "weaver-save-point": 1,
            "run": stamp.run,
            "sequence": stamp.sequence,
            "turn": stamp.turn,
            "schema": schema,
            "image": image.len(),
            "taken": {"pid": std::process::id(), "ordinal": ordinal, "wall_ns": wall_ns.to_string()},
        })
        .to_string();
        SavePoint {
            stamp,
            schema,
            image,
            header,
        }
    }

    /// The check over the stamp line and the image.
    fn check(&self) -> String {
        let mut hasher = sha2::Sha256::new();
        hasher.update(self.header.as_bytes());
        hasher.update(b"\n");
        hasher.update(&self.image);
        hex(&hasher.finalize())
    }

    /// The check line in its one spelling.
    fn check_line(&self) -> String {
        serde_json::json!({"check": self.check()}).to_string()
    }

    /// The file's bytes: stamp line, check line, image.
    pub fn bytes(&self) -> Vec<u8> {
        let check = self.check_line();
        let mut out = Vec::with_capacity(self.header.len() + check.len() + self.image.len() + 2);
        out.extend_from_slice(self.header.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(check.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(&self.image);
        out
    }

    /// The save point's digest, sha256 hex over the whole file, which is its
    /// name in the room and its identity on the trace.
    pub fn digest(&self) -> String {
        hex(&sha2::Sha256::digest(self.bytes()))
    }

    /// The name the room holds it under.
    pub fn name(&self) -> String {
        format!("{}.save-point", self.digest())
    }

    /// Read a save point back from its bytes, refusing one that is torn,
    /// malformed or fails its check. The stamp is read from the header the
    /// check covers, so a stamp that was altered reads as corrupt and never
    /// as a position.
    pub fn parse(bytes: &[u8]) -> Result<SavePoint, SavePointFault> {
        let malformed = |why: &str| SavePointFault::Malformed(why.to_string());
        let first = bytes
            .iter()
            .position(|&b| b == b'\n')
            .ok_or_else(|| malformed("no stamp line"))?;
        let header_line = std::str::from_utf8(&bytes[..first])
            .map_err(|_| malformed("stamp line is not UTF-8"))?
            .to_string();
        let rest = &bytes[first + 1..];
        let second = rest
            .iter()
            .position(|&b| b == b'\n')
            .ok_or_else(|| malformed("no check line"))?;
        let check_line = &rest[..second];
        let image = &rest[second + 1..];
        let header: serde_json::Value = serde_json::from_str(&header_line)
            .map_err(|e| malformed(&format!("stamp line: {e}")))?;
        // **The stamp line carries exactly the format's seven members and
        // `taken` exactly its three, each of its type**: anything absent,
        // extra or of another type is not this format, so the digest and
        // the name are computable only for a file that follows it, and a
        // stamp without its nonce cannot make two save points of one
        // holding share bytes.
        let members = header
            .as_object()
            .ok_or_else(|| malformed("stamp line is not an object"))?;
        const STAMP: [&str; 7] = [
            "weaver-save-point",
            "run",
            "sequence",
            "turn",
            "schema",
            "image",
            "taken",
        ];
        if members.len() != STAMP.len() || STAMP.iter().any(|name| !members.contains_key(*name)) {
            return Err(malformed(
                "stamp line does not carry exactly the format's members",
            ));
        }
        if header.get("weaver-save-point").and_then(|v| v.as_u64()) != Some(1) {
            return Err(malformed("version this build does not read"));
        }
        let taken = header
            .get("taken")
            .and_then(|t| t.as_object())
            .ok_or_else(|| malformed("taken is not an object"))?;
        let nonce_sound = taken.len() == 3
            && taken.get("pid").is_some_and(|v| v.as_u64().is_some())
            && taken.get("ordinal").is_some_and(|v| v.as_u64().is_some())
            && taken.get("wall_ns").is_some_and(|v| {
                // Digits alone, and digits that fit an unsigned 64-bit count
                // of nanoseconds, what a clock can be: `u64`'s own parse
                // admits a sign, so the digit rule stands beside it.
                v.as_str().is_some_and(|s| {
                    !s.is_empty()
                        && s.bytes().all(|b| b.is_ascii_digit())
                        && s.parse::<u64>().is_ok()
                })
            });
        if !nonce_sound {
            return Err(malformed(
                "taken does not carry exactly pid, ordinal and wall_ns",
            ));
        }
        let member = |name: &str| {
            header
                .get(name)
                .ok_or_else(|| malformed(&format!("no {name}")))
        };
        let text = |name: &str| -> Result<String, SavePointFault> {
            member(name)?
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| malformed(&format!("{name} is not a string")))
        };
        let number = |name: &str| -> Result<u64, SavePointFault> {
            member(name)?
                .as_u64()
                .ok_or_else(|| malformed(&format!("{name} is not a number")))
        };
        if number("image")? != image.len() as u64 {
            return Err(malformed("image length disagrees with the stamp"));
        }
        let parsed = SavePoint {
            stamp: Stamp {
                run: text("run")?,
                sequence: number("sequence")?,
                turn: number("turn")?,
            },
            schema: text("schema")?,
            image: image.to_vec(),
            header: header_line,
        };
        // The check line is required in its one spelling, the one `bytes`
        // renders, so the bytes a parsed save point renders are the bytes it
        // was read from and the digest is the file's: a check line spelled
        // any other way, whatever check it names, is not this format.
        if check_line != parsed.check_line().as_bytes() {
            return Err(SavePointFault::CheckFailed);
        }
        Ok(parsed)
    }
}

/// The digest of a schema's text, sha256 hex, as the stamp names it.
pub fn schema_digest(schema_text: &str) -> String {
    hex(&sha2::Sha256::digest(schema_text.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// The member's room, held open as a directory descriptor for the member's
/// life, per `weaver-state-Spec` section 2: every save point is written and
/// read relative to it and never by a path, and the `grants` ask reads its
/// owner, group and mode through it.
pub struct Room {
    dir: OwnedFd,
}

impl Room {
    /// Open the room. The path is the territory the vector names, used once
    /// here; everything after goes through the descriptor.
    pub fn open(path: &std::path::Path) -> Result<Room, SavePointFault> {
        let dir = nix::fcntl::open(
            path,
            nix::fcntl::OFlag::O_RDONLY
                | nix::fcntl::OFlag::O_DIRECTORY
                | nix::fcntl::OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::empty(),
        )
        .map_err(|e| SavePointFault::Io(format!("open room: {e}")))?;
        let room = Room { dir };
        room.clear_parts();
        Ok(room)
    }

    /// The boundary as the room states it, per the contract's `grants` ask:
    /// the owner, group and mode of the directory, read through the
    /// descriptor, spelled so two readings compare and no more.
    pub fn surface(&self) -> Result<Vec<String>, SavePointFault> {
        let stat = nix::sys::stat::fstat(self.dir.as_fd())
            .map_err(|e| SavePointFault::Io(format!("fstat room: {e}")))?;
        Ok(vec![
            format!("owner {}:{}", stat.st_uid, stat.st_gid),
            format!("mode {:04o}", stat.st_mode & 0o7777),
        ])
    }

    /// Write a save point as a new file and answer its finished name: the
    /// part written and finished in one call, for a writer that needs no
    /// acknowledgement, the offline builder's case.
    pub fn write(&self, save_point: &SavePoint) -> Result<String, SavePointFault> {
        let name = self.write_part(save_point)?;
        self.finish(&name)?;
        Ok(name)
    }

    /// **Write a save point as a part and answer the finished name it will
    /// take**, per `weaver-state-Spec` section 3 on the operator's ruling of
    /// 2026-10-06 on #1 (A3.0 item 4): the bytes go to a part name created
    /// exclusively, dotted so publication never reads it, and are synced;
    /// the finished name is given by [`Room::finish`] on the harness's
    /// acknowledgement alone. Any part standing before this write is
    /// removed first, so the room holds at most one.
    pub fn write_part(&self, save_point: &SavePoint) -> Result<String, SavePointFault> {
        use nix::fcntl::OFlag;
        self.clear_parts();
        let name = save_point.name();
        let part = format!(".part-{name}");
        let io = |what: &str, e: nix::errno::Errno| SavePointFault::Io(format!("{what}: {e}"));
        let fd = nix::fcntl::openat(
            self.dir.as_fd(),
            part.as_str(),
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
        )
        .map_err(|e| io("create part", e))?;
        let outcome = (|| {
            let mut file = std::fs::File::from(fd);
            file.write_all(&save_point.bytes())
                .map_err(|e| SavePointFault::Io(format!("write part: {e}")))?;
            file.sync_all()
                .map_err(|e| SavePointFault::Io(format!("sync part: {e}")))
        })();
        if outcome.is_err() {
            self.discard_part(&name);
        }
        outcome?;
        Ok(name)
    }

    /// **Give a part its finished name**, on the harness's acknowledgement:
    /// the part is linked under the finished name, the link refusing an
    /// existing entry so nothing here overwrites, the part is unlinked, and
    /// the room is synced so the entry is durable before the name is
    /// answered. A failed link removes the part, a failed room sync removes
    /// the finished name too, and no file stands under a finished name the
    /// answer did not give.
    pub fn finish(&self, name: &str) -> Result<(), SavePointFault> {
        let part = format!(".part-{name}");
        let io = |what: &str, e: nix::errno::Errno| SavePointFault::Io(format!("{what}: {e}"));
        let linked = nix::unistd::linkat(
            self.dir.as_fd(),
            part.as_str(),
            self.dir.as_fd(),
            name,
            nix::fcntl::AtFlags::empty(),
        )
        .map_err(|e| io("link finished name", e));
        let _ = nix::unistd::unlinkat(
            self.dir.as_fd(),
            part.as_str(),
            nix::unistd::UnlinkatFlags::NoRemoveDir,
        );
        linked?;
        let name = name.to_string();
        if let Err(e) = nix::unistd::fsync(self.dir.as_fd()) {
            if let Err(why) = self.remove_finished(&name) {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "state_fault": format!("a finished save point name could not be removed after a failed room sync: {why}"),
                        "name": name,
                    })
                );
            }
            return Err(io("sync room", e));
        }
        Ok(())
    }

    /// Remove a part that will not be finished: a failed write, an
    /// acknowledgement that never came, or a part a dead member left.
    pub fn discard_part(&self, name: &str) {
        let part = format!(".part-{name}");
        let _ = nix::unistd::unlinkat(
            self.dir.as_fd(),
            part.as_str(),
            nix::unistd::UnlinkatFlags::NoRemoveDir,
        );
    }

    /// Remove every part standing in the room, read through the room's own
    /// descriptor: at the open, so a dead member's part does not outlive it,
    /// and before each write, so the room holds at most one.
    pub fn clear_parts(&self) {
        let Ok(mut listing) = nix::dir::Dir::openat(
            self.dir.as_fd(),
            ".",
            nix::fcntl::OFlag::O_RDONLY
                | nix::fcntl::OFlag::O_DIRECTORY
                | nix::fcntl::OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::empty(),
        ) else {
            return;
        };
        let parts: Vec<String> = listing
            .iter()
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().to_str().ok().map(str::to_string))
            .filter(|entry| entry.starts_with(".part-"))
            .collect();
        for part in parts {
            let _ = nix::unistd::unlinkat(
                self.dir.as_fd(),
                part.as_str(),
                nix::unistd::UnlinkatFlags::NoRemoveDir,
            );
        }
    }

    /// Remove a finished name and sync the room so the removal is durable,
    /// naming the file in the reason where either fails.
    fn remove_finished(&self, name: &str) -> Result<(), String> {
        nix::unistd::unlinkat(
            self.dir.as_fd(),
            name,
            nix::unistd::UnlinkatFlags::NoRemoveDir,
        )
        .map_err(|e| format!("unlink {name}: {e}"))?;
        nix::unistd::fsync(self.dir.as_fd())
            .map_err(|e| format!("sync room after unlink {name}: {e}"))
    }

    /// Read a save point by name from the room and nowhere else: the name
    /// must be a plain entry, the open follows no link, the bytes are judged
    /// before anything is answered, **and the name must be the save point's
    /// own**, the digest with the suffix, so one save point has one name and
    /// a copy under another is refused as an alias; the name an answer or
    /// the `save_point` event carries is thereby the digest's by
    /// construction.
    pub fn read(&self, name: &str) -> Result<SavePoint, SavePointFault> {
        use nix::fcntl::OFlag;
        if !is_plain_name(name) {
            return Err(SavePointFault::NotAPlainName);
        }
        let fd = nix::fcntl::openat(
            self.dir.as_fd(),
            name,
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::empty(),
        )
        .map_err(|e| SavePointFault::Io(format!("open {name}: {e}")))?;
        let save_point = read_regular(fd)?;
        if save_point.name() != name {
            return Err(SavePointFault::NameDisagrees);
        }
        Ok(save_point)
    }
}

/// A plain entry of the room: not empty, not `.` or `..`, no `/`, no NUL, and
/// not a dotted name, which is where the part files live.
pub fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.starts_with('.')
        && !name.contains('/')
        && !name.contains('\0')
}

/// Read a save point from a descriptor that must hold a regular file, the
/// whole of it, and judge it.
fn read_regular(fd: OwnedFd) -> Result<SavePoint, SavePointFault> {
    let stat =
        nix::sys::stat::fstat(fd.as_fd()).map_err(|e| SavePointFault::Io(format!("fstat: {e}")))?;
    if stat.st_mode & nix::libc::S_IFMT != nix::libc::S_IFREG {
        return Err(SavePointFault::Io("not a regular file".into()));
    }
    let mut file = std::fs::File::from(fd);
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| SavePointFault::Io(format!("read: {e}")))?;
    SavePoint::parse(&bytes)
}

/// The save point a load restores, read from the descriptor admin hands the
/// member at its spawn, per `weaver-state-Spec` section 2: probed before it
/// is adopted. `Ok(None)` is a number holding nothing, which is an agent's
/// first load or a hand-run member; a number holding anything but a regular
/// file open for reading, or a file that is not a sound save point, is a
/// fault the member refuses to start on.
pub fn read_descriptor(fd: std::os::fd::RawFd) -> Result<Option<SavePoint>, SavePointFault> {
    // SAFETY: F_GETFD on a number that may hold nothing fails with EBADF and
    // touches nothing.
    let flags = unsafe { nix::libc::fcntl(fd, nix::libc::F_GETFD) };
    if flags == -1 {
        let error = std::io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(nix::libc::EBADF) => Ok(None),
            _ => Err(SavePointFault::Io(format!(
                "probe descriptor {fd}: {error}"
            ))),
        };
    }
    // SAFETY: the number holds a descriptor, probed above, that admin armed
    // for this process and nothing else in it adopts.
    let owned = unsafe { <OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(fd) };
    read_regular(owned).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taken(run: &str, sequence: u64, image: &[u8]) -> SavePoint {
        SavePoint::take(
            Stamp {
                run: run.into(),
                sequence,
                turn: 2,
            },
            "CREATE TABLE event (id INTEGER PRIMARY KEY)",
            image.to_vec(),
        )
    }

    /// **A save point round-trips through its bytes and its digest is its
    /// name**, and two taken of one position differ by digest. Perturbation:
    /// drop `taken` from the header and the two names collide.
    #[test]
    fn a_save_point_round_trips_and_two_of_one_position_differ() {
        let first = taken("r-1", 41, b"image bytes");
        let parsed = SavePoint::parse(&first.bytes()).expect("parses");
        assert_eq!(parsed, first);
        assert_eq!(parsed.digest(), first.digest());
        assert_eq!(first.name(), format!("{}.save-point", first.digest()));
        assert_eq!(
            first.schema,
            schema_digest("CREATE TABLE event (id INTEGER PRIMARY KEY)")
        );
        let second = taken("r-1", 41, b"image bytes");
        assert_ne!(
            first.name(),
            second.name(),
            "two save points at one position never share a name"
        );
        assert!(
            first.header.contains("\"ordinal\":") && second.header != first.header,
            "the nonce carries the process's own count: {}",
            first.header
        );
    }

    /// **A torn or damaged save point reads as corrupt and never as
    /// holdings**: truncated part way, a byte flipped in the image, a byte
    /// flipped in the stamp, and a check line altered each refuse.
    /// Perturbation: skip the check comparison in `parse` and the flipped
    /// cases parse as sound.
    /// **The corpus holds the two readers equal**, per `weaver-admin-Spec`
    /// section 4: every file of the workspace's save-point corpus parses to
    /// the verdict the corpus gives it, so a case this parse admits and
    /// admin's judgment refuses, or the reverse, fails here or in admin's
    /// own corpus test. Perturbation: relax any one rule of `parse` and its
    /// case parses sound against a `refuses` verdict.
    #[test]
    fn the_corpus_holds_the_parse_equal_to_admins_judgment() {
        let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../weaver-types/tests/fixtures/save-points");
        let verdicts = std::fs::read_to_string(corpus.join("verdicts.txt")).expect("the verdicts");
        let mut cases = 0;
        for line in verdicts.lines().filter(|l| !l.trim().is_empty()) {
            let (name, verdict) = line.split_once(' ').expect("name and verdict");
            let bytes = std::fs::read(corpus.join(name)).expect(name);
            let parsed = SavePoint::parse(&bytes);
            match verdict {
                "sound" => assert!(parsed.is_ok(), "{name} parses sound: {parsed:?}"),
                "refuses" => assert!(parsed.is_err(), "{name} refuses"),
                other => panic!("{name}: verdict {other} is not sound or refuses"),
            }
            cases += 1;
        }
        assert!(cases >= 10, "the corpus holds its cases: {cases}");
    }

    #[test]
    fn a_torn_or_damaged_save_point_is_refused() {
        let sound = taken("r-1", 41, b"a longer image so a cut lands inside it");
        let bytes = sound.bytes();
        let truncated = &bytes[..bytes.len() - 5];
        assert!(matches!(
            SavePoint::parse(truncated),
            Err(SavePointFault::Malformed(_))
        ));
        let mut flipped = bytes.clone();
        let last = flipped.len() - 1;
        flipped[last] ^= 0x01;
        assert_eq!(SavePoint::parse(&flipped), Err(SavePointFault::CheckFailed));
        let mut stamp_flipped = bytes.clone();
        let at = sound
            .header
            .find("\"sequence\":41")
            .expect("the stamp spells its sequence")
            + "\"sequence\":".len();
        stamp_flipped[at] = b'7';
        assert_eq!(
            SavePoint::parse(&stamp_flipped),
            Err(SavePointFault::CheckFailed),
            "an altered stamp is corrupt, never a position"
        );
        // A check line spelled another way, the same check inside it, is
        // not this format: a parsed save point's digest is the file's.
        // Perturbation: parse the check line as JSON and compare the named
        // check, and the respelled file parses with a digest that is not
        // sha256 of its bytes.
        let line_end = bytes.iter().position(|&b| b == b'\n').unwrap() + 1;
        let check_end = line_end + bytes[line_end..].iter().position(|&b| b == b'\n').unwrap();
        let mut respelled = bytes[..line_end].to_vec();
        respelled.extend_from_slice(b"{ \"check\" : ");
        respelled.extend_from_slice(&bytes[line_end + 9..check_end - 1]);
        respelled.extend_from_slice(b" }");
        respelled.extend_from_slice(&bytes[check_end..]);
        assert_eq!(
            SavePoint::parse(&respelled),
            Err(SavePointFault::CheckFailed)
        );
        // **The stamp line is exactly the format's**: a stamp without its
        // nonce, with a malformed nonce, with an extra member, or missing
        // one, is not this format even with a check written to match.
        // Perturbation: read the members by name without the count and
        // the extra-member and absent-nonce stamps parse.
        let restamped = |edit: &dyn Fn(&mut serde_json::Map<String, serde_json::Value>)| {
            let mut stamp: serde_json::Map<String, serde_json::Value> =
                serde_json::from_str(&sound.header).unwrap();
            edit(&mut stamp);
            let header = serde_json::Value::Object(stamp).to_string();
            let mut hasher = sha2::Sha256::new();
            hasher.update(header.as_bytes());
            hasher.update(b"\n");
            hasher.update(&sound.image);
            let check = serde_json::json!({"check": hex(&hasher.finalize())}).to_string();
            let mut bytes = header.into_bytes();
            bytes.push(b'\n');
            bytes.extend_from_slice(check.as_bytes());
            bytes.push(b'\n');
            bytes.extend_from_slice(&sound.image);
            bytes
        };
        type Edit<'a> = &'a dyn Fn(&mut serde_json::Map<String, serde_json::Value>);
        let cases: [(&str, Edit); 5] = [
            ("no taken", &|m| {
                m.remove("taken");
            }),
            ("malformed taken", &|m| {
                m.insert("taken".into(), serde_json::json!({"pid": 1}));
            }),
            ("wall_ns not digits", &|m| {
                m.insert(
                    "taken".into(),
                    serde_json::json!({"pid": 1, "ordinal": 0, "wall_ns": "soon"}),
                );
            }),
            ("extra member", &|m| {
                m.insert("note".into(), serde_json::json!("x"));
            }),
            ("no schema", &|m| {
                m.remove("schema");
            }),
        ];
        for (label, edit) in cases {
            assert!(
                matches!(
                    SavePoint::parse(&restamped(edit)),
                    Err(SavePointFault::Malformed(_))
                ),
                "{label} is not this format"
            );
        }
        assert_eq!(
            SavePoint::parse(&restamped(&|_| {})).expect("the unedited stamp still parses"),
            sound
        );
        assert_eq!(
            SavePoint::parse(&bytes).expect("sound").digest(),
            hex(&sha2::Sha256::digest(&bytes)),
            "a parsed save point's digest is sha256 of the file"
        );
        assert!(matches!(
            SavePoint::parse(b"not a save point"),
            Err(SavePointFault::Malformed(_))
        ));
        assert!(matches!(
            SavePoint::parse(b"{}\n{}\n"),
            Err(SavePointFault::Malformed(_))
        ));
    }

    /// **The room writes a new file under the digest and never over one**,
    /// reads it back by name, refuses a name that is not a plain entry, and
    /// a second write of the same bytes is refused rather than overwriting.
    /// Perturbation: replace the link with a rename and the second write
    /// succeeds over the first.
    #[test]
    fn the_room_writes_new_files_and_overwrites_none() {
        let dir = std::env::temp_dir().join(format!(
            "weaver-state-room-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("room");
        let room = Room::open(&dir).expect("opens");
        let save_point = taken("r-1", 41, b"image");
        let name = room.write(&save_point).expect("writes");
        assert_eq!(name, save_point.name());
        assert!(dir.join(&name).is_file());
        assert!(
            !dir.join(format!(".part-{name}")).exists(),
            "the part name is gone"
        );
        assert_eq!(room.read(&name).expect("reads"), save_point);
        assert!(
            matches!(room.write(&save_point), Err(SavePointFault::Io(_))),
            "the same bytes again are refused, never overwritten"
        );
        for bad in ["", ".", "..", "../x", "a/b", ".part-x", "a\0b"] {
            assert_eq!(
                room.read(bad).err(),
                Some(SavePointFault::NotAPlainName),
                "{bad:?}"
            );
        }
        assert!(matches!(
            room.read("absent.save-point"),
            Err(SavePointFault::Io(_))
        ));
        // **One save point has one name**: the same bytes under another
        // plain name are an alias and refused, the digest's name still
        // reading. Perturbation: drop the name check from `read` and the
        // copy restores under its alias.
        std::fs::copy(dir.join(&name), dir.join("copy")).expect("copies");
        assert_eq!(room.read("copy").err(), Some(SavePointFault::NameDisagrees));
        assert_eq!(room.read(&name).expect("reads"), save_point);
        // The failed-sync cleanup is checked and names the file: a removal
        // of a name the room does not hold reports which name. Perturbation:
        // ignore the unlink's result in `remove_finished` and the absent
        // name reports success.
        let why = room
            .remove_finished("absent.save-point")
            .expect_err("an absent finished name cannot be removed");
        assert!(why.contains("unlink absent.save-point"), "{why}");
        room.remove_finished(&name)
            .expect("the finished name is removed and the room synced");
        assert!(!dir.join(&name).exists());
        let surface = room.surface().expect("surface");
        assert_eq!(surface.len(), 2);
        assert!(surface[0].starts_with("owner "));
        assert!(surface[1].starts_with("mode 0"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The descriptor is probed before it is adopted**: a closed number is
    /// no save point, a regular file holding one reads, and a number holding
    /// a directory is refused. Perturbation: drop the regular-file check and
    /// the directory case reads as an io error from the read rather than a
    /// refusal, which this test tells apart by the message.
    #[test]
    fn the_descriptor_is_probed_before_it_is_adopted() {
        use std::os::fd::IntoRawFd;
        let dir = std::env::temp_dir().join(format!(
            "weaver-state-spfd-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("one.save-point");
        let save_point = taken("r-1", 3, b"img");
        std::fs::write(&path, save_point.bytes()).expect("writes");
        let file = std::fs::File::open(&path).expect("opens");
        // A fresh high number, so no concurrent test's descriptor is touched.
        // SAFETY: F_DUPFD on a descriptor this test owns.
        let fd = unsafe { nix::libc::fcntl(file.into_raw_fd(), nix::libc::F_DUPFD, 800) };
        assert!(fd >= 800);
        let read = read_descriptor(fd).expect("reads").expect("holds one");
        assert_eq!(read, save_point);
        // Adopted and closed by the read, so the number now holds nothing.
        assert_eq!(read_descriptor(fd).expect("probes"), None);
        let directory = std::fs::File::open(&dir).expect("opens the dir");
        let fault = read_descriptor(directory.into_raw_fd()).expect_err("refused");
        assert_eq!(
            fault,
            SavePointFault::Io("not a regular file".into()),
            "a directory at the number is refused"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

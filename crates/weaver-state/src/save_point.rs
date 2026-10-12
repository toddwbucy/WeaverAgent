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

/// **The save point's bound, one gibibyte of rendered file**, per
/// `weaver-state-Spec` section 3 on the operator's ruling of 2026-10-08 that
/// both readers enforce it: admin's `SAVE_POINT_BOUND` holds the same number
/// under its own watch, the two crates seeing neither's constant, and the
/// member writes no save point past it.
pub const SAVE_POINT_BOUND: u64 = 1024 * 1024 * 1024;

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
    /// The rendered save point is past [`SAVE_POINT_BOUND`], so it is never
    /// written: the file's size and the bound it exceeds.
    PastBound { size: u64, bound: u64 },
}

impl std::fmt::Display for SavePointFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SavePointFault::Malformed(why) => write!(f, "malformed save point: {why}"),
            SavePointFault::CheckFailed => write!(f, "save point fails its check"),
            SavePointFault::Io(why) => write!(f, "save point io: {why}"),
            SavePointFault::PastBound { size, bound } => {
                write!(f, "save point is {size} bytes, past the bound of {bound}")
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

    /// The length of the file's bytes, counted without rendering them.
    pub fn rendered_len(&self) -> u64 {
        (self.header.len() + self.check_line().len() + self.image.len() + 2) as u64
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
        // **The run id and the schema digest are bounded** (the operator's
        // ruling of 2026-10-08 on #1, the custody audit's G2), as admin's
        // judgment bounds them: a run id of at most 128 printable ASCII
        // bytes, a schema of exactly 64 lowercase hex.
        let run = text("run")?;
        let schema = text("schema")?;
        stamp_bounds(&run, &schema)?;
        let parsed = SavePoint {
            stamp: Stamp {
                run,
                sequence: number("sequence")?,
                turn: number("turn")?,
            },
            schema,
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

/// **The stamp's bounds, one rule for the writer and the reader** (the
/// operator's ruling of 2026-10-08 on #1, the custody audit's G2, as admin's
/// judgment bounds them): a run id of at most 128 printable ASCII bytes and a
/// schema of exactly 64 lowercase hex. [`SavePoint::parse`] refuses a file
/// that breaks either, and [`Room::write_part_within`] refuses to write one
/// (#99, K7), so every finished file is one the readers admit.
fn stamp_bounds(run: &str, schema: &str) -> Result<(), SavePointFault> {
    let malformed = |why: &str| Err(SavePointFault::Malformed(why.to_string()));
    if run.len() > 128 || !run.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        return malformed("run is not at most 128 printable ASCII bytes");
    }
    if schema.len() != 64
        || !schema
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return malformed("schema is not 64 lowercase hex");
    }
    Ok(())
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

    /// **A test helper that skips the acknowledgement**: the part written
    /// and finished in one call, with no harness to acknowledge it. No
    /// writer outside the tests finishes a save point without one; the
    /// offline builder's Spec act decides how its writes are acknowledged
    /// (#99, K6).
    // Test-only hook kept here (#118): a `Room` method the save point suite calls.
    #[cfg(test)]
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
        self.write_part_within(save_point, SAVE_POINT_BOUND)
    }

    /// **A save point past the bound is refused before any part is
    /// created**, per `weaver-state-Spec` section 3: the standing part is
    /// cleared as for any write, the refusal leaves the room holding none,
    /// and the caller answers nothing. The bound is a parameter so a test
    /// can lower it; every writer passes [`SAVE_POINT_BOUND`]. **So is a
    /// stamp the readers would refuse** (#99, K7): a run or a schema past
    /// [`stamp_bounds`] is `Malformed` here, by the parse's own rule, so no
    /// finished file is one the member's parse or admin's judgment refuses.
    pub fn write_part_within(
        &self,
        save_point: &SavePoint,
        bound: u64,
    ) -> Result<String, SavePointFault> {
        use nix::fcntl::OFlag;
        self.clear_parts();
        stamp_bounds(&save_point.stamp.run, &save_point.schema)?;
        let size = save_point.rendered_len();
        if size > bound {
            return Err(SavePointFault::PastBound { size, bound });
        }
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
mod tests;

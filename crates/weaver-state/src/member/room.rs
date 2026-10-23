//! Raw image custody inside the inherited member identity's judged room.

use std::io::{self, Read, Write};
use std::os::fd::{AsFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::path::Path;

use nix::fcntl::OFlag;
use weaver_types::state_flow::DecimalU64;

use super::channel::invalid;

pub(super) struct Room {
    dir: OwnedFd,
}

pub(super) enum WriteFault {
    Refused(io::Error),
    Cleanup(io::Error),
}
impl From<io::Error> for WriteFault {
    fn from(e: io::Error) -> Self {
        Self::Refused(e)
    }
}
impl From<nix::errno::Errno> for WriteFault {
    fn from(e: nix::errno::Errno) -> Self {
        Self::Refused(e.into())
    }
}

impl Room {
    pub(super) fn open(path: &Path) -> io::Result<Self> {
        let dir = nix::fcntl::open(
            path,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::empty(),
        )?;
        let room = Self { dir };
        room.clear_remnants()?;
        Ok(room)
    }

    pub(super) fn surface(&self) -> io::Result<Vec<String>> {
        let stat = nix::sys::stat::fstat(self.dir.as_fd())?;
        Ok(vec![
            format!("owner {}:{}", stat.st_uid, stat.st_gid),
            format!("mode {:04o}", stat.st_mode & 0o7777),
        ])
    }

    pub(super) fn write_part(&self, id: DecimalU64, bytes: &[u8]) -> Result<String, WriteFault> {
        if id.0 == 0 {
            return Err(invalid("zero capture ID").into());
        }
        let name = format!("capture-{}.sqlite.part", id.0);
        self.write(&name, bytes)?;
        Ok(name)
    }

    pub(super) fn write_seed(&self, seed: &str, bytes: &[u8]) -> Result<String, WriteFault> {
        let name = format!("seed-{seed}.sqlite");
        if !owned_name(&name) {
            return Err(invalid("invalid seed room name").into());
        }
        self.write(&name, bytes)?;
        Ok(name)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> Result<(), WriteFault> {
        if bytes.len() as u64 > weaver_state::save_point::SAVE_POINT_BOUND {
            return Err(invalid("image exceeds bound").into());
        }
        let fd = nix::fcntl::openat(
            self.dir.as_fd(),
            name,
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
        )?;
        let result: io::Result<()> = (|| {
            let mut file = std::fs::File::from(fd);
            file.write_all(bytes)?;
            file.sync_all()?;
            nix::unistd::fsync(self.dir.as_fd())?;
            Ok(())
        })();
        if result.is_err() {
            self.remove(name).map_err(WriteFault::Cleanup)?;
        }
        result.map_err(WriteFault::Refused)
    }

    pub(super) fn finish(&self, id: DecimalU64) -> io::Result<String> {
        let part = format!("capture-{}.sqlite.part", id.0);
        let finished = format!("capture-{}.sqlite", id.0);
        // linkat refuses replacement. A failed unlink leaves both names known to
        // the capture owner, which treats it as component failure and cleans both.
        nix::unistd::linkat(
            self.dir.as_fd(),
            part.as_str(),
            self.dir.as_fd(),
            finished.as_str(),
            nix::fcntl::AtFlags::empty(),
        )?;
        if let Err(error) = nix::unistd::unlinkat(
            self.dir.as_fd(),
            part.as_str(),
            nix::unistd::UnlinkatFlags::NoRemoveDir,
        ) {
            self.remove(&finished)?;
            return Err(error.into());
        }
        if let Err(error) = nix::unistd::fsync(self.dir.as_fd()) {
            self.remove(&finished)?;
            return Err(error.into());
        }
        Ok(finished)
    }

    pub(super) fn remove(&self, name: &str) -> io::Result<()> {
        if !owned_name(name) {
            return Err(invalid("refused room name"));
        }
        nix::unistd::unlinkat(
            self.dir.as_fd(),
            name,
            nix::unistd::UnlinkatFlags::NoRemoveDir,
        )?;
        nix::unistd::fsync(self.dir.as_fd())?;
        Ok(())
    }

    pub(super) fn clear_remnants(&self) -> io::Result<()> {
        let mut listing = nix::dir::Dir::openat(
            self.dir.as_fd(),
            ".",
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::empty(),
        )?;
        for entry in listing.iter() {
            let entry = entry?;
            if let Ok(name) = entry.file_name().to_str()
                && owned_name(name)
            {
                self.remove(name)?;
            }
        }
        Ok(())
    }
}

fn owned_name(name: &str) -> bool {
    if let Some(id) = name.strip_prefix("capture-").and_then(|s| {
        s.strip_suffix(".sqlite.part")
            .or_else(|| s.strip_suffix(".sqlite"))
    }) {
        return id
            .parse::<u64>()
            .is_ok_and(|n| n > 0 && n.to_string() == id);
    }
    name.strip_prefix("seed-")
        .and_then(|s| s.strip_suffix(".sqlite"))
        .is_some_and(|id| {
            id.len() == 32
                && id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
}

/// Probe before opening any process-owned descriptors, then consume fd4 once.
pub(super) fn read_image_fd(fd: RawFd, required: bool) -> io::Result<Option<Vec<u8>>> {
    // SAFETY: fcntl only probes the supplied inherited number.
    let flags = unsafe { nix::libc::fcntl(fd, nix::libc::F_GETFD) };
    if flags < 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(nix::libc::EBADF) && !required {
            return Ok(None);
        }
        return Err(error);
    }
    if !required {
        return Err(invalid("seed mode refuses fd4"));
    }
    // SAFETY: the descriptor was just probed, and this borrow owns nothing.
    let borrowed = unsafe { BorrowedFd::borrow_raw(fd) };
    let stat = nix::sys::stat::fstat(borrowed)?;
    // SAFETY: this reads flags only, before adoption.
    let mode = unsafe { nix::libc::fcntl(fd, nix::libc::F_GETFL) };
    if mode < 0 {
        return Err(io::Error::last_os_error());
    }
    if stat.st_mode & nix::libc::S_IFMT != nix::libc::S_IFREG
        || mode & nix::libc::O_ACCMODE != nix::libc::O_RDONLY
    {
        return Err(invalid("fd4 must be a read-only regular image"));
    }
    let bound = weaver_state::save_point::SAVE_POINT_BOUND;
    if stat.st_size <= 0 || stat.st_size as u64 > bound {
        return Err(invalid("fd4 image size is invalid"));
    }
    // SAFETY: the trusted entry gives this process sole ownership after validation.
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(stat.st_size as usize)
        .map_err(io::Error::other)?;
    (&mut file).take(bound + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > bound || bytes.len() != stat.st_size as usize {
        return Err(invalid("fd4 changed or exceeded its image bound"));
    }
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "state-room-unit-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn cleanup_names_are_exact_and_cannot_leave_the_room() {
        for name in [
            "capture-1.sqlite.part",
            "capture-18446744073709551615.sqlite",
            "seed-0123456789abcdef0123456789abcdef.sqlite",
        ] {
            assert!(owned_name(name), "{name}");
        }
        for name in [
            "../capture-1.sqlite",
            "capture-01.sqlite",
            "capture-0.sqlite",
            "capture-18446744073709551616.sqlite",
            "capture-1.sqlite/child",
            "seed-A123456789abcdef0123456789abcdef.sqlite",
            "legacy.save-point",
            ".part-legacy.save-point",
            "state.log",
        ] {
            assert!(!owned_name(name), "{name}");
        }
    }

    #[test]
    fn startup_retires_only_new_owned_remnants_and_exclusive_creation_refuses_collision() {
        let scratch = Scratch::new();
        for name in [
            "capture-1.sqlite.part",
            "capture-2.sqlite",
            "seed-0123456789abcdef0123456789abcdef.sqlite",
            "old.save-point",
            ".part-old.save-point",
            "state.log",
        ] {
            std::fs::write(scratch.0.join(name), b"prior").unwrap();
        }
        let room = Room::open(&scratch.0).unwrap();
        assert!(!scratch.0.join("capture-1.sqlite.part").exists());
        assert!(!scratch.0.join("capture-2.sqlite").exists());
        assert!(
            !scratch
                .0
                .join("seed-0123456789abcdef0123456789abcdef.sqlite")
                .exists()
        );
        for name in ["old.save-point", ".part-old.save-point", "state.log"] {
            assert_eq!(std::fs::read(scratch.0.join(name)).unwrap(), b"prior");
        }
        let name = room.write_part(3.into(), b"first image").ok().unwrap();
        assert!(room.write_part(3.into(), b"replacement").is_err());
        assert_eq!(
            std::fs::read(scratch.0.join(&name)).unwrap(),
            b"first image"
        );
        assert!(room.remove("../state.log").is_err());
        room.remove(&name).unwrap();
        assert!(!scratch.0.join(name).exists());
    }

    #[test]
    fn failed_cleanup_and_symlink_room_refuse_custody() {
        let scratch = Scratch::new();
        std::fs::create_dir(scratch.0.join("capture-1.sqlite.part")).unwrap();
        assert!(Room::open(&scratch.0).is_err());
        let link = Scratch::new();
        std::os::unix::fs::symlink(&scratch.0, link.0.join("alias")).unwrap();
        assert!(Room::open(&link.0.join("alias")).is_err());
    }

    #[test]
    fn image_descriptor_is_read_only_regular_and_owned_once() {
        use std::os::fd::{AsRawFd, IntoRawFd};

        const CHILD: &str = "WEAVER_STATE_IMAGE_FD_TEST_CHILD";
        const TEST: &str =
            "member::room::tests::image_descriptor_is_read_only_regular_and_owned_once";
        if std::env::var_os(CHILD).is_none() {
            // The closed descriptor number must not be reused by another test
            // between read_image_fd returning and the ownership probe below.
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
                .env(CHILD, "1")
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                output.status.success(),
                "isolated descriptor test failed: {stdout}\n{stderr}"
            );
            assert!(
                stdout.contains("test result: ok. 1 passed;"),
                "the exact child test must run once: {stdout}\n{stderr}"
            );
            return;
        }

        let scratch = Scratch::new();
        let path = scratch.0.join("image");
        std::fs::write(&path, b"raw image bytes").unwrap();
        let writable = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        assert!(read_image_fd(writable.as_raw_fd(), true).is_err());
        assert!(read_image_fd(writable.as_raw_fd(), false).is_err());
        let directory = std::fs::File::open(&scratch.0).unwrap();
        assert!(read_image_fd(directory.as_raw_fd(), true).is_err());
        let read = std::fs::File::open(&path).unwrap();
        let fd = read.into_raw_fd();
        assert_eq!(
            read_image_fd(fd, true).unwrap().unwrap(),
            b"raw image bytes"
        );
        // No descriptor-opening operation occurs before this probe. The child
        // runs only this test, so EBADF proves the adopted File closed its fd.
        assert_eq!(unsafe { nix::libc::fcntl(fd, nix::libc::F_GETFD) }, -1);
        assert_eq!(
            io::Error::last_os_error().raw_os_error(),
            Some(nix::libc::EBADF)
        );
    }
}

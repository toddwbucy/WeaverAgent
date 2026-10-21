//! Fallible descriptor-relative writes for the publication protocol.
//!
//! Names come from admin. These operations know no capture phase, eligibility,
//! or worker outcome, and no destructor removes evidence after a failed act.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::fs::{FileExt, MetadataExt};

use nix::fcntl::{AtFlags, OFlag, RenameFlags};
use nix::sys::stat::Mode;
use nix::unistd::{Gid, Uid, UnlinkatFlags};
use sha2::{Digest as _, Sha256};
use weaver_types::state_flow::{DecimalU64, Digest};

use crate::save_points::{Owner, SAVE_POINT_BOUND};

const FILES: [&str; 3] = ["state.sqlite", "agent.toml", "binding.json"];

/// A directory created by this act beneath a held root. It remains held after
/// rename, and `name` changes before any post-rename operation can fail.
pub(super) struct PartDirectory {
    root: OwnedFd,
    directory: OwnedFd,
    name: String,
    owner: Owner,
    identity: Identity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
}

impl Identity {
    fn read(fd: BorrowedFd<'_>) -> io::Result<Self> {
        let metadata = nix::sys::stat::fstat(fd)?;
        Ok(Self {
            device: metadata.st_dev,
            inode: metadata.st_ino,
        })
    }

    fn at(root: BorrowedFd<'_>, name: &str) -> io::Result<Self> {
        let metadata = nix::sys::stat::fstatat(root, name, AtFlags::AT_SYMLINK_NOFOLLOW)?;
        Ok(Self {
            device: metadata.st_dev,
            inode: metadata.st_ino,
        })
    }

    fn require_name(self, root: BorrowedFd<'_>, name: &str) -> io::Result<()> {
        if Self::at(root, name)? != self {
            return Err(io::Error::other(
                "directory entry no longer names the held inode",
            ));
        }
        Ok(())
    }
}

pub(super) fn create_part(
    root: BorrowedFd<'_>,
    name: &str,
    owner: Owner,
) -> io::Result<PartDirectory> {
    basename(name)?;
    let root = root.try_clone_to_owned()?;
    nix::sys::stat::mkdirat(root.as_fd(), name, Mode::from_bits_truncate(0o700))?;
    // If opening fails after mkdir, no held inode proves ownership for cleanup.
    // Leave that unpublished entry visible rather than unlinking by name alone.
    let directory = nix::fcntl::openat(
        root.as_fd(),
        name,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )?;
    let identity = Identity::read(directory.as_fd())?;
    let prepared = (|| {
        set_owner_mode(directory.as_fd(), owner, 0o700)?;
        judge_directory(directory.as_fd(), owner, 0o700)?;
        identity.require_name(root.as_fd(), name)?;
        sync_directory(directory.as_fd())?;
        sync_directory(root.as_fd())
    })();
    if let Err(error) = prepared {
        if identity.require_name(root.as_fd(), name).is_ok() {
            let _ = nix::unistd::unlinkat(root.as_fd(), name, UnlinkatFlags::RemoveDir);
            let _ = sync_directory(root.as_fd());
        }
        return Err(error);
    }
    Ok(PartDirectory {
        root,
        directory,
        name: name.into(),
        owner,
        identity,
    })
}

impl PartDirectory {
    pub(super) fn fd(&self) -> BorrowedFd<'_> {
        self.directory.as_fd()
    }

    pub(super) fn name(&self) -> &str {
        &self.name
    }

    /// Make the completed directory readable by the declared access group.
    /// Its bytes and custody must already be complete before publication.
    pub(super) fn finish_mode(&mut self, owner: Owner) -> io::Result<()> {
        self.identity.require_name(self.root.as_fd(), &self.name)?;
        judge_directory(self.fd(), self.owner, 0o700)?;
        set_owner_mode(self.fd(), owner, 0o750)?;
        self.owner = owner;
        judge_directory(self.fd(), owner, 0o750)?;
        sync_directory(self.fd())
    }

    pub(super) fn rename_no_replace(&mut self, final_name: &str) -> io::Result<()> {
        basename(final_name)?;
        self.identity.require_name(self.root.as_fd(), &self.name)?;
        judge_directory(self.fd(), self.owner, 0o750)?;
        #[cfg(test)]
        checkpoint(Point::BeforeRename)?;
        nix::fcntl::renameat2(
            self.root.as_fd(),
            self.name.as_str(),
            self.root.as_fd(),
            final_name,
            RenameFlags::RENAME_NOREPLACE,
        )?;
        self.name = final_name.into();
        #[cfg(test)]
        checkpoint(Point::AfterRename)?;
        sync_directory(self.root.as_fd())?;
        self.identity.require_name(self.root.as_fd(), &self.name)
    }

    /// Remove only this held directory and its three possible files. Unknown
    /// entries or a changed inode refuse without recursive walking.
    pub(super) fn remove(&self) -> io::Result<()> {
        self.identity.require_name(self.root.as_fd(), &self.name)?;
        let mode = nix::sys::stat::fstat(self.fd())?.st_mode & 0o7777;
        if !matches!(mode, 0o700 | 0o750) {
            return Err(io::Error::other("part directory has an unexpected mode"));
        }
        judge_directory(self.fd(), self.owner, mode)?;
        let names = names(self.fd())?;
        if names
            .iter()
            .any(|name| !FILES.iter().any(|allowed| name == allowed.as_bytes()))
        {
            return Err(io::Error::other("part directory holds unknown material"));
        }
        let mut held = Vec::new();
        for name in FILES {
            if !names.contains(name.as_bytes()) {
                continue;
            }
            let file = open_read(self.fd(), name)?;
            let metadata = file.metadata()?;
            // A failed creation may not have reached its final gid or mode.
            // This private, exclusively created directory is its provenance.
            if !metadata.is_file() || metadata.uid() != self.owner.uid || metadata.nlink() != 1 {
                return Err(io::Error::other(
                    "part cleanup found material it does not own",
                ));
            }
            super::repository::judge_acl(file.as_fd(), name).map_err(io::Error::other)?;
            held.push((name, file));
        }
        for (name, file) in held {
            Identity::read(file.as_fd())?.require_name(self.fd(), name)?;
            nix::unistd::unlinkat(self.fd(), name, UnlinkatFlags::NoRemoveDir)?;
        }
        sync_directory(self.fd())?;
        self.identity.require_name(self.root.as_fd(), &self.name)?;
        nix::unistd::unlinkat(
            self.root.as_fd(),
            self.name.as_str(),
            UnlinkatFlags::RemoveDir,
        )?;
        sync_directory(self.root.as_fd())
    }
}

/// Write one new file. Any error leaves its evidence in place for explicit
/// protocol cleanup. A pre-existing entry is never opened for writing.
pub(super) fn write_new(
    directory: BorrowedFd<'_>,
    name: &str,
    bytes: &[u8],
    owner: Owner,
    mode: u32,
) -> io::Result<File> {
    let mut file = create_file(directory, name, owner, mode, false)?;
    write_bytes(&mut file, bytes)?;
    sync_file(file.as_fd())?;
    equal_bytes(&file, bytes)?;
    judge_file(&file, owner, mode, name)?;
    Identity::read(file.as_fd())?.require_name(directory, name)?;
    sync_directory(directory)?;
    Ok(file)
}

pub(super) struct CopyResult {
    pub(super) file: File,
    pub(super) sha256: Digest,
    pub(super) size_bytes: DecimalU64,
}

/// Copy only the reported whole image, hashing precisely the bytes written.
/// The caller has already judged source custody. Positional reads preserve the
/// verified source descriptor's offset, including when copying a prior artifact.
pub(super) fn copy_new(
    directory: BorrowedFd<'_>,
    name: &str,
    source: &File,
    expected_size: u64,
    owner: Owner,
    mode: u32,
) -> io::Result<CopyResult> {
    if expected_size > SAVE_POINT_BOUND {
        return Err(io::Error::other("source exceeds the save-point bound"));
    }
    let source_metadata = source.metadata()?;
    if !source_metadata.is_file() || source_metadata.len() != expected_size {
        return Err(io::Error::other("source is not the reported regular image"));
    }
    let mut file = create_file(directory, name, owner, mode, false)?;
    let mut digest = Sha256::new();
    let mut offset = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    while offset < expected_size {
        let length = usize::try_from((expected_size - offset).min(buffer.len() as u64))
            .map_err(io::Error::other)?;
        let read = read_at(source, &mut buffer[..length], offset)?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "source image ended early",
            ));
        }
        write_bytes(&mut file, &buffer[..read])?;
        digest.update(&buffer[..read]);
        offset += read as u64;
        #[cfg(test)]
        checkpoint(Point::CopiedChunk)?;
    }
    require_eof(source, expected_size)?;
    let after = source.metadata()?;
    if after.len() != expected_size
        || after.mtime() != source_metadata.mtime()
        || after.mtime_nsec() != source_metadata.mtime_nsec()
        || after.ctime() != source_metadata.ctime()
        || after.ctime_nsec() != source_metadata.ctime_nsec()
    {
        return Err(io::Error::other("source image changed during copy"));
    }
    let expected_digest = digest.finalize();
    sync_file(file.as_fd())?;
    let actual_digest = hash_exact(&file, expected_size)?;
    if actual_digest != expected_digest[..] {
        return Err(io::Error::other(
            "copied image does not read back to its hash",
        ));
    }
    judge_file(&file, owner, mode, name)?;
    Identity::read(file.as_fd())?.require_name(directory, name)?;
    sync_directory(directory)?;
    Ok(CopyResult {
        file,
        sha256: Digest::new(format!("{expected_digest:x}")).map_err(io::Error::other)?,
        size_bytes: DecimalU64(expected_size),
    })
}

/// Append to precisely the previously read index. It never truncates or repairs
/// a prior or newly written tail. An uncertain append remains visible to refusal.
pub(super) fn append_line(
    directory: BorrowedFd<'_>,
    name: &str,
    owner: Owner,
    expected_prefix: &[u8],
    record: &[u8],
) -> io::Result<()> {
    basename(name)?;
    if (!expected_prefix.is_empty() && !expected_prefix.ends_with(b"\n"))
        || record.is_empty()
        || record.contains(&b'\n')
    {
        return Err(io::Error::other(
            "index append requires complete single-line records",
        ));
    }
    let mut file = match nix::fcntl::openat(
        directory,
        name,
        OFlag::O_RDWR | OFlag::O_APPEND | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => File::from(fd),
        Err(nix::errno::Errno::ENOENT) if expected_prefix.is_empty() => {
            let file = create_file(directory, name, owner, 0o644, true)?;
            // The new index and its exact ownership are durable before the
            // first line can name any artifact.
            sync_file(file.as_fd())?;
            sync_directory(directory)?;
            file
        }
        Err(error) => return Err(error.into()),
    };
    judge_file(&file, owner, 0o644, name)?;
    equal_bytes(&file, expected_prefix)?;
    let prior_length = u64::try_from(expected_prefix.len()).map_err(io::Error::other)?;
    let record_end = prior_length
        .checked_add(u64::try_from(record.len()).map_err(io::Error::other)?)
        .ok_or_else(|| io::Error::other("index offset overflows"))?;
    let appended_end = record_end
        .checked_add(1)
        .ok_or_else(|| io::Error::other("index offset overflows"))?;
    #[cfg(test)]
    checkpoint(Point::BeforeAppend)?;
    write_bytes(&mut file, record)?;
    write_bytes(&mut file, b"\n")?;
    sync_file(file.as_fd())?;
    equal_range(&file, 0, expected_prefix)?;
    equal_range(&file, prior_length, record)?;
    equal_range(&file, record_end, b"\n")?;
    require_eof(&file, appended_end)?;
    judge_file(&file, owner, 0o644, name)?;
    Identity::read(file.as_fd())?.require_name(directory, name)?;
    sync_directory(directory)
}

/// Replace an existing guard only after its full replacement is durable. Failure
/// before rename preserves the old guard. Failure after rename retains the new
/// guard and returns an error, never leaving a gap with neither file present.
pub(super) fn replace_file(
    directory: BorrowedFd<'_>,
    name: &str,
    temporary: &str,
    expected_old: &[u8],
    replacement: &[u8],
    owner: Owner,
    mode: u32,
) -> io::Result<()> {
    basename(name)?;
    basename(temporary)?;
    if name == temporary {
        return Err(io::Error::other(
            "replacement temporary equals the existing name",
        ));
    }
    let old = open_read(directory, name)?;
    judge_file(&old, owner, mode, name)?;
    equal_bytes(&old, expected_old)?;
    let replacement = write_new(directory, temporary, replacement, owner, mode)?;
    Identity::read(old.as_fd())?.require_name(directory, name)?;
    Identity::read(replacement.as_fd())?.require_name(directory, temporary)?;
    equal_bytes(&old, expected_old)?;
    #[cfg(test)]
    checkpoint(Point::BeforeReplace)?;
    nix::fcntl::renameat(directory, temporary, directory, name)?;
    #[cfg(test)]
    checkpoint(Point::AfterReplace)?;
    sync_directory(directory)?;
    Identity::read(replacement.as_fd())?.require_name(directory, name)
}

/// Remove the exact, judged record after its owner has decided cleanup is due.
pub(super) fn remove_file(
    directory: BorrowedFd<'_>,
    name: &str,
    expected: &[u8],
    owner: Owner,
    mode: u32,
) -> io::Result<()> {
    basename(name)?;
    let file = open_read(directory, name)?;
    judge_file(&file, owner, mode, name)?;
    equal_bytes(&file, expected)?;
    Identity::read(file.as_fd())?.require_name(directory, name)?;
    #[cfg(test)]
    checkpoint(Point::BeforeRemove)?;
    nix::unistd::unlinkat(directory, name, UnlinkatFlags::NoRemoveDir)?;
    #[cfg(test)]
    checkpoint(Point::AfterRemoveDirectorySync)?;
    sync_directory(directory)
}

pub(super) fn sync_directory(directory: BorrowedFd<'_>) -> io::Result<()> {
    #[cfg(test)]
    checkpoint(Point::DirectorySync)?;
    sync_fd(directory)
}

pub(super) fn available_bytes(directory: BorrowedFd<'_>) -> io::Result<u64> {
    let stat = nix::sys::statvfs::fstatvfs(directory)?;
    stat.blocks_available()
        .checked_mul(stat.fragment_size())
        .ok_or_else(|| io::Error::other("filesystem available byte count overflows"))
}

fn basename(name: &str) -> io::Result<()> {
    if name.is_empty() || matches!(name, "." | "..") || name.contains('/') || name.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a relative basename",
        ));
    }
    Ok(())
}

fn create_file(
    directory: BorrowedFd<'_>,
    name: &str,
    owner: Owner,
    mode: u32,
    append: bool,
) -> io::Result<File> {
    basename(name)?;
    if !matches!(mode, 0o600 | 0o640 | 0o644) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unsupported exact file mode",
        ));
    }
    let mut flags =
        OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    if append {
        flags |= OFlag::O_APPEND;
    }
    let file = File::from(nix::fcntl::openat(
        directory,
        name,
        flags,
        Mode::from_bits_truncate(0o600),
    )?);
    set_owner_mode(file.as_fd(), owner, mode)?;
    judge_file(&file, owner, mode, name)?;
    Ok(file)
}

fn set_owner_mode(fd: BorrowedFd<'_>, owner: Owner, mode: u32) -> io::Result<()> {
    #[cfg(test)]
    checkpoint(Point::BeforeOwner)?;
    nix::unistd::fchown(
        fd,
        Some(Uid::from_raw(owner.uid)),
        Some(Gid::from_raw(owner.gid)),
    )?;
    #[cfg(test)]
    checkpoint(Point::BeforeMode)?;
    nix::sys::stat::fchmod(fd, Mode::from_bits_truncate(mode))?;
    Ok(())
}

fn judge_file(file: &File, owner: Owner, mode: u32, name: &str) -> io::Result<()> {
    super::repository::judge_file(file, owner, mode, name).map_err(io::Error::other)
}

fn judge_directory(directory: BorrowedFd<'_>, owner: Owner, mode: u32) -> io::Result<()> {
    let metadata = nix::sys::stat::fstat(directory)?;
    if metadata.st_mode & nix::libc::S_IFMT != nix::libc::S_IFDIR
        || metadata.st_mode & 0o7777 != mode
        || metadata.st_uid != owner.uid
        || metadata.st_gid != owner.gid
    {
        return Err(io::Error::other(
            "directory custody does not match its declaration",
        ));
    }
    super::repository::judge_acl(directory, "part directory").map_err(io::Error::other)
}

fn open_read(directory: BorrowedFd<'_>, name: &str) -> io::Result<File> {
    basename(name)?;
    Ok(File::from(nix::fcntl::openat(
        directory,
        name,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::empty(),
    )?))
}

fn sync_file(fd: BorrowedFd<'_>) -> io::Result<()> {
    #[cfg(test)]
    checkpoint(Point::FileSync)?;
    sync_fd(fd)
}

fn sync_fd(fd: BorrowedFd<'_>) -> io::Result<()> {
    sync_retry(|| nix::unistd::fsync(fd).map_err(io::Error::from))
}

fn sync_retry(mut sync: impl FnMut() -> io::Result<()>) -> io::Result<()> {
    loop {
        match sync() {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            result => return result,
        }
    }
}

fn write_bytes(file: &mut File, bytes: &[u8]) -> io::Result<()> {
    #[cfg(test)]
    if let Some(length) = partial_write() {
        file.write_all(&bytes[..length.min(bytes.len())])?;
        return Err(io::Error::other("injected partial write"));
    }
    file.write_all(bytes)
}

fn read_at(file: &File, bytes: &mut [u8], offset: u64) -> io::Result<usize> {
    loop {
        match file.read_at(bytes, offset) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            result => return result,
        }
    }
}

fn require_eof(file: &File, offset: u64) -> io::Result<()> {
    if read_at(file, &mut [0_u8], offset)? != 0 {
        return Err(io::Error::other("file has bytes past its reported end"));
    }
    Ok(())
}

fn equal_bytes(file: &File, bytes: &[u8]) -> io::Result<()> {
    equal_range(file, 0, bytes)?;
    require_eof(file, bytes.len() as u64)
}

fn equal_range(file: &File, start: u64, bytes: &[u8]) -> io::Result<()> {
    let mut checked = 0;
    let mut buffer = [0_u8; 64 * 1024];
    while checked < bytes.len() {
        let length = (bytes.len() - checked).min(buffer.len());
        let offset = start
            .checked_add(checked as u64)
            .ok_or_else(|| io::Error::other("file offset overflows"))?;
        let read = read_at(file, &mut buffer[..length], offset)?;
        if read == 0 || buffer[..read] != bytes[checked..checked + read] {
            return Err(io::Error::other(
                "file does not read back to its expected bytes",
            ));
        }
        checked += read;
    }
    Ok(())
}

fn hash_exact(file: &File, size: u64) -> io::Result<[u8; 32]> {
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut offset = 0;
    while offset < size {
        let length =
            usize::try_from((size - offset).min(buffer.len() as u64)).map_err(io::Error::other)?;
        let read = read_at(file, &mut buffer[..length], offset)?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "copied image ended early",
            ));
        }
        digest.update(&buffer[..read]);
        offset += read as u64;
    }
    require_eof(file, size)?;
    Ok(digest.finalize().into())
}

fn names(directory: BorrowedFd<'_>) -> io::Result<BTreeSet<Vec<u8>>> {
    // A fresh description avoids a previous directory read's shared offset.
    let fd = nix::fcntl::openat(
        directory,
        ".",
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC,
        Mode::empty(),
    )?;
    let mut directory = nix::dir::Dir::from_fd(fd)?;
    let mut names = BTreeSet::new();
    for entry in directory.iter() {
        let entry = entry?;
        let name = entry.file_name().to_bytes();
        if name != b"." && name != b".." {
            names.insert(name.to_vec());
        }
    }
    Ok(names)
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Point {
    BeforeOwner,
    BeforeMode,
    FileSync,
    DirectorySync,
    BeforeRename,
    AfterRename,
    BeforeAppend,
    BeforeRemove,
    AfterRemoveDirectorySync,
    BeforeReplace,
    AfterReplace,
    CopiedChunk,
}

#[cfg(test)]
thread_local! {
    static FAILURE: std::cell::RefCell<Option<(Point, usize)>> = const { std::cell::RefCell::new(None) };
    static PARTIAL_WRITE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
    static COPY_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn checkpoint(point: Point) -> io::Result<()> {
    if point == Point::CopiedChunk {
        let hook = COPY_HOOK.with(|hook| hook.borrow_mut().take());
        if let Some(hook) = hook {
            hook();
        }
    }
    FAILURE.with(|failure| {
        let mut failure = failure.borrow_mut();
        if let Some((expected, remaining)) = failure.as_mut()
            && *expected == point
        {
            if *remaining == 0 {
                *failure = None;
                return Err(io::Error::other(format!("injected {point:?}")));
            }
            *remaining -= 1;
        }
        Ok(())
    })
}

#[cfg(test)]
fn partial_write() -> Option<usize> {
    PARTIAL_WRITE.with(|value| value.take())
}

/// Scoped syscall failure for publication's guard-transition tests.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(super) enum GuardFault {
    BeforeReplace,
    AfterReplace,
    BeforeRemove,
    AfterRemoveDirectorySync,
}

#[cfg(test)]
pub(super) fn with_guard_fault<R>(
    fault: GuardFault,
    matching_calls_before_failure: usize,
    run: impl FnOnce() -> R,
) -> R {
    struct Restore(Option<(Point, usize)>);
    impl Drop for Restore {
        fn drop(&mut self) {
            FAILURE.with(|value| *value.borrow_mut() = self.0);
        }
    }
    let _restore = Restore(FAILURE.with(|value| value.borrow_mut().take()));
    let point = match fault {
        GuardFault::BeforeReplace => Point::BeforeReplace,
        GuardFault::AfterReplace => Point::AfterReplace,
        GuardFault::BeforeRemove => Point::BeforeRemove,
        GuardFault::AfterRemoveDirectorySync => Point::AfterRemoveDirectorySync,
    };
    FAILURE.with(|value| *value.borrow_mut() = Some((point, matching_calls_before_failure)));
    run()
}

/// Scoped failure at the append write boundary for independent protocol tests.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(super) enum AppendFault {
    PartialWrite(usize),
    FileSync,
}

#[cfg(test)]
pub(super) fn with_append_fault<R>(fault: AppendFault, run: impl FnOnce() -> R) -> R {
    struct Restore {
        failure: Option<(Point, usize)>,
        partial: Option<usize>,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            FAILURE.with(|value| *value.borrow_mut() = self.failure);
            PARTIAL_WRITE.with(|value| value.set(self.partial));
        }
    }
    let _restore = Restore {
        failure: FAILURE.with(|value| value.borrow_mut().take()),
        partial: PARTIAL_WRITE.with(|value| value.take()),
    };
    match fault {
        AppendFault::PartialWrite(length) => PARTIAL_WRITE.with(|value| value.set(Some(length))),
        AppendFault::FileSync => {
            FAILURE.with(|value| *value.borrow_mut() = Some((Point::FileSync, 0)))
        }
    }
    run()
}

#[cfg(test)]
mod tests;

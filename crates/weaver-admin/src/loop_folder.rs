//! **The loop admin finds standing**, per `weaver-admin-Spec` section 4: the
//! agent's own folder beside the config base holds, where the operator laid
//! one, the loop folder `loop/` with its wire file `wires.py`. Admin judges
//! both on their descriptors, reads the wire file once and carries its path
//! and the sha256 of the bytes it read on the enter. Where no loop folder
//! stands, the compiled loop runs and the enter carries none.

use std::io::Read as _;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use nix::errno::Errno;
use nix::fcntl::OFlag;
use sha2::{Digest as _, Sha256};
use weaver_types::WiresElection;

/// The loop folder's name in the agent's own folder.
pub(crate) const LOOP_FOLDER: &str = "loop";
/// The wire file's name in the loop folder.
pub(crate) const WIRE_FILE: &str = "wires.py";
/// The most bytes a wire file may hold.
pub(crate) const WIRE_FILE_BOUND: u64 = 256 * 1024;
/// The loop folder's mode: root's, listed and passed by the agent's group.
pub(crate) const LOOP_FOLDER_MODE: u32 = 0o750;
/// The wire file's mode: root's, read by the agent's group.
pub(crate) const WIRE_FILE_MODE: u32 = 0o640;

/// **The agent's own folder**: the config base's parent joined with the
/// agent's name, `/etc/weaver/<agent>/` where the root is
/// `/etc/weaver/admin/<agent>`. `None` where the root has no grandparent.
pub(crate) fn agent_folder(root: &Path, agent: &str) -> Option<PathBuf> {
    root.parent()?.parent().map(|above| above.join(agent))
}

/// **The judgment**: `Ok(None)` where the agent's folder or the loop folder
/// does not stand, the election where both stand in their shape and the
/// wire file reads, and the reason otherwise. `owner` is the uid every
/// piece must belong to (root under a real invocation), `group` the agent's
/// own group, which the loop folder and the wire file are grouped to.
pub(crate) fn judge(
    folder: &Path,
    owner: u32,
    group: u32,
) -> Result<Option<WiresElection>, String> {
    let directory = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    let agent = match nix::fcntl::open(folder, directory, nix::sys::stat::Mode::empty()) {
        Ok(fd) => fd,
        Err(Errno::ENOENT) => return Ok(None),
        Err(Errno::ELOOP) => {
            return Err(format!("the agent's folder {} is a link", folder.display()));
        }
        Err(Errno::ENOTDIR) => {
            return Err(format!(
                "the agent's folder {} is not a directory",
                folder.display()
            ));
        }
        Err(e) => {
            return Err(format!(
                "the agent's folder {} does not open ({e})",
                folder.display()
            ));
        }
    };
    let said = |what: &str| format!("the agent's folder {} {what}", folder.display());
    let metadata = stat(&agent).map_err(|e| said(&format!("does not stat ({e})")))?;
    if metadata.uid() != owner {
        return Err(said("is not root's"));
    }
    if metadata.mode() & 0o022 != 0 {
        return Err(said("is writable by its group or by others"));
    }
    no_access_entries(&agent).map_err(|why| said(&why))?;

    let loop_path = folder.join(LOOP_FOLDER);
    let said = |what: &str| format!("the loop folder {} {what}", loop_path.display());
    let held = match nix::fcntl::openat(
        agent.as_fd(),
        LOOP_FOLDER,
        directory,
        nix::sys::stat::Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(Errno::ENOENT) => return Ok(None),
        Err(Errno::ELOOP) => return Err(said("is a link")),
        Err(Errno::ENOTDIR) => return Err(said("is not a directory")),
        Err(e) => return Err(said(&format!("does not open ({e})"))),
    };
    let metadata = stat(&held).map_err(|e| said(&format!("does not stat ({e})")))?;
    if metadata.uid() != owner {
        return Err(said("is not root's"));
    }
    if metadata.gid() != group {
        return Err(said("is not grouped to the agent's group"));
    }
    if metadata.mode() & 0o7777 != LOOP_FOLDER_MODE {
        return Err(said("is not mode 0750"));
    }
    no_access_entries(&held).map_err(|why| said(&why))?;

    let wire_path = loop_path.join(WIRE_FILE);
    let said = |what: &str| format!("the wire file {} {what}", wire_path.display());
    let file = OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC;
    let wires =
        match nix::fcntl::openat(held.as_fd(), WIRE_FILE, file, nix::sys::stat::Mode::empty()) {
            Ok(fd) => fd,
            Err(Errno::ENOENT) => return Err(said("does not stand in the loop folder")),
            Err(Errno::ELOOP) => return Err(said("is a link")),
            Err(e) => return Err(said(&format!("does not open ({e})"))),
        };
    let metadata = stat(&wires).map_err(|e| said(&format!("does not stat ({e})")))?;
    if !metadata.is_file() {
        return Err(said("is not a regular file"));
    }
    if metadata.uid() != owner {
        return Err(said("is not root's"));
    }
    if metadata.gid() != group {
        return Err(said("is not grouped to the agent's group"));
    }
    if metadata.mode() & 0o7777 != WIRE_FILE_MODE {
        return Err(said("is not mode 0640"));
    }
    if metadata.len() > WIRE_FILE_BOUND {
        return Err(said("holds more than 256 KiB"));
    }
    no_access_entries(&wires).map_err(|why| said(&why))?;
    // **Read once, through the judged descriptor**, at most one byte past
    // the bound, so a file grown after the look is refused rather than
    // digested in part.
    let mut bytes = Vec::new();
    std::fs::File::from(wires)
        .take(WIRE_FILE_BOUND + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| said(&format!("does not read ({e})")))?;
    if bytes.len() as u64 > WIRE_FILE_BOUND {
        return Err(said("holds more than 256 KiB"));
    }
    let file = wire_path
        .to_str()
        .ok_or_else(|| said("is not named in UTF-8"))?
        .to_owned();
    let sha256 = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(Some(WiresElection { file, sha256 }))
}

fn stat(fd: &OwnedFd) -> std::io::Result<std::fs::Metadata> {
    std::fs::File::from(fd.try_clone()?).metadata()
}

fn no_access_entries(fd: &OwnedFd) -> Result<(), String> {
    match crate::carries_access_entries_fd(fd.as_fd()) {
        Ok(false) => Ok(()),
        Ok(true) => Err("carries an access-control entry beyond its mode".into()),
        Err(e) => Err(format!(
            "has access-control entries that cannot be read ({e})"
        )),
    }
}

#[cfg(test)]
mod tests;

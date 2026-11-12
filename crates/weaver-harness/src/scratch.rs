//! Test-only: the unit tests' scratch path guard, moved out of `lib.rs` by #118.

pub(crate) struct Scratch(pub(crate) std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        match std::fs::symlink_metadata(&self.0) {
            Ok(meta) if meta.is_dir() => drop(std::fs::remove_dir_all(&self.0)),
            Ok(_) => drop(std::fs::remove_file(&self.0)),
            Err(_) => {}
        }
    }
}

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

/// A fresh file at a predictable path under the temp directory, made
/// exclusively, so a path planted there, a link included, refuses rather than
/// being written through. A leftover from an earlier run that panicked under
/// the same pid is removed first, and removing a link removes the link and not
/// its target.
pub(crate) fn exclusive_file(path: &std::path::Path) -> std::fs::File {
    let _ = std::fs::remove_file(path);
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .expect("a scratch file, made afresh")
}

/// A fresh directory under the temp directory, any earlier one of the
/// same name removed first so a stale socket cannot refuse a bind.
pub(crate) fn dir(name: String) -> Scratch {
    let dir = Scratch(std::env::temp_dir().join(name));
    let _ = std::fs::remove_dir_all(&dir.0);
    std::fs::create_dir_all(&dir.0).expect("scratch dir");
    dir
}

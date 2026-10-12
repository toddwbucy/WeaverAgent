//! Test-only (#118): the `scratch` module, moved from `src/main.rs` verbatim.

pub(crate) struct Scratch(pub(crate) std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        match std::fs::symlink_metadata(&self.0) {
            Ok(meta) if meta.is_dir() => drop(std::fs::remove_dir_all(&self.0)),
            Ok(_) => drop(std::fs::remove_file(&self.0)),
            Err(_) => {}
        }
        // **The territory sibling too** (#113): `write_root` lays an agent's
        // territory out beside its root, as `<root>.territory`. Only for a
        // root with no extension, whose sibling that is exactly.
        if self.0.extension().is_none() {
            let sibling = self.0.with_extension("territory");
            if std::fs::symlink_metadata(&sibling).is_ok_and(|meta| meta.is_dir()) {
                drop(std::fs::remove_dir_all(sibling));
            }
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

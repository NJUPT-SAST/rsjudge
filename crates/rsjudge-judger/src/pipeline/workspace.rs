// SPDX-License-Identifier: Apache-2.0

//! The directory one submission is compiled and run in.
//!
//! The first slice keeps the directory under the system temporary directory,
//! which the demo `filePolicy` already allows for every kind of access. The
//! per-run layout, ownership and mount namespace of #548 will replace this
//! type without changing its callers.

use std::io;
use std::path::{Path, PathBuf};

/// A run directory, removed when the last handle to it drops.
#[derive(Debug)]
pub struct Workspace {
    /// The owned temporary directory.
    dir: tempfile::TempDir,
}

impl Workspace {
    /// Create an empty run directory.
    ///
    /// # Errors
    ///
    /// Returns an error when the directory cannot be created.
    pub fn create() -> io::Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix("rsjudge-run-")
            .tempdir()
            .map_err(io::Error::other)?;

        Ok(Self { dir })
    }

    /// The path of the directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// The path of `name` inside the directory.
    #[must_use]
    pub fn join(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// Write `contents` to `name` inside the directory and report its path.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be written.
    pub fn write(&self, name: &str, contents: impl AsRef<[u8]>) -> io::Result<PathBuf> {
        let path = self.join(name);
        std::fs::write(&path, contents)?;

        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::Workspace;

    #[test]
    fn writes_files_inside_the_directory() {
        let workspace = Workspace::create().expect("failed to create the workspace");
        let path = workspace
            .write("source.c", "int main(void) { return 0; }")
            .expect("failed to write the source");

        assert!(path.starts_with(workspace.path()));
        assert_eq!(
            std::fs::read_to_string(&path).expect("failed to read the source"),
            "int main(void) { return 0; }"
        );
        assert_eq!(workspace.join("source.c"), path);
    }
}

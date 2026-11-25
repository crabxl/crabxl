//! Atomic replacement of files on disk.

use crabxl_core::{Error, ErrorKind, Result};
use std::{fs::File, io::Write, path::Path};

/// Write a file through a temporary file in the same directory, then rename it
/// over `path`.
///
/// The target is either unchanged (when `write` or any later step fails) or
/// completely replaced; readers never see a partial file. Saving over a file
/// that is still open as the source of the data is safe. The data and the
/// directory entry are flushed to stable storage before the call returns.
/// An existing target keeps its permissions; a new file gets the default
/// permissions of the process (its umask applies on Unix).
pub fn write_atomically<T>(
    path: impl AsRef<Path>,
    write: impl FnOnce(&mut File) -> Result<T>,
) -> Result<T> {
    let path = path.as_ref();
    let io = |message: &str, error: std::io::Error| Error::caused_by(ErrorKind::Io, message, error);
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let existing = std::fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions());
    let mut builder = tempfile::Builder::new();
    builder.prefix(".crabxl-save-").suffix(".tmp");
    #[cfg(unix)]
    if existing.is_none() {
        use std::os::unix::fs::PermissionsExt;
        // Requested at creation, so the umask applies as for any new file.
        builder.permissions(std::fs::Permissions::from_mode(0o666));
    }
    let mut temporary = builder
        .tempfile_in(parent)
        .map_err(|error| io("Cannot create a temporary file next to the target", error))?;
    let value = write(temporary.as_file_mut())?;
    temporary
        .as_file_mut()
        .flush()
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| io("Cannot flush the temporary file", error))?;
    if let Some(permissions) = existing {
        temporary
            .as_file()
            .set_permissions(permissions)
            .map_err(|error| io("Cannot copy the target's permissions", error))?;
    }
    // The guarded temporary path is removed if the rename fails.
    let temporary = temporary.into_temp_path();
    std::fs::rename(&temporary, path)
        .map_err(|error| io("Cannot replace the target file", error))?;
    // The rename succeeded: stop guarding the old temporary name.
    let _ = temporary.keep();
    #[cfg(unix)]
    {
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| io("Cannot flush the target directory", error))?;
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn replaces_completely_or_not_at_all() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("book.xlsx");
        std::fs::write(&target, b"old").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
        }
        let failed = write_atomically(&target, |file| {
            file.write_all(b"partial").unwrap();
            Err::<(), _>(Error::new(ErrorKind::Io, "injected"))
        });
        assert!(failed.is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
        write_atomically(&target, |file| {
            file.write_all(b"new")
                .map_err(|error| Error::caused_by(ErrorKind::Io, "write", error))
        })
        .unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&target).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o640);
        }
        // No temporary files remain.
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}

//! Native local-filesystem backend for macOS and Linux.

use cyber_pumpkin_backend::{Backend, BackendError, ErrorKind, ReadStream, WriteStream};
use cyber_pumpkin_core::{
    BackendCapabilities, BackendId, BackendPath, CapabilitySupport, EntryKind, FileEntry,
};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;

/// Backend that exposes the host filesystem through the common I/O contract.
#[derive(Clone, Debug)]
pub struct LocalBackend {
    id: BackendId,
}

impl LocalBackend {
    /// Creates a local backend with the supplied configured identifier.
    #[must_use]
    pub const fn new(id: BackendId) -> Self {
        Self { id }
    }

    fn native_path(path: &BackendPath) -> &Path {
        Path::new(path.as_str())
    }

    fn entry_from_path(path: &Path, metadata: &fs::Metadata) -> Result<FileEntry, BackendError> {
        let path_text = path.to_str().ok_or_else(|| {
            BackendError::new(
                ErrorKind::InvalidInput,
                "decode path",
                None,
                "local path is not valid UTF-8",
            )
        })?;

        let name = match path.file_name() {
            Some(value) => value
                .to_str()
                .ok_or_else(|| {
                    BackendError::new(
                        ErrorKind::InvalidInput,
                        "decode file name",
                        None,
                        "local file name is not valid UTF-8",
                    )
                })?
                .to_owned(),
            None => path_text.to_owned(),
        };

        let file_type = metadata.file_type();
        let kind = if file_type.is_file() {
            EntryKind::File
        } else if file_type.is_dir() {
            EntryKind::Directory
        } else if file_type.is_symlink() {
            EntryKind::Symlink
        } else {
            EntryKind::Other
        };

        let backend_path = BackendPath::new(path_text).map_err(|error| {
            BackendError::new(
                ErrorKind::InvalidInput,
                "convert path",
                None,
                error.to_string(),
            )
        })?;

        Ok(FileEntry {
            path: backend_path,
            name,
            kind,
            size: file_type.is_file().then_some(metadata.len()),
        })
    }

    fn io_error(operation: &'static str, path: &BackendPath, error: &io::Error) -> BackendError {
        let kind = match error.kind() {
            io::ErrorKind::NotFound => ErrorKind::NotFound,
            io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            io::ErrorKind::InvalidInput => ErrorKind::InvalidInput,
            _ => ErrorKind::Io,
        };
        BackendError::new(kind, operation, Some(path.clone()), error.to_string())
    }
}

impl Backend for LocalBackend {
    fn id(&self) -> &BackendId {
        &self.id
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            resumable_write: CapabilitySupport::Unsupported,
            atomic_rename: CapabilitySupport::Supported,
            unix_permissions: CapabilitySupport::Supported,
            server_checksum: CapabilitySupport::Unsupported,
        }
    }

    fn list(&self, path: &BackendPath) -> Result<Vec<FileEntry>, BackendError> {
        let entries = fs::read_dir(Self::native_path(path))
            .map_err(|error| Self::io_error("list directory", path, &error))?;
        let mut output = Vec::new();

        for entry_result in entries {
            let entry = entry_result
                .map_err(|error| Self::io_error("read directory entry", path, &error))?;
            let native_path = entry.path();
            let metadata = fs::symlink_metadata(&native_path)
                .map_err(|error| Self::io_error("read metadata", path, &error))?;
            output.push(Self::entry_from_path(&native_path, &metadata)?);
        }

        output.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(output)
    }

    fn stat(&self, path: &BackendPath) -> Result<FileEntry, BackendError> {
        let native = Self::native_path(path);
        let metadata = fs::symlink_metadata(native)
            .map_err(|error| Self::io_error("read metadata", path, &error))?;
        Self::entry_from_path(native, &metadata)
    }

    fn open_read(&self, path: &BackendPath) -> Result<ReadStream, BackendError> {
        let file = File::open(Self::native_path(path))
            .map_err(|error| Self::io_error("open for read", path, &error))?;
        Ok(Box::new(file))
    }

    fn open_write(&self, path: &BackendPath) -> Result<WriteStream, BackendError> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(Self::native_path(path))
            .map_err(|error| Self::io_error("open for write", path, &error))?;
        Ok(Box::new(file))
    }

    fn create_dir(&self, path: &BackendPath) -> Result<(), BackendError> {
        fs::create_dir(Self::native_path(path))
            .map_err(|error| Self::io_error("create directory", path, &error))
    }

    fn rename(&self, source: &BackendPath, destination: &BackendPath) -> Result<(), BackendError> {
        fs::rename(Self::native_path(source), Self::native_path(destination))
            .map_err(|error| Self::io_error("rename", source, &error))
    }

    fn remove(&self, path: &BackendPath) -> Result<(), BackendError> {
        let metadata = fs::symlink_metadata(Self::native_path(path))
            .map_err(|error| Self::io_error("read metadata before remove", path, &error))?;
        if metadata.file_type().is_dir() {
            fs::remove_dir(Self::native_path(path))
                .map_err(|error| Self::io_error("remove directory", path, &error))
        } else {
            fs::remove_file(Self::native_path(path))
                .map_err(|error| Self::io_error("remove file", path, &error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LocalBackend;
    use cyber_pumpkin_backend::Backend;
    use cyber_pumpkin_core::{BackendId, BackendPath, EntryKind};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    fn test_directory() -> PathBuf {
        let serial = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "cyber-pumpkin-local-{}-{serial}",
            std::process::id()
        ))
    }

    fn backend_path(path: &std::path::Path) -> Result<BackendPath, Box<dyn std::error::Error>> {
        let text = path.to_str().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "test path must be UTF-8")
        })?;
        Ok(BackendPath::new(text)?)
    }

    #[test]
    fn lists_and_stats_native_files() -> Result<(), Box<dyn std::error::Error>> {
        let root = test_directory();
        fs::create_dir(&root)?;
        fs::write(root.join("hello.txt"), b"pumpkin")?;

        let backend = LocalBackend::new(BackendId::new("local")?);
        let root_path = backend_path(&root)?;
        let entries = backend.list(&root_path)?;

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "hello.txt");
        assert_eq!(entries[0].kind, EntryKind::File);
        assert_eq!(entries[0].size, Some(7));

        let file_path = backend_path(&root.join("hello.txt"))?;
        let stat = backend.stat(&file_path)?;
        assert_eq!(stat.size, Some(7));

        fs::remove_dir_all(&root)?;
        Ok(())
    }
}

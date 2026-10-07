//! Safe Linux/Android filesystem ownership for caller-supplied private fixtures.
//!
//! Uses pinned `rustix` 1.1.5, no raw descriptors/unsafe FFI. All writers must
//! cooperate on the same stable lock inode. Caller controls trusted ancestors
//! and must prevent directory/lock replacement and hard-link manipulation.
//! Directory and leaf descriptors are anchored, no-follow and nonblocking;
//! lock/data leaves must be regular files. No path discovery or JVM coexistence.
//! Other targets fail closed before filesystem effects. Android compilation is
//! not device execution or power-loss durability evidence.

use crate::preferences::MAX_BYTES;
use std::fmt;
use std::sync::atomic::AtomicU64;

pub(crate) const DATA_FILE: &str = "preferences.pb";
pub(crate) const LOCK_FILE: &str = ".preferences.lock";
pub(crate) static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[cfg(any(target_os = "linux", target_os = "android"))]
mod platform {
    use super::{DATA_FILE, LOCK_FILE, MAX_BYTES, TEMP_SEQUENCE};
    use crate::preferences::{CommitOutcome, Error};
    use rustix::fs::{
        AtFlags, FileType, FlockOperation, Mode, OFlags, flock, fstat, fsync, open, openat,
        renameat, unlinkat,
    };
    use rustix::io::Errno;
    use std::fs::File;
    use std::io::{Read, Write};
    use std::path::Path;
    use std::sync::atomic::Ordering;

    pub(crate) struct DirectoryLock {
        directory: File,
        lock: File,
    }
    impl DirectoryLock {
        pub(crate) fn open(path: &Path) -> Result<Self, Error> {
            // Remove trailing separators/dots so NOFOLLOW checks actual leaf,
            // not an implied '.' after a directory symlink. Ancestors trusted.
            let path: std::path::PathBuf = path.components().collect();
            let directory = File::from(
                open(
                    path,
                    OFlags::RDONLY
                        | OFlags::DIRECTORY
                        | OFlags::CLOEXEC
                        | OFlags::NOFOLLOW
                        | OFlags::NONBLOCK,
                    Mode::empty(),
                )
                .map_err(|_| Error::Io)?,
            );
            let lock = File::from(
                openat(
                    &directory,
                    LOCK_FILE,
                    OFlags::RDWR
                        | OFlags::CREATE
                        | OFlags::CLOEXEC
                        | OFlags::NOFOLLOW
                        | OFlags::NONBLOCK,
                    Mode::RUSR | Mode::WUSR,
                )
                .map_err(|_| Error::Io)?,
            );
            regular(&lock)?;
            flock(&lock, FlockOperation::NonBlockingLockExclusive).map_err(|error| {
                if error == Errno::AGAIN {
                    Error::Owned
                } else {
                    Error::Io
                }
            })?;
            Ok(Self { directory, lock })
        }
        pub(crate) fn read(&self) -> Result<Option<Vec<u8>>, Error> {
            let fd = match openat(
                &self.directory,
                DATA_FILE,
                OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
                Mode::empty(),
            ) {
                Ok(fd) => fd,
                Err(Errno::NOENT) => return Ok(None),
                Err(_) => return Err(Error::Io),
            };
            let file = File::from(fd);
            regular(&file)?;
            let mut bytes = Vec::new();
            file.take(u64::try_from(MAX_BYTES + 1).map_err(|_| Error::LimitExceeded)?)
                .read_to_end(&mut bytes)
                .map_err(|_| Error::Io)?;
            if bytes.len() > MAX_BYTES {
                return Err(Error::LimitExceeded);
            }
            Ok(Some(bytes))
        }
        pub(crate) fn sync(&self) -> CommitOutcome {
            if fsync(&self.directory).is_ok() {
                CommitOutcome::Synced
            } else {
                CommitOutcome::DirectorySyncUnavailable
            }
        }
    }
    fn regular(file: &File) -> Result<(), Error> {
        let stat = fstat(file).map_err(|_| Error::Io)?;
        if FileType::from_raw_mode(stat.st_mode) == FileType::RegularFile {
            Ok(())
        } else {
            Err(Error::Io)
        }
    }
    impl Drop for DirectoryLock {
        fn drop(&mut self) {
            // Also releases on load/decode errors. Explicit unlock prevents a
            // transient fork-inherited descriptor delaying normal owner release.
            let _ = flock(&self.lock, FlockOperation::Unlock);
        }
    }
    pub(crate) struct Temporary<'a> {
        directory: &'a DirectoryLock,
        name: String,
        file: File,
        committed: bool,
    }
    impl<'a> Temporary<'a> {
        pub(crate) fn create(directory: &'a DirectoryLock) -> Result<Self, Error> {
            Self::create_from(directory, TEMP_SEQUENCE.fetch_add(32, Ordering::Relaxed))
        }
        pub(crate) fn create_from(directory: &'a DirectoryLock, start: u64) -> Result<Self, Error> {
            for offset in 0..32 {
                let name = format!(
                    ".preferences-{}-{}.tmp",
                    std::process::id(),
                    start.wrapping_add(offset)
                );
                match openat(
                    &directory.directory,
                    name.as_str(),
                    OFlags::WRONLY
                        | OFlags::CREATE
                        | OFlags::EXCL
                        | OFlags::CLOEXEC
                        | OFlags::NOFOLLOW
                        | OFlags::NONBLOCK,
                    Mode::RUSR | Mode::WUSR,
                ) {
                    Ok(fd) => {
                        return Ok(Self {
                            directory,
                            name,
                            file: File::from(fd),
                            committed: false,
                        });
                    }
                    Err(Errno::EXIST) => {}
                    Err(_) => return Err(Error::Io),
                }
            }
            Err(Error::Io)
        }
        pub(crate) fn write(&mut self, bytes: &[u8]) -> Result<(), Error> {
            self.file.write_all(bytes).map_err(|_| Error::Io)
        }
        pub(crate) fn sync(&self) -> Result<(), Error> {
            fsync(&self.file).map_err(|_| Error::Io)
        }
        pub(crate) fn commit(&mut self) -> Result<(), Error> {
            renameat(
                &self.directory.directory,
                self.name.as_str(),
                &self.directory.directory,
                DATA_FILE,
            )
            .map_err(|_| Error::Io)?;
            self.committed = true;
            Ok(())
        }
        #[cfg(test)]
        pub(crate) fn name(&self) -> &str {
            &self.name
        }
    }
    impl Drop for Temporary<'_> {
        fn drop(&mut self) {
            if !self.committed {
                let _ = unlinkat(
                    &self.directory.directory,
                    self.name.as_str(),
                    AtFlags::empty(),
                );
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{DirectoryLock, Temporary};
        use crate::preference_lock::{DATA_FILE, TEMP_SEQUENCE};
        use crate::preferences::{CommitOutcome, Error};
        use rustix::fs::{Mode, OFlags, openat};
        use std::fs::File;
        use std::os::fd::OwnedFd;
        use std::os::unix::{fs::PermissionsExt as _, net::UnixStream};
        use std::path::Path;
        use std::path::PathBuf;
        use std::sync::atomic::Ordering;
        struct Directory(PathBuf);
        impl Directory {
            fn new() -> Self {
                let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("kernel-fixtures");
                std::fs::create_dir_all(&root).unwrap();
                let path = root.join(format!(
                    "kernel-{}-{}",
                    std::process::id(),
                    TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
                ));
                std::fs::create_dir(&path).unwrap();
                Self(path)
            }
        }
        impl Drop for Directory {
            fn drop(&mut self) {
                std::fs::remove_dir_all(&self.0).unwrap();
            }
        }
        #[test]
        fn actual_descriptor_write_fsync_rename_failures_leave_target_unchanged() {
            let d = Directory::new();
            let target = d.0.join(DATA_FILE);
            std::fs::write(&target, b"old synthetic bytes").unwrap();
            let lease = DirectoryLock::open(&d.0).unwrap();
            let old = std::fs::read(&target).unwrap();
            let mut temp = Temporary::create(&lease).unwrap();
            // Reopen only test temp as read-only: actual write returns EBADF.
            temp.file = File::from(
                openat(
                    &lease.directory,
                    temp.name.as_str(),
                    OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
                    Mode::empty(),
                )
                .unwrap(),
            );
            assert_eq!(temp.write(b"new synthetic bytes"), Err(Error::Io));
            drop(temp);
            assert!(std::fs::read(&target).unwrap().eq(&old));
            let mut temp = Temporary::create(&lease).unwrap();
            // Anonymous socket descriptor gives genuine unsupported fsync; no
            // filesystem fault/power-loss or device execution claim.
            let (socket, _peer) = UnixStream::pair().unwrap();
            temp.file = File::from(OwnedFd::from(socket));
            assert_eq!(temp.sync(), Err(Error::Io));
            drop(temp);
            assert!(std::fs::read(&target).unwrap().eq(&old));
            let mut temp = Temporary::create(&lease).unwrap();
            temp.write(b"new synthetic bytes").unwrap();
            temp.sync().unwrap();
            std::fs::set_permissions(&d.0, std::fs::Permissions::from_mode(0o500)).unwrap();
            let result = temp.commit();
            std::fs::set_permissions(&d.0, std::fs::Permissions::from_mode(0o700)).unwrap();
            assert_eq!(result, Err(Error::Io));
            drop(temp);
            assert!(std::fs::read(&target).unwrap().eq(&old));
            assert_eq!(std::fs::read_dir(&d.0).unwrap().count(), 2);
        }
        #[test]
        fn directory_sync_kernel_error_is_explicit_committed_outcome() {
            let d = Directory::new();
            let mut lease = DirectoryLock::open(&d.0).unwrap();
            let (socket, _peer) = UnixStream::pair().unwrap();
            lease.directory = File::from(OwnedFd::from(socket));
            assert_eq!(lease.sync(), CommitOutcome::DirectorySyncUnavailable);
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
mod platform {
    use crate::preferences::{CommitOutcome, Error};
    use std::path::Path;
    pub(crate) struct DirectoryLock {
        unavailable: std::convert::Infallible,
    }
    pub(crate) struct Temporary<'a>(&'a DirectoryLock);
    impl DirectoryLock {
        pub(crate) fn open(_: &Path) -> Result<Self, Error> {
            Err(Error::LockUnsupported)
        }
        pub(crate) fn read(&self) -> Result<Option<Vec<u8>>, Error> {
            Err(Error::LockUnsupported)
        }
        pub(crate) fn sync(&self) -> CommitOutcome {
            match self.unavailable {}
        }
    }
    impl<'a> Temporary<'a> {
        pub(crate) fn create(_: &'a DirectoryLock) -> Result<Self, Error> {
            Err(Error::LockUnsupported)
        }
        pub(crate) fn write(&mut self, _: &[u8]) -> Result<(), Error> {
            Err(Error::LockUnsupported)
        }
        pub(crate) fn sync(&self) -> Result<(), Error> {
            Err(Error::LockUnsupported)
        }
        pub(crate) fn commit(&mut self) -> Result<(), Error> {
            Err(Error::LockUnsupported)
        }
    }
}

pub(crate) use platform::{DirectoryLock, Temporary};
impl fmt::Debug for DirectoryLock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DirectoryLock([redacted])")
    }
}
impl fmt::Debug for Temporary<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Temporary([redacted])")
    }
}

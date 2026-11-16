//! Local publication barriers. Errors are never downgraded to success.
//! Requires a filesystem/device honoring file and directory sync semantics.

use std::fs::File;
use std::path::Path;

/// Flush file data and metadata, including the drive cache on macOS.
pub fn sync_file(file: &File) -> std::io::Result<()> {
    file.sync_all()?;
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        // SAFETY: a live borrowed descriptor, no pointer arguments; the
        // operation does not transfer ownership or invalidate the descriptor.
        if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_FULLFSYNC) } == -1 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Persist directory entries, failing on platforms/filesystems that cannot.
pub fn sync_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(feature = "test-faults")]
    test_faults::before_sync(path)?;
    sync_file(&File::open(path)?)
}

/// Explicit test-build-only, thread-local failures; no environment switch and
/// no way to turn a failed barrier into success. Absent from default builds.
#[cfg(feature = "test-faults")]
#[doc(hidden)]
pub mod test_faults {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    struct Fault {
        path: PathBuf,
        remaining: usize,
        kind: std::io::ErrorKind,
        before_failure: Box<dyn FnOnce()>,
    }
    thread_local! { static FAULT: RefCell<Option<Fault>> = const { RefCell::new(None) }; }
    pub struct Guard(std::marker::PhantomData<std::rc::Rc<()>>);
    impl Drop for Guard {
        fn drop(&mut self) {
            FAULT.with(|fault| {
                fault.replace(None);
            });
        }
    }
    pub fn fail_dir_sync(path: PathBuf, occurrence: usize, kind: std::io::ErrorKind) -> Guard {
        fail_dir_sync_with(path, occurrence, kind, || {})
    }
    pub fn fail_dir_sync_with(
        path: PathBuf,
        occurrence: usize,
        kind: std::io::ErrorKind,
        before_failure: impl FnOnce() + 'static,
    ) -> Guard {
        assert!(occurrence > 0);
        FAULT.with(|fault| {
            assert!(fault.borrow().is_none());
            fault.replace(Some(Fault {
                path: path.canonicalize().unwrap(),
                remaining: occurrence,
                kind,
                before_failure: Box::new(before_failure),
            }));
        });
        Guard(std::marker::PhantomData)
    }
    pub(super) fn before_sync(path: &Path) -> std::io::Result<()> {
        let path = path.canonicalize()?;
        let injected = FAULT.with(|fault| {
            let mut fault = fault.borrow_mut();
            if let Some(active) = fault.as_mut()
                && active.path == path
            {
                active.remaining -= 1;
                if active.remaining == 0 {
                    return fault.take();
                }
            }
            None
        });
        if let Some(fault) = injected {
            (fault.before_failure)();
            return Err(std::io::Error::new(
                fault.kind,
                "injected directory sync failure",
            ));
        }
        Ok(())
    }
}

/// Create a directory tree and publish each new entry bottom-up. Existing
/// ancestors are synced too: an earlier failed creation may still be visible
/// in the cache. Call at initialization/publication, not per buffered row.
pub fn create_dir_all(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    let absolute = path.canonicalize()?;
    for directory in absolute.ancestors() {
        sync_dir(directory)?;
    }
    Ok(())
}

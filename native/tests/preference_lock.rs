//! Synthetic safe descriptor ownership tests; no Android execution claim.
#![deny(
    missing_docs,
    unsafe_code,
    rust_2018_idioms,
    unused_lifetimes,
    clippy::all,
    clippy::pedantic
)]
#[path = "../src/preference_lock.rs"]
mod preference_lock;
#[path = "../src/preferences.rs"]
mod preferences;

use preference_lock::{DATA_FILE, DirectoryLock, LOCK_FILE, Temporary};
use preferences::{Error, FixtureStore, Value};
use rustix::fs::{CWD, FileType, Mode, mknodat};
use std::fs;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        fs::create_dir_all(&root).expect("fixture preparation failed");
        let path = root.join(format!(
            "lock-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("fixture preparation failed");
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("fixture cleanup failed");
    }
}

#[test]
fn descriptor_debug_redacts_all_authority() {
    let d = Directory::new();
    let lease = DirectoryLock::open(&d.0).unwrap();
    let temp = Temporary::create(&lease).unwrap();
    assert_eq!(format!("{lease:?}"), "DirectoryLock([redacted])");
    assert_eq!(format!("{temp:?}"), "Temporary([redacted])");
}

#[test]
fn directory_lock_and_data_symlink_leaves_never_follow_external_target() {
    let external = Directory::new();
    let sentinel = external.0.join("sentinel");
    fs::write(&sentinel, b"synthetic bytes").unwrap();
    for leaf in [LOCK_FILE, DATA_FILE] {
        let d = Directory::new();
        symlink(&sentinel, d.0.join(leaf)).unwrap();
        assert!(matches!(FixtureStore::open(&d.0), Err(Error::Io)));
        assert!(fs::read(&sentinel).unwrap().eq(b"synthetic bytes"));
        fs::remove_file(d.0.join(leaf)).unwrap();
        symlink(external.0.join("absent"), d.0.join(leaf)).unwrap();
        assert!(matches!(FixtureStore::open(&d.0), Err(Error::Io)));
        assert!(!external.0.join("absent").exists());
    }
    let d = Directory::new();
    let alias = d.0.join("alias");
    symlink(&external.0, &alias).unwrap();
    for path in [alias.clone(), alias.join(""), alias.join(".")] {
        assert!(matches!(FixtureStore::open(&path), Err(Error::Io)));
    }
    assert!(!external.0.join(LOCK_FILE).exists());
}

#[test]
fn fifo_probe_helper_process() {
    let Some(path) = std::env::var_os("EH_PREFERENCE_FIFO_PROBE") else {
        return;
    };
    assert!(matches!(
        FixtureStore::open(Path::new(&path)),
        Err(Error::Io)
    ));
}

#[test]
fn fifo_leaves_reject_without_blocking_in_bounded_helper() {
    for leaf in [LOCK_FILE, DATA_FILE] {
        let d = Directory::new();
        mknodat(
            CWD,
            d.0.join(leaf),
            FileType::Fifo,
            Mode::RUSR | Mode::WUSR,
            0,
        )
        .unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "fifo_probe_helper_process"])
            .env("EH_PREFERENCE_FIFO_PROBE", &d.0)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            if Instant::now() > deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("fixture FIFO rejection timed out");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

#[test]
fn anchored_descriptor_does_not_redirect_after_directory_path_replacement() {
    let parent = Directory::new();
    let path = parent.0.join("private");
    fs::create_dir(&path).unwrap();
    let mut store = FixtureStore::open(&path).unwrap();
    store
        .update(|edit| edit.set("x".to_owned(), Value::Integer(1)))
        .unwrap();
    let moved = parent.0.join("retained");
    fs::rename(&path, &moved).unwrap();
    fs::create_dir(&path).unwrap();
    store
        .update(|edit| edit.set("x".to_owned(), Value::Integer(2)))
        .unwrap();
    assert!(!path.join(DATA_FILE).exists());
    assert!(!path.join(LOCK_FILE).exists());
    drop(store);
    assert_eq!(
        FixtureStore::open(&moved).unwrap().preferences().get("x"),
        Some(&Value::Integer(2))
    );
}

#[test]
fn load_errors_release_lock_and_inode_survives_reopen() {
    let d = Directory::new();
    fs::write(d.0.join(DATA_FILE), [10, 5, 10]).unwrap();
    assert!(matches!(FixtureStore::open(&d.0), Err(Error::Truncated)));
    let inode = fs::metadata(d.0.join(LOCK_FILE)).unwrap().ino();
    fs::remove_file(d.0.join(DATA_FILE)).unwrap();
    fs::create_dir(d.0.join(DATA_FILE)).unwrap();
    assert!(matches!(FixtureStore::open(&d.0), Err(Error::Io)));
    fs::remove_dir(d.0.join(DATA_FILE)).unwrap();
    let mut store = FixtureStore::open(&d.0).unwrap();
    store
        .update(|edit| edit.set("x".to_owned(), Value::Boolean(true)))
        .unwrap();
    drop(store);
    assert_eq!(fs::metadata(d.0.join(LOCK_FILE)).unwrap().ino(), inode);
    assert_eq!(
        fs::metadata(d.0.join(DATA_FILE))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(d.0.join(LOCK_FILE))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn current_data_symlink_and_fifo_drift_refuse_commit() {
    for fifo in [false, true] {
        let d = Directory::new();
        let outside = Directory::new();
        let mut store = FixtureStore::open(&d.0).unwrap();
        store
            .update(|edit| edit.set("x".to_owned(), Value::Integer(1)))
            .unwrap();
        fs::remove_file(d.0.join(DATA_FILE)).unwrap();
        if fifo {
            mknodat(
                CWD,
                d.0.join(DATA_FILE),
                FileType::Fifo,
                Mode::RUSR | Mode::WUSR,
                0,
            )
            .unwrap();
        } else {
            fs::write(outside.0.join(DATA_FILE), []).unwrap();
            symlink(outside.0.join(DATA_FILE), d.0.join(DATA_FILE)).unwrap();
        }
        assert_eq!(
            store.update(|_| panic!("unsafe leaf callback must not run")),
            Err(Error::Io)
        );
        assert_eq!(store.preferences().get("x"), Some(&Value::Integer(1)));
        if !fifo {
            assert!(fs::read(outside.0.join(DATA_FILE)).unwrap().is_empty());
        }
    }
}

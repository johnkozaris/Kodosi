use std::io::Read as _;

use super::*;

fn real_tempdir() -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let real = root.path().canonicalize().unwrap();
    (root, real)
}

#[test]
fn private_directories_reject_relative_and_dot_paths() {
    assert!(create_private_dir(Path::new("relative")).is_err());
    let (_root, real) = real_tempdir();
    assert!(create_private_dir(&real.join("a/../b")).is_err());
}

#[test]
fn concurrent_bootstrap_creates_private_directories() {
    let (_root, real) = real_tempdir();
    let barrier = std::sync::Barrier::new(8);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                for index in 0..32 {
                    barrier.wait();
                    create_private_dir(&real.join(format!("race-{index}/nested/core"))).unwrap();
                }
            });
        }
    });
    for index in 0..32 {
        let parent = real.join(format!("race-{index}"));
        for path in [
            parent.clone(),
            parent.join("nested"),
            parent.join("nested/core"),
        ] {
            let metadata = std::fs::symlink_metadata(&path).unwrap();
            assert!(metadata.is_dir());
            #[cfg(unix)]
            {
                use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
                assert_eq!(metadata.uid(), rustix::process::geteuid().as_raw());
                assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
            }
        }
    }
}

#[test]
fn private_writes_replace_atomically() {
    let (_root, real) = real_tempdir();
    let path = real.join("secrets/value");
    write_private(&path, b"first").unwrap();
    write_private(&path, b"second").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"second");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn private_files_open_once_and_lock() {
    let (_root, real) = real_tempdir();
    let path = real.join("host.lock");
    let first = open_private(&path).unwrap();
    first.try_lock().unwrap();
    let second = open_private(&path).unwrap();
    assert!(second.try_lock().is_err());
    drop(first);
    assert!(second.try_lock().is_ok());
}

#[test]
fn beneath_opens_accept_regular_files_and_reject_escapes() {
    let (_root, real) = real_tempdir();
    std::fs::create_dir(real.join("folder")).unwrap();
    std::fs::write(real.join("folder/file"), b"content").unwrap();
    let root = open_dir(&real).unwrap();
    let folder = open_dir_beneath(&root, "folder").unwrap();
    let mut text = String::new();
    open_file_beneath(&folder, "file")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(text, "content");
    for name in ["", ".", "..", "a/b", "a\\b"] {
        assert!(open_file_beneath(&folder, name).is_err());
    }
    assert!(open_file_beneath(&root, "folder").is_err());
}

#[cfg(unix)]
#[test]
fn private_storage_refuses_symlinks_without_touching_the_target() {
    use std::os::unix::fs::{PermissionsExt as _, symlink};
    let (_root, real) = real_tempdir();
    let target = real.join("target");
    std::fs::create_dir(&target).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o750)).unwrap();
    let link = real.join("link");
    symlink(&target, &link).unwrap();
    assert!(create_private_dir(&link.join("core")).is_err());
    assert!(!target.join("core").exists());
    assert_eq!(
        std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o750
    );
    std::fs::write(target.join("file"), b"x").unwrap();
    symlink(target.join("file"), real.join("file-link")).unwrap();
    assert!(open_private(&real.join("file-link")).is_err());
    let root = open_dir(&real).unwrap();
    assert!(open_dir_beneath(&root, "link").is_err());
    assert!(open_file_beneath(&root, "file-link").is_err());
}

#[cfg(unix)]
#[test]
fn private_files_refuse_shared_permissions() {
    use std::os::unix::fs::PermissionsExt as _;
    let (_root, real) = real_tempdir();
    let path = real.join("shared");
    std::fs::write(&path, b"").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(open_private(&path).is_err());
}

#[cfg(unix)]
#[test]
fn executables_need_execute_permission() {
    use std::os::unix::fs::PermissionsExt as _;
    let (_root, real) = real_tempdir();
    let path = real.join("tool");
    std::fs::write(&path, b"#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(!is_executable(&path));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(is_executable(&path));
}

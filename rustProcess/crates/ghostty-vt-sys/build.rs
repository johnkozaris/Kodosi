use sha2::{Digest as _, Sha256};
use std::{
    env,
    fmt::Write as _,
    fs,
    io::Read as _,
    path::{Path, PathBuf},
    process::Command,
};

const GHOSTTY_LOCK: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../Ghostty.lock"
));
const EXPECTED_TARGET: &str = "aarch64-macos";
const EXPECTED_OPTIMIZE: &str = "ReleaseFast";

fn main() {
    println!("cargo:rerun-if-env-changed=KODOSI_GHOSTTY_DIR");
    println!("cargo:rerun-if-changed=../../../Ghostty.lock");
    let expected_package_commit = lock_value("package_commit");
    let expected_ghostty_commit = lock_value("upstream_commit");
    let expected_archive_digest = lock_value("combined_archive_sha256");
    let target_os = env::var("CARGO_CFG_TARGET_OS").expect("Cargo target OS");
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").expect("Cargo target architecture");
    assert_eq!(
        (target_os.as_str(), target_arch.as_str()),
        ("macos", "aarch64"),
        "Kodosi's vendored libghostty-vt artifact supports arm64 macOS only"
    );

    let root = env::var_os("KODOSI_GHOSTTY_DIR")
        .filter(|path| !path.is_empty())
        .map_or_else(default_ghostty_dir, PathBuf::from);
    watch_git_authority(&root);
    let provenance = root.join("Vendor/libghostty.version");
    let artifact_dir = root.join("Vendor/GhosttyVt/macos-arm64/lib");
    let archive = artifact_dir.join("libghostty-vt.a");
    require_package_commit(&root, expected_package_commit);

    println!("cargo:rerun-if-changed={}", provenance.display());
    println!("cargo:rerun-if-changed={}", archive.display());

    let metadata = fs::read_to_string(&provenance)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", provenance.display()));
    require_metadata(
        &metadata,
        "ghostty_commit",
        expected_ghostty_commit,
        &provenance,
    );
    require_metadata(&metadata, "vt_target", EXPECTED_TARGET, &provenance);
    require_metadata(&metadata, "vt_cpu", "baseline", &provenance);
    require_metadata(&metadata, "vt_optimize", EXPECTED_OPTIMIZE, &provenance);
    assert!(
        archive.is_file(),
        "missing vendored VT archive: {}",
        archive.display()
    );
    let metadata_digest = metadata_value(&metadata, "combined_archive_sha256", &provenance);
    assert_eq!(
        metadata_digest,
        expected_archive_digest,
        "{} must match combined_archive_sha256 in Ghostty.lock",
        provenance.display()
    );
    let actual_digest = sha256_file(&archive);
    assert_eq!(
        actual_digest,
        expected_archive_digest,
        "{} digest must match combined_archive_sha256 in Ghostty.lock",
        archive.display()
    );

    println!("cargo:rustc-link-search=native={}", artifact_dir.display());

    println!("cargo:rustc-link-lib=static:-bundle=ghostty-vt");
    println!("cargo:rustc-link-lib=c++");
}

fn watch_git_authority(root: &Path) {
    for git_path in ["HEAD", "packed-refs"] {
        if let Ok(output) = Command::new("git")
            .args([
                "-C",
                root.to_str().expect("UTF-8 Ghostty path"),
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                git_path,
            ])
            .output()
            && output.status.success()
            && let Ok(path) = std::str::from_utf8(&output.stdout)
        {
            println!("cargo:rerun-if-changed={}", path.trim());
        }
    }
    if let Ok(output) = Command::new("git")
        .args([
            "-C",
            root.to_str().expect("UTF-8 Ghostty path"),
            "symbolic-ref",
            "-q",
            "HEAD",
        ])
        .output()
        && output.status.success()
        && let Ok(reference) = std::str::from_utf8(&output.stdout)
        && let Ok(ref_path) = Command::new("git")
            .args([
                "-C",
                root.to_str().expect("UTF-8 Ghostty path"),
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                reference.trim(),
            ])
            .output()
        && ref_path.status.success()
        && let Ok(path) = std::str::from_utf8(&ref_path.stdout)
    {
        println!("cargo:rerun-if-changed={}", path.trim());
    }
}

fn default_ghostty_dir() -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..3 {
        root.pop();
    }
    root.join("../kodosi-ghostty")
}

fn lock_value(key: &str) -> &'static str {
    GHOSTTY_LOCK
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("Ghostty.lock must contain {key}"))
}

fn metadata_value<'a>(metadata: &'a str, key: &str, source: &Path) -> &'a str {
    metadata
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("{} must contain {key}", source.display()))
}

fn sha256_file(path: &Path) -> String {
    let mut file = fs::File::open(path)
        .unwrap_or_else(|error| panic!("failed to open {}: {error}", path.display()));
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let digest = hasher.finalize();
    digest.iter().fold(
        String::with_capacity(digest.len() * 2),
        |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to String cannot fail");
            output
        },
    )
}

fn require_package_commit(root: &Path, expected_package_commit: &str) {
    let output = Command::new("git")
        .args(["-C"])
        .arg(root)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap_or_else(|error| panic!("failed to inspect {} Git commit: {error}", root.display()));
    assert!(
        output.status.success(),
        "failed to inspect {} Git commit: {}",
        root.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = String::from_utf8(output.stdout)
        .expect("git rev-parse output must be UTF-8")
        .trim()
        .to_owned();
    assert_eq!(
        actual,
        expected_package_commit,
        "{} must be the reviewed kodosi-ghostty package commit",
        root.display()
    );
}

fn require_metadata(metadata: &str, key: &str, expected: &str, source: &Path) {
    let actual = metadata
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")));
    assert_eq!(
        actual,
        Some(expected),
        "{} must contain {key}={expected}",
        source.display()
    );
}

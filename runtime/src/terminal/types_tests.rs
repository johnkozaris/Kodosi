use super::TerminalMetadata;

fn in_folder(directory: &str) -> TerminalMetadata {
    TerminalMetadata {
        directory: Some(directory.to_owned()),
        ..TerminalMetadata::default()
    }
}

#[test]
fn folders_from_every_host_platform_are_valid() {
    for folder in [
        "/home/ada/project",
        r"C:\Users\ada\project",
        "C:/Users/ada",
        r"\\server\share\project",
    ] {
        assert!(in_folder(folder).is_valid(), "{folder}");
    }
}

#[test]
fn relative_or_malformed_folders_are_invalid() {
    for folder in [
        "",
        "project",
        "C:project",
        "C:",
        r"\\",
        r"1:\x",
        "/tmp/\u{7}",
    ] {
        assert!(!in_folder(folder).is_valid(), "{folder:?}");
    }
}

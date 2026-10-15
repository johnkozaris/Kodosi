use std::path::PathBuf;

use super::parse_cwd;

fn reported(raw: &str) -> Option<PathBuf> {
    parse_cwd(raw.as_bytes())
}

#[cfg(windows)]
#[test]
fn a_windows_host_takes_its_folders_from_osc_7_and_quoted_or_bare_osc_9_9() {
    for (raw, folder) in [
        (r"C:\Users\ada\project", r"C:\Users\ada\project"),
        (
            r#""C:\Users\ada\my project ✓""#,
            r"C:\Users\ada\my project ✓",
        ),
        (r#""\\server\share\project""#, r"\\server\share\project"),
        (
            "file://pc/C:/Users/ada/my%20project",
            r"C:\Users\ada\my project",
        ),
        ("file:///d:/", r"d:\"),
        ("file://server/share/x", r"\\server\share\x"),
    ] {
        assert_eq!(reported(raw), Some(PathBuf::from(folder)), "{raw}");
    }
}

#[cfg(unix)]
#[test]
fn a_unix_host_takes_its_folders_from_osc_7_and_quoted_or_bare_osc_9_9() {
    for (raw, folder) in [
        ("/home/ada/project", "/home/ada/project"),
        (r#""/home/ada/my project ✓""#, "/home/ada/my project ✓"),
        ("file://pc/home/ada/my%20project", "/home/ada/my project"),
    ] {
        assert_eq!(reported(raw), Some(PathBuf::from(folder)), "{raw}");
    }
}

#[test]
fn a_report_that_is_no_folder_of_this_host_leaves_the_folder_alone() {
    let foreign: &[&str] = if cfg!(windows) {
        &["/home/ada", "file://pc/C%3A%20Users%20ada"]
    } else {
        &[r"C:\Users\ada"]
    };
    for raw in [
        "",
        "\"\"",
        "project",
        r#""C:\Users\ada"#,
        "file://pc/home/ada\u{7}",
        "file://pc/home/%ZZ",
        "file://pc",
    ]
    .iter()
    .chain(foreign)
    {
        assert_eq!(reported(raw), None, "{raw:?}");
    }
    assert_eq!(reported(&format!("/{}", "a".repeat(4096))), None);
}

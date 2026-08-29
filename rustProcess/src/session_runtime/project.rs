use std::{
    collections::BTreeSet,
    fs,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

use reqwest::Url;

const DISCOVERY_REFRESH_INTERVAL: Duration = Duration::from_secs(45);

#[derive(Debug, Clone)]
pub(crate) struct ProjectDiscovery {
    pub(crate) project_type: Option<String>,
    pub(crate) package_manager: Option<String>,
    pub(crate) manifest_files: Vec<String>,
    pub(crate) git_url: Option<String>,
    pub(crate) git_branch: Option<String>,
    pub(crate) git_remotes: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct CachedProjectDiscovery {
    pub(crate) data: ProjectDiscovery,
    refreshed_at: Instant,
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "ManifestFacts is a short-lived capability bitmap for one-pass manifest analysis, so separate booleans keep the detector and precedence rules explicit."
)]
#[derive(Debug, Default, Clone, Copy)]
struct ManifestFacts {
    has_node: bool,
    has_rust: bool,
    has_python: bool,
    has_go: bool,
    has_swift: bool,
    has_dotnet: bool,
    uses_pnpm: bool,
    uses_npm: bool,
    uses_yarn: bool,
    uses_bun: bool,
    uses_uv: bool,
    uses_poetry: bool,
}

#[derive(Debug, Default)]
struct GitDiscovery {
    origin_url: Option<String>,
    branch: Option<String>,
    remotes: Vec<String>,
}

impl ManifestFacts {
    fn project_type(self) -> Option<String> {
        let mut kinds = Vec::new();
        if self.has_node {
            kinds.push("Node");
        }
        if self.has_rust {
            kinds.push("Rust");
        }
        if self.has_python {
            kinds.push("Python");
        }
        if self.has_go {
            kinds.push("Go");
        }
        if self.has_swift {
            kinds.push("Swift");
        }
        if self.has_dotnet {
            kinds.push(".NET");
        }

        (!kinds.is_empty()).then(|| kinds.join(" + "))
    }

    fn package_manager(self) -> Option<String> {
        if self.uses_pnpm {
            Some("pnpm".to_owned())
        } else if self.uses_npm {
            Some("npm".to_owned())
        } else if self.uses_yarn {
            Some("yarn".to_owned())
        } else if self.uses_bun {
            Some("bun".to_owned())
        } else if self.has_rust {
            Some("cargo".to_owned())
        } else if self.uses_uv {
            Some("uv".to_owned())
        } else if self.uses_poetry {
            Some("poetry".to_owned())
        } else if self.has_python {
            Some("python".to_owned())
        } else if self.has_go {
            Some("go".to_owned())
        } else if self.has_swift {
            Some("swiftpm".to_owned())
        } else if self.has_dotnet {
            Some("dotnet".to_owned())
        } else {
            None
        }
    }
}

impl CachedProjectDiscovery {
    pub(crate) fn new(data: ProjectDiscovery) -> Self {
        Self {
            data,
            refreshed_at: Instant::now(),
        }
    }

    pub(crate) fn is_stale(&self) -> bool {
        self.refreshed_at.elapsed() >= DISCOVERY_REFRESH_INTERVAL
    }

    pub(crate) fn refreshed_at(&self) -> Instant {
        self.refreshed_at
    }
}

pub(crate) fn discover(working_dir: &str) -> ProjectDiscovery {
    let path = Path::new(working_dir);
    let manifest_files = detect_manifest_files(path);
    let manifest_facts = analyze_manifest_files(&manifest_files);
    let git = discover_git(working_dir);
    let git_url = git.origin_url.as_deref().and_then(safe_git_url);

    ProjectDiscovery {
        project_type: manifest_facts.project_type(),
        package_manager: manifest_facts.package_manager(),
        manifest_files,
        git_url,
        git_branch: git.branch,
        git_remotes: git.remotes,
    }
}

fn detect_manifest_files(path: &Path) -> Vec<String> {
    let mut manifests = Vec::new();
    for candidate in [
        "package.json",
        "pnpm-lock.yaml",
        "package-lock.json",
        "yarn.lock",
        "bun.lock",
        "bun.lockb",
        "Cargo.toml",
        "pyproject.toml",
        "uv.lock",
        "poetry.lock",
        "go.mod",
        "Package.swift",
    ] {
        if path.join(candidate).is_file() {
            manifests.push(candidate.to_owned());
        }
    }

    if let Ok(entries) = fs::read_dir(path) {
        let mut dotnet_manifests = entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| has_extension(name, "sln") || has_extension(name, "csproj"))
            .collect::<Vec<_>>();
        dotnet_manifests.sort();
        manifests.extend(dotnet_manifests);
    }

    manifests
}

fn analyze_manifest_files(manifest_files: &[String]) -> ManifestFacts {
    let mut facts = ManifestFacts::default();
    for file in manifest_files {
        match file.as_str() {
            "package.json" => facts.has_node = true,
            "Cargo.toml" => facts.has_rust = true,
            "pyproject.toml" => facts.has_python = true,
            "go.mod" => facts.has_go = true,
            "Package.swift" => facts.has_swift = true,
            "pnpm-lock.yaml" => facts.uses_pnpm = true,
            "package-lock.json" => facts.uses_npm = true,
            "yarn.lock" => facts.uses_yarn = true,
            "bun.lock" | "bun.lockb" => facts.uses_bun = true,
            "uv.lock" => facts.uses_uv = true,
            "poetry.lock" => facts.uses_poetry = true,
            _ if has_extension(file, "sln") || has_extension(file, "csproj") => {
                facts.has_dotnet = true;
            }
            _ => {}
        }
    }

    facts
}

fn discover_git(working_dir: &str) -> GitDiscovery {
    let branch = git_branch(working_dir);
    let (origin_url, remotes) = git_remote_details(working_dir);
    GitDiscovery {
        origin_url,
        branch,
        remotes,
    }
}

fn git_branch(working_dir: &str) -> Option<String> {
    git_output(working_dir, &["rev-parse", "--abbrev-ref", "HEAD"]).filter(|value| value != "HEAD")
}

fn git_remote_details(working_dir: &str) -> (Option<String>, Vec<String>) {
    git_output(working_dir, &["remote", "-v"])
        .map(|value| parse_git_remote_verbose(&value))
        .unwrap_or_default()
}

fn git_output(working_dir: &str, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(working_dir)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8(output.stdout).ok()?;
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn parse_git_remote_verbose(output: &str) -> (Option<String>, Vec<String>) {
    let mut remote_names = BTreeSet::new();
    let mut origin_fetch_url = None;
    let mut origin_fallback_url = None;

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let mut parts = line.split_whitespace();
        let Some(name) = parts.next() else {
            continue;
        };
        let Some(url) = parts.next() else {
            continue;
        };
        let direction = parts.next();
        remote_names.insert(name.to_owned());
        if name == "origin" {
            if direction == Some("(fetch)") {
                origin_fetch_url = Some(url.to_owned());
            } else {
                origin_fallback_url.get_or_insert_with(|| url.to_owned());
            }
        }
    }

    (
        origin_fetch_url.or(origin_fallback_url),
        remote_names.into_iter().collect(),
    )
}

fn safe_git_url(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return None;
    }

    let candidate = scp_remote_to_ssh_url(trimmed).unwrap_or_else(|| trimmed.to_owned());
    let mut parsed = Url::parse(&candidate).ok()?;
    if !matches!(parsed.scheme(), "http" | "https" | "ssh") || parsed.host_str().is_none() {
        return None;
    }

    parsed.set_username("").ok()?;
    parsed.set_password(None).ok()?;
    parsed.set_query(None);
    parsed.set_fragment(None);
    let path = parsed.path().trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path).to_owned();
    parsed.set_path(&path);
    Some(parsed.to_string())
}

fn scp_remote_to_ssh_url(value: &str) -> Option<String> {
    if value.contains("://") {
        return None;
    }
    let (authority, path) = value.split_once(':')?;
    if !authority.contains('@') {
        return None;
    }
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if host.is_empty()
        || path.is_empty()
        || host.contains(['/', '\\'])
        || path.starts_with('/')
        || path.starts_with('\\')
    {
        return None;
    }
    Some(format!("ssh://{host}/{path}"))
}

fn has_extension(path: &str, extension: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
}

#[cfg(test)]
mod tests {
    use super::{analyze_manifest_files, parse_git_remote_verbose, safe_git_url};

    #[test]
    fn analyze_manifest_files_tracks_project_types_and_package_manager_precedence() {
        let facts = analyze_manifest_files(&[
            "package.json".to_owned(),
            "pnpm-lock.yaml".to_owned(),
            "Cargo.toml".to_owned(),
            "kodosi.csproj".to_owned(),
        ]);

        assert_eq!(facts.project_type(), Some("Node + Rust + .NET".to_owned()));
        assert_eq!(facts.package_manager(), Some("pnpm".to_owned()));
    }

    #[test]
    fn analyze_manifest_files_falls_back_to_language_when_lockfile_is_missing() {
        let facts = analyze_manifest_files(&["pyproject.toml".to_owned()]);

        assert_eq!(facts.project_type(), Some("Python".to_owned()));
        assert_eq!(facts.package_manager(), Some("python".to_owned()));
    }

    #[test]
    fn safe_git_url_canonicalizes_supported_network_remotes() {
        assert_eq!(
            safe_git_url("https://github.com/kodosi/app.git"),
            Some("https://github.com/kodosi/app".to_owned())
        );
        assert_eq!(
            safe_git_url("http://example.com/team/repo/"),
            Some("http://example.com/team/repo".to_owned())
        );
        assert_eq!(
            safe_git_url("git@github.com:kodosi/app.git"),
            Some("ssh://github.com/kodosi/app".to_owned())
        );
        assert_eq!(
            safe_git_url("ssh://git@github.com/kodosi/app.git"),
            Some("ssh://github.com/kodosi/app".to_owned())
        );
    }

    #[test]
    fn safe_git_url_removes_credentials_query_and_fragment() {
        assert_eq!(
            safe_git_url("https://user:secret@example.com/team/repo.git?token=hidden#fragment"),
            Some("https://example.com/team/repo".to_owned())
        );
        assert_eq!(
            safe_git_url("ssh://deploy:secret@example.com/team/repo.git?identity=hidden#fragment"),
            Some("ssh://example.com/team/repo".to_owned())
        );
        assert_eq!(
            safe_git_url("alice@example.com:team/repo.git"),
            Some("ssh://example.com/team/repo".to_owned())
        );
    }

    #[test]
    fn safe_git_url_rejects_local_and_unsafe_remotes() {
        for value in [
            "javascript:alert(1)",
            "file:///tmp/kodosi",
            "mailto:test@example.com",
            "/tmp/kodosi",
            "../kodosi",
            "host:/absolute/path",
            "host:\\absolute\\path",
            "https://example.com/repo\nsecret",
        ] {
            assert_eq!(safe_git_url(value), None, "{value}");
        }
    }

    #[test]
    fn parse_git_remote_verbose_extracts_origin_fetch_and_unique_names() {
        let (origin, remotes) = parse_git_remote_verbose(
            "origin\tgit@github.com:kodosi/app.git (fetch)\n\
             origin\tgit@github.com:kodosi/app.git (push)\n\
             upstream\thttps://github.com/example/upstream.git (fetch)\n",
        );

        assert_eq!(origin, Some("git@github.com:kodosi/app.git".to_owned()));
        assert_eq!(remotes, vec!["origin".to_owned(), "upstream".to_owned()]);
    }
}

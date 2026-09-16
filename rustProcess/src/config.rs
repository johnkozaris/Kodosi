use crate::{Error, Result};
use reqwest::Url;
use std::{
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct Config {
    pub data_root: PathBuf,
    pub home: PathBuf,
    pub initial_shell: Option<String>,
    pub backend_url: Url,
    pub oidc_issuer: String,
    pub oidc_client_id: String,
    pub oidc_scopes: Vec<String>,
    pub secret_service: String,
    pub isolated: bool,
}

impl Config {
    pub fn load() -> Result<Self> {
        let dirs = directories::BaseDirs::new().ok_or_else(|| {
            Error::Invalid("cannot determine the user's home directory".to_owned())
        })?;
        let home =
            std::env::var_os("HOME").map_or_else(|| dirs.home_dir().to_path_buf(), PathBuf::from);
        let production = std::env::var_os("KODOSI_PRODUCTION_DATA_ROOT")
            .map_or_else(|| dirs.config_dir().join("kodosi"), PathBuf::from);
        let (data_root, isolated) = match std::env::var_os("KODOSI_DATA_ROOT") {
            Some(value) => {
                if std::env::var_os("KODOSI_PRODUCTION_DATA_ROOT").is_none() {
                    return Err(Error::Invalid(
                        "KODOSI_PRODUCTION_DATA_ROOT is required with an isolated root".to_owned(),
                    ));
                }
                let root = PathBuf::from(value);
                validate_isolated(&root, &production, &home)?;
                (root.join("core"), true)
            }
            None => (production, false),
        };
        let backend = std::env::var("KODOSI__BACKEND__API")
            .unwrap_or_else(|_| "https://api.kodosi.com".to_owned());
        let backend_url =
            Url::parse(&backend).map_err(|error| Error::Invalid(error.to_string()))?;
        crate::identity::oidc::validate_url(&backend_url)?;
        if backend_url.query().is_some() {
            return Err(Error::Invalid(
                "backend URL must not contain a query".to_owned(),
            ));
        }
        let oidc_issuer = std::env::var("KODOSI__AUTH__ISSUER")
            .unwrap_or_else(|_| "https://auth.kodosi.com/realms/kodosi".to_owned());
        let issuer = Url::parse(&oidc_issuer).map_err(|error| Error::Invalid(error.to_string()))?;
        crate::identity::oidc::validate_url(&issuer)?;
        if issuer.query().is_some() {
            return Err(Error::Invalid(
                "OIDC issuer must not contain a query".into(),
            ));
        }
        secure_directory(&data_root)?;
        Ok(Self {
            data_root,
            home,
            isolated,
            backend_url,
            initial_shell: std::env::var("KODOSI__RUNTIME__INITIAL_SHELL")
                .ok()
                .filter(|value| !value.is_empty()),
            oidc_issuer,
            oidc_client_id: std::env::var("KODOSI__AUTH__CLIENT_ID")
                .unwrap_or_else(|_| "kodosi-app".to_owned()),
            oidc_scopes: std::env::var("KODOSI__AUTH__SCOPE")
                .unwrap_or_else(|_| "openid profile offline_access".to_owned())
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            secret_service: std::env::var("KODOSI__AUTH__KEYRING_SERVICE")
                .unwrap_or_else(|_| "com.kodosi.local".to_owned()),
        })
    }

    pub fn isolated(root: &Path) -> Result<Self> {
        secure_directory(root)?;
        let home = root.join("home");
        secure_directory(&home)?;
        let data_root = root.join("core");
        secure_directory(&data_root)?;
        Ok(Self {
            data_root,
            home,
            initial_shell: None,
            backend_url: Url::parse("http://127.0.0.1:1")
                .map_err(|error| Error::Invalid(error.to_string()))?,
            oidc_issuer: "http://127.0.0.1:1".to_owned(),
            oidc_client_id: "kodosi-test".to_owned(),
            oidc_scopes: vec!["openid".to_owned()],
            secret_service: "kodosi.isolated".to_owned(),
            isolated: true,
        })
    }
}

pub(crate) fn secure_directory(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(Error::Invalid(
            "storage path must be absolute without dot components".to_owned(),
        ));
    }
    let mut prefix = PathBuf::new();
    for component in path.components() {
        prefix.push(component.as_os_str());
        match std::fs::symlink_metadata(&prefix) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(Error::Invalid(format!(
                    "storage path contains a symlink: {}",
                    prefix.display()
                )));
            }
            Ok(meta) if !meta.is_dir() => {
                return Err(Error::Invalid(format!(
                    "storage path is not a directory: {}",
                    prefix.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match std::fs::DirBuilder::new().mode(0o700).create(&prefix) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        let metadata = std::fs::symlink_metadata(&prefix)?;
                        if !metadata.is_dir()
                            || metadata.file_type().is_symlink()
                            || metadata.uid() != rustix::process::geteuid().as_raw()
                        {
                            return Err(Error::Invalid(
                                "storage directory changed during creation".to_owned(),
                            ));
                        }
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    let meta = std::fs::metadata(path)?;
    if meta.uid() != rustix::process::geteuid().as_raw() {
        return Err(Error::Invalid(
            "runtime storage belongs to another user".to_owned(),
        ));
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn validate_isolated(root: &Path, production: &Path, home: &Path) -> Result<()> {
    if !root.is_absolute()
        || !production.is_absolute()
        || !home.is_absolute()
        || root.parent().is_none()
        || root == home
        || root.starts_with(production)
        || production.starts_with(root)
    {
        return Err(Error::Invalid(
            "isolated data root must not overlap home or production storage".to_owned(),
        ));
    }
    if root.to_string_lossy().contains(['$', '\n', '\r', '\0'])
        || root
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(Error::Invalid("invalid isolated data root".to_owned()));
    }
    let root_resolved = resolve_existing(root)?;
    let production_resolved = resolve_existing(production)?;
    let home_resolved = resolve_existing(home)?;
    if root_resolved == home_resolved
        || root_resolved.starts_with(&production_resolved)
        || production_resolved.starts_with(&root_resolved)
    {
        return Err(Error::Invalid(
            "isolated root aliases home or production storage".to_owned(),
        ));
    }
    Ok(())
}

fn resolve_existing(path: &Path) -> Result<PathBuf> {
    let mut prefix = path;
    let mut missing = Vec::new();
    loop {
        match prefix.canonicalize() {
            Ok(mut resolved) => {
                for component in missing.into_iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = prefix.file_name().ok_or_else(|| {
                    Error::Invalid("storage path has no existing ancestor".to_owned())
                })?;
                missing.push(name.to_os_string());
                prefix = prefix.parent().ok_or_else(|| {
                    Error::Invalid("storage path has no existing ancestor".to_owned())
                })?;
            }
            Err(error) => return Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn isolated_paths_reject_aliases_of_production_ancestors() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().canonicalize().unwrap();
        let home = real.join("home");
        std::fs::create_dir(&home).unwrap();
        let alias = real.join("alias");
        std::os::unix::fs::symlink(&home, &alias).unwrap();
        let production = alias.join("kodosi");
        assert!(validate_isolated(&home, &production, &real.join("unrelated-home")).is_err());
        assert!(validate_isolated(&home.join("kodosi/test"), &production, &home).is_err());
    }

    #[test]
    fn secure_directory_refuses_symlinks_without_modifying_the_target() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().canonicalize().unwrap();
        let target = real.join("target");
        std::fs::create_dir(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o750)).unwrap();
        let link = real.join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(secure_directory(&link.join("core")).is_err());
        assert!(!target.join("core").exists());
        assert_eq!(std::fs::metadata(target).unwrap().mode() & 0o777, 0o750);
    }

    #[test]
    fn concurrent_bootstrap_creates_private_directories() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().canonicalize().unwrap();
        let barrier = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    let mut failure = None;
                    for index in 0..32 {
                        barrier.wait();
                        if let Err(error) =
                            secure_directory(&real.join(format!("race-{index}/nested/core")))
                        {
                            failure = Some(error);
                        }
                    }
                    assert!(failure.is_none(), "{failure:?}");
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
                let metadata = std::fs::symlink_metadata(path).unwrap();
                assert!(metadata.is_dir());
                assert_eq!(metadata.uid(), rustix::process::geteuid().as_raw());
                assert_eq!(metadata.mode() & 0o777, 0o700);
            }
        }
    }

    #[test]
    fn isolated_paths_reject_production_and_ancestors() {
        let home = Path::new("/home/person");
        let production = home.join(".config/kodosi");
        for candidate in [&production, home, Path::new("/"), Path::new("relative")] {
            assert!(validate_isolated(candidate, &production, home).is_err());
        }
        assert!(validate_isolated(Path::new("/tmp/kodosi-test"), &production, home).is_ok());
    }
}

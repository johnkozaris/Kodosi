use crate::{Error, Result};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostKind {
    App,
    Foreground,
    Background,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub data_root: PathBuf,
    /// The native client's preferences file in an isolated run. Other runs
    /// keep client preferences where the platform keeps them.
    pub preferences: Option<PathBuf>,
    pub home: PathBuf,
    pub initial_shell: Option<String>,
    pub backend_url: Url,
    pub oidc_issuer: String,
    pub oidc_client_id: String,
    pub oidc_scopes: Vec<String>,
    pub secret_service: String,
    pub isolated: bool,
    pub host: HostKind,
}

impl Config {
    pub fn load() -> Result<Self> {
        let home = kodosi_os::fs::home_dir()?;
        let production = match std::env::var_os("KODOSI_PRODUCTION_DATA_ROOT") {
            Some(root) => PathBuf::from(root),
            None => kodosi_os::fs::default_data_root()?,
        };
        let (data_root, preferences, isolated) = match std::env::var_os("KODOSI_DATA_ROOT") {
            Some(value) => {
                if std::env::var_os("KODOSI_PRODUCTION_DATA_ROOT").is_none() {
                    return Err(Error::Invalid(
                        "KODOSI_PRODUCTION_DATA_ROOT is required with an isolated root".to_owned(),
                    ));
                }
                let root = PathBuf::from(value);
                validate_isolated(&root, &production, &home)?;
                secure_directory(&root)?;
                (root.join("core"), Some(root.join("preferences.ini")), true)
            }
            None => (production, None, false),
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
            preferences,
            home,
            isolated,
            backend_url,
            initial_shell: std::env::var("KODOSI__RUNTIME__INITIAL_SHELL")
                .ok()
                .filter(|value| !value.is_empty()),
            oidc_issuer,
            oidc_client_id: "kodosi-app".to_owned(),
            oidc_scopes: ["openid", "profile", "offline_access"]
                .map(str::to_owned)
                .to_vec(),
            secret_service: "com.kodosi.local".to_owned(),
            host: HostKind::App,
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
            preferences: Some(root.join("preferences.ini")),
            home,
            initial_shell: None,
            backend_url: Url::parse("http://127.0.0.1:1")
                .map_err(|error| Error::Invalid(error.to_string()))?,
            oidc_issuer: "http://127.0.0.1:1".to_owned(),
            oidc_client_id: "kodosi-test".to_owned(),
            oidc_scopes: vec!["openid".to_owned()],
            secret_service: "kodosi.isolated".to_owned(),
            isolated: true,
            host: HostKind::App,
        })
    }
}

pub(crate) fn secure_directory(path: &Path) -> Result<()> {
    kodosi_os::fs::create_private_dir(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::InvalidInput | std::io::ErrorKind::PermissionDenied => {
            Error::Invalid(error.to_string())
        }
        _ => error.into(),
    })
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

#[cfg(all(test, unix))]
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
    fn isolated_paths_reject_production_and_ancestors() {
        let home = Path::new("/home/person");
        let production = home.join(".config/kodosi");
        for candidate in [&production, home, Path::new("/"), Path::new("relative")] {
            assert!(validate_isolated(candidate, &production, home).is_err());
        }
        assert!(validate_isolated(Path::new("/tmp/kodosi-test"), &production, home).is_ok());
    }
}

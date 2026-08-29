use std::path::Path;
#[cfg(feature = "cli")]
use std::path::PathBuf;
#[cfg(feature = "cli")]
use std::sync::OnceLock;

#[cfg(feature = "cli")]
use aws_lc_rs::digest;
use serde::Deserialize;

use crate::{AppError, Result};

const DEFAULT_DECISION_TIMEOUT_SECS: u64 = 120;
const MAX_DECISION_TIMEOUT_SECS: u64 = 540;
const _: () = assert!(DEFAULT_DECISION_TIMEOUT_SECS <= MAX_DECISION_TIMEOUT_SECS);

const EMBEDDED_DEFAULT_CONFIG: &str = include_str!("../kodosi.toml");
#[cfg(feature = "cli")]
static CLI_CONFIG_PATH: OnceLock<PathBuf> = OnceLock::new();

#[cfg(feature = "cli")]
pub(crate) fn set_cli_config_path(path: PathBuf) -> Result<()> {
    CLI_CONFIG_PATH
        .set(path)
        .map_err(|_| AppError::Unsupported {
            reason: "the CLI configuration path was initialized more than once".to_owned(),
        })
}

#[cfg(feature = "cli")]
pub(crate) fn cli_config_path() -> Option<&'static Path> {
    CLI_CONFIG_PATH.get().map(PathBuf::as_path)
}

#[cfg(feature = "cli")]
pub(crate) fn configuration_identity() -> Result<String> {
    let mut context = digest::Context::new(&digest::SHA256);
    update_identity_field(
        &mut context,
        b"embedded",
        EMBEDDED_DEFAULT_CONFIG.as_bytes(),
    )?;
    match cli_config_path() {
        Some(path) => {
            let canonical = path.canonicalize().map_err(AppError::Io)?;
            let bytes = std::fs::read(&canonical).map_err(AppError::Io)?;
            update_identity_field(
                &mut context,
                b"path",
                canonical.to_string_lossy().as_bytes(),
            )?;
            update_identity_field(&mut context, b"file", &bytes)?;
        }
        None => update_identity_field(&mut context, b"path", b"")?,
    }
    let mut overrides = std::env::vars_os()
        .filter_map(|(key, value)| {
            let key = key.to_string_lossy();
            key.to_ascii_uppercase()
                .starts_with("KODOSI__")
                .then(|| (key.into_owned(), value.to_string_lossy().into_owned()))
        })
        .collect::<Vec<_>>();
    overrides.sort_unstable();
    for (key, value) in overrides {
        update_identity_field(&mut context, key.as_bytes(), value.as_bytes())?;
    }
    Ok(hex_digest(context.finish().as_ref()))
}

#[cfg(feature = "cli")]
fn update_identity_field(context: &mut digest::Context, label: &[u8], value: &[u8]) -> Result<()> {
    let label_len = u32::try_from(label.len()).map_err(|_| AppError::Unsupported {
        reason: "configuration identity label is too large".to_owned(),
    })?;
    let value_len = u64::try_from(value.len()).map_err(|_| AppError::Unsupported {
        reason: "configuration identity value is too large".to_owned(),
    })?;
    context.update(&label_len.to_be_bytes());
    context.update(label);
    context.update(&value_len.to_be_bytes());
    context.update(value);
    Ok(())
}

#[cfg(feature = "cli")]
fn hex_digest(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut output, byte| {
            let _ = write!(output, "{byte:02x}");
            output
        },
    )
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct AppConfig {
    pub(crate) log_filter: String,
    pub(crate) runtime: RuntimeConfig,
    pub(crate) backend: BackendConfig,
    pub(crate) auth: AuthConfig,
    pub(crate) permissions: PermissionsConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct RuntimeConfig {
    pub(crate) tick_interval_ms: u64,
    pub(crate) session_prefix: String,
    pub(crate) initial_shell: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct BackendConfig {
    pub(crate) api: Option<String>,
    pub(crate) host_relay: Option<String>,
    pub(crate) viewer_relay: Option<String>,
    pub(crate) user_events: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct AuthConfig {
    pub(crate) issuer: Option<String>,
    pub(crate) client_id: String,
    pub(crate) scope: String,
    pub(crate) audience: Option<String>,
    pub(crate) keyring_service: String,
    pub(crate) refresh_skew_minutes: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct PermissionsConfig {
    pub(crate) decision_timeout_secs: u64,
    pub(crate) allow_terminal_clipboard_write: bool,
    #[serde(rename = "on_timeout", default)]
    _legacy_on_timeout: Option<LegacyFailClosedTimeout>,
}

impl Default for PermissionsConfig {
    fn default() -> Self {
        Self {
            decision_timeout_secs: DEFAULT_DECISION_TIMEOUT_SECS,
            allow_terminal_clipboard_write: false,
            _legacy_on_timeout: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct LegacyFailClosedTimeout;

impl<'de> Deserialize<'de> for LegacyFailClosedTimeout {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value == "fail_closed" {
            Ok(Self)
        } else {
            Err(serde::de::Error::custom(
                "legacy permissions.on_timeout must be \"fail_closed\"",
            ))
        }
    }
}

impl PermissionsConfig {
    pub(crate) const fn decision_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.decision_timeout_secs)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AppStateConfig {
    pub(crate) runtime: RuntimeConfig,
    pub(crate) auth_refresh_skew_minutes: i64,
    pub(crate) permissions: PermissionsConfig,
}

impl AppConfig {
    pub(crate) fn load(path: Option<&Path>) -> Result<Self> {
        let path = path.map(Path::to_path_buf);
        #[cfg(feature = "cli")]
        let path = path.or_else(|| CLI_CONFIG_PATH.get().cloned());
        let mut builder = config::Config::builder().add_source(config::File::from_str(
            EMBEDDED_DEFAULT_CONFIG,
            config::FileFormat::Toml,
        ));

        if let Some(path) = path.as_deref() {
            builder = builder.add_source(config::File::from(path.to_path_buf()).required(true));
        }

        builder = builder.add_source(config::Environment::with_prefix("kodosi").separator("__"));

        let settings = builder.build().map_err(AppError::ConfigSource)?;
        let config: Self = settings
            .try_deserialize()
            .map_err(AppError::ConfigDeserialize)?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.runtime.tick_interval_ms == 0 {
            return Err(AppError::ConfigDeserialize(config::ConfigError::Message(
                "runtime.tick_interval_ms must be > 0".to_owned(),
            )));
        }
        if self.auth.refresh_skew_minutes < 0 {
            return Err(AppError::ConfigDeserialize(config::ConfigError::Message(
                "auth.refresh_skew_minutes must not be negative".to_owned(),
            )));
        }
        if !(1..=MAX_DECISION_TIMEOUT_SECS).contains(&self.permissions.decision_timeout_secs) {
            return Err(AppError::ConfigDeserialize(config::ConfigError::Message(
                format!(
                    "permissions.decision_timeout_secs must be in [1, {MAX_DECISION_TIMEOUT_SECS}]"
                ),
            )));
        }
        Ok(())
    }

    pub(crate) fn app_state_config(&self) -> AppStateConfig {
        AppStateConfig {
            runtime: self.runtime.clone(),
            auth_refresh_skew_minutes: self.auth.refresh_skew_minutes,
            permissions: self.permissions.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs, path::PathBuf};

    use super::{AppConfig, EMBEDDED_DEFAULT_CONFIG};
    use crate::AppError;

    #[test]
    fn load_without_path_uses_embedded_defaults() {
        let loaded = AppConfig::load(None).expect("load embedded defaults");

        assert_eq!(loaded.runtime.tick_interval_ms, 120);
        assert_eq!(loaded.runtime.session_prefix, "kodosi");
        assert_eq!(
            loaded.backend.api.as_deref(),
            Some("https://api.kodosi.com")
        );
        assert_eq!(
            loaded.backend.host_relay.as_deref(),
            Some("wss://api.kodosi.com")
        );
        assert_eq!(loaded.auth.client_id, "kodosi-app");
    }

    #[test]
    fn explicit_path_overrides_embedded_defaults() {
        let temp_root = unique_temp_dir("explicit-override");
        let override_path = temp_root.join("override.toml");
        fs::write(
            &override_path,
            "[backend]\napi = \"https://api.example.test\"\n",
        )
        .expect("write override file");

        let loaded = AppConfig::load(Some(&override_path)).expect("load with override");

        assert_eq!(
            loaded.backend.api.as_deref(),
            Some("https://api.example.test")
        );
        assert_eq!(loaded.runtime.tick_interval_ms, 120);
        fs::remove_dir_all(&temp_root).expect("remove temp dir");
    }

    fn load_with_environment(
        path: Option<&std::path::Path>,
        environment: HashMap<String, String>,
    ) -> crate::Result<AppConfig> {
        let mut builder = config::Config::builder().add_source(config::File::from_str(
            super::EMBEDDED_DEFAULT_CONFIG,
            config::FileFormat::Toml,
        ));
        if let Some(path) = path {
            builder = builder.add_source(config::File::from(path.to_path_buf()).required(true));
        }
        builder = builder.add_source(
            config::Environment::with_prefix("kodosi")
                .separator("__")
                .source(Some(environment)),
        );
        let settings = builder.build().map_err(AppError::ConfigSource)?;
        let config: AppConfig = settings
            .try_deserialize()
            .map_err(AppError::ConfigDeserialize)?;
        config.validate()?;
        Ok(config)
    }

    #[test]
    fn environment_overrides_embedded_and_optional_toml_file() {
        let temp_root = unique_temp_dir("environment-overlay");
        let override_path = temp_root.join("override.toml");
        fs::write(
            &override_path,
            "[backend]\napi = \"https://file.example.test\"\n",
        )
        .expect("write override file");
        let environment = HashMap::from([(
            "KODOSI__BACKEND__API".to_owned(),
            "https://env.example.test".to_owned(),
        )]);

        let loaded = load_with_environment(Some(&override_path), environment)
            .expect("load TOML and environment overlays");

        assert_eq!(
            loaded.backend.api.as_deref(),
            Some("https://env.example.test")
        );
        assert_eq!(loaded.runtime.tick_interval_ms, 120);
        fs::remove_dir_all(&temp_root).expect("remove temp dir");
    }

    #[test]
    fn unknown_override_key_is_rejected() {
        let temp_root = unique_temp_dir("unknown-key");
        let override_path = temp_root.join("override.toml");
        fs::write(
            &override_path,
            "[backend]\napii = \"https://typo.example.test\"\n",
        )
        .expect("write override file");

        assert!(AppConfig::load(Some(&override_path)).is_err());
        fs::remove_dir_all(&temp_root).expect("remove temp dir");
    }

    #[test]
    fn removed_configuration_surfaces_are_rejected() {
        for contents in [
            "app_name = \"legacy\"\n",
            "[runtime.terminal_history]\nmax_lines = 1024\n",
        ] {
            let temp_root = unique_temp_dir("removed-surface");
            let override_path = temp_root.join("override.toml");
            fs::write(&override_path, contents).expect("write override file");
            assert!(AppConfig::load(Some(&override_path)).is_err());
            fs::remove_dir_all(&temp_root).expect("remove temp dir");
        }
    }

    #[test]
    fn permissions_default_timeout_is_120s() {
        let loaded = AppConfig::load(None).expect("load embedded defaults");
        assert_eq!(loaded.permissions.decision_timeout_secs, 120);
        assert!(!loaded.permissions.allow_terminal_clipboard_write);
        let constructed = AppConfig::default();
        assert_eq!(constructed.permissions.decision_timeout_secs, 120);
        assert!(!constructed.permissions.allow_terminal_clipboard_write);
        assert!(
            EMBEDDED_DEFAULT_CONFIG
                .lines()
                .all(|line| !line.trim_start().starts_with("on_timeout"))
        );
    }

    #[test]
    fn legacy_fail_closed_timeout_policy_is_accepted_and_ignored() {
        let temp_root = unique_temp_dir("permissions-legacy-fail-closed");
        let override_path = temp_root.join("override.toml");
        fs::write(
            &override_path,
            "[permissions]\ndecision_timeout_secs = 30\non_timeout = \"fail_closed\"\n",
        )
        .expect("write override file");

        let loaded = AppConfig::load(Some(&override_path)).expect("legacy fail_closed config");
        assert_eq!(loaded.permissions.decision_timeout_secs, 30);
        fs::remove_dir_all(&temp_root).expect("remove temp dir");
    }

    #[test]
    fn legacy_non_fail_closed_timeout_policy_is_rejected() {
        for value in ["fail_open", "unknown"] {
            let temp_root = unique_temp_dir(value);
            let override_path = temp_root.join("override.toml");
            fs::write(
                &override_path,
                format!("[permissions]\ndecision_timeout_secs = 30\non_timeout = \"{value}\"\n"),
            )
            .expect("write override file");

            assert!(
                AppConfig::load(Some(&override_path)).is_err(),
                "{value} must remain rejected"
            );
            fs::remove_dir_all(&temp_root).expect("remove temp dir");
        }
    }

    #[test]
    fn permissions_timeout_out_of_range_is_rejected() {
        let temp_root = unique_temp_dir("permissions-bad");
        let override_path = temp_root.join("override.toml");
        fs::write(&override_path, "[permissions]\ndecision_timeout_secs = 0\n")
            .expect("write override file");
        assert!(
            AppConfig::load(Some(&override_path)).is_err(),
            "zero decision timeout must fail validation"
        );
        fs::remove_dir_all(&temp_root).expect("remove temp dir");
    }

    #[test]
    fn permissions_timeout_is_bounded() {
        let temp_root = unique_temp_dir("permissions-copilot-margin");
        let override_path = temp_root.join("override.toml");
        fs::write(
            &override_path,
            "[permissions]\ndecision_timeout_secs = 600\n",
        )
        .expect("write override file");
        assert!(AppConfig::load(Some(&override_path)).is_err());
        fs::remove_dir_all(&temp_root).expect("remove temp dir");
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("kodosi-config-{label}-{}", uuid::Uuid::now_v7()));
        fs::create_dir_all(&dir).expect("create temp test directory");
        dir
    }
}

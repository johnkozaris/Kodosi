use std::{fmt, fs, path::PathBuf};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use subtle::ConstantTimeEq as _;
use time::OffsetDateTime;
use zeroize::Zeroizing;

use crate::{
    Result,
    identity_core::platform_secrets::{PlatformSecretLoadResult, PlatformSecretStoreResult},
    support::{
        platform::fs as support_fs,
        storage::atomic_file::{FileMode, atomic_write_json},
    },
};

#[derive(Clone)]
pub(crate) struct StoredTokens {
    pub(crate) access_token: Zeroizing<String>,
    pub(crate) refresh_token: Option<Zeroizing<String>>,
    pub(crate) expires_at: OffsetDateTime,
    pub(crate) backend_origin: Option<String>,
    pub(crate) backend_user_id: Option<String>,
}

impl StoredTokens {
    #[cfg(any(test, feature = "cli"))]
    fn backend_account(&self) -> Option<StoredBackendAccount> {
        Some(StoredBackendAccount {
            backend_origin: self.backend_origin.clone()?,
            backend_user_id: self.backend_user_id.clone()?,
        })
    }

    fn has_backend_account(&self) -> bool {
        self.backend_origin.is_some() && self.backend_user_id.is_some()
    }

    pub(crate) fn new(
        access_token: String,
        refresh_token: Option<String>,
        expires_at: OffsetDateTime,
    ) -> Self {
        Self {
            access_token: Zeroizing::new(access_token),
            refresh_token: refresh_token.map(Zeroizing::new),
            expires_at,
            backend_origin: None,
            backend_user_id: None,
        }
    }

    pub(crate) fn bind_backend_account(
        mut self,
        backend_origin: String,
        backend_user_id: String,
    ) -> Self {
        self.backend_origin = Some(backend_origin);
        self.backend_user_id = Some(backend_user_id);
        self
    }
}

impl fmt::Debug for StoredTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoredTokens")
            .field("access_token", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field("expires_at", &self.expires_at)
            .field("backend_origin", &self.backend_origin)
            .field("backend_user_id", &self.backend_user_id)
            .finish()
    }
}

#[derive(Serialize, Deserialize)]
struct StoredTokensWire {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_at: OffsetDateTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    backend_origin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    backend_user_id: Option<String>,
}

impl Serialize for StoredTokens {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        StoredTokensWire {
            access_token: (*self.access_token).clone(),
            refresh_token: self.refresh_token.as_ref().map(|t| (**t).clone()),
            expires_at: self.expires_at,
            backend_origin: self.backend_origin.clone(),
            backend_user_id: self.backend_user_id.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StoredTokens {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let wire = StoredTokensWire::deserialize(deserializer)?;
        let mut tokens = Self::new(wire.access_token, wire.refresh_token, wire.expires_at);
        tokens.backend_origin = wire.backend_origin;
        tokens.backend_user_id = wire.backend_user_id;
        Ok(tokens)
    }
}

#[cfg(any(test, feature = "cli"))]
pub(crate) struct StoredBackendAccount {
    pub(crate) backend_origin: String,
    pub(crate) backend_user_id: String,
}

pub(crate) trait TokenStore {
    fn load(&self, subject: &str) -> Result<Option<StoredTokens>>;
    fn save(&self, subject: &str, tokens: &StoredTokens) -> Result<()>;
    fn clear(&self, subject: &str) -> Result<()>;
    async fn bind_backend_account_if_current(
        &self,
        subject: &str,
        expected_access_token: &str,
        backend_origin: &str,
        backend_user_id: &str,
    ) -> Result<bool> {
        let _lock = self.acquire_refresh_lock(subject).await?;
        let Some(tokens) = self.load(subject)? else {
            return Ok(false);
        };
        if !bool::from(
            tokens
                .access_token
                .as_str()
                .as_bytes()
                .ct_eq(expected_access_token.as_bytes()),
        ) {
            return Ok(false);
        }
        self.save(
            subject,
            &tokens.bind_backend_account(backend_origin.to_owned(), backend_user_id.to_owned()),
        )?;
        Ok(true)
    }

    async fn acquire_refresh_lock(&self, subject: &str) -> Result<Option<RefreshLock>>;
}

pub(crate) struct RefreshLock {
    #[expect(dead_code, reason = "held-for-RAII; release runs on drop")]
    file: fs::File,
}

#[derive(Debug, Clone)]
pub(crate) struct FileTokenStore {
    dir: PathBuf,
}

impl FileTokenStore {
    pub(crate) fn default_location() -> Result<Self> {
        let dir = crate::support::storage::paths::tokens_dir()?;
        Ok(Self { dir })
    }

    fn validate_subject(subject: &str) -> Result<()> {
        const MAX_SUBJECT_LEN: usize = 128;
        if subject.is_empty() || subject.len() > MAX_SUBJECT_LEN {
            return Err(crate::AppError::Unsupported {
                reason: format!(
                    "token subject must be 1..={MAX_SUBJECT_LEN} bytes (got {})",
                    subject.len()
                ),
            });
        }
        let ok = subject
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        if !ok {
            return Err(crate::AppError::Unsupported {
                reason: "token subject must match [A-Za-z0-9._-]+".to_owned(),
            });
        }
        if subject.starts_with('.') {
            return Err(crate::AppError::Unsupported {
                reason: "token subject must not start with `.`".to_owned(),
            });
        }
        Ok(())
    }

    fn token_path(&self, subject: &str) -> Result<PathBuf> {
        Self::validate_subject(subject)?;
        Ok(self.dir.join(format!("{subject}.tokens.json")))
    }

    fn ensure_dir(&self) -> Result<()> {
        support_fs::ensure_dir(&self.dir)
    }

    fn acquire_refresh_lock_blocking(&self, subject: &str) -> Result<Option<RefreshLock>> {
        Self::validate_subject(subject)?;
        self.ensure_dir()?;
        let lock_path = self.dir.join(format!("{subject}.tokens.lock"));
        let file = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(crate::AppError::Io)?;

        file.lock().map_err(crate::AppError::Io)?;
        Ok(Some(RefreshLock { file }))
    }
}

impl TokenStore for FileTokenStore {
    fn load(&self, subject: &str) -> Result<Option<StoredTokens>> {
        let path = self.token_path(subject)?;
        match fs::read_to_string(&path) {
            Ok(payload) => serde_json::from_str(&payload)
                .map(Some)
                .map_err(crate::AppError::Json),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(crate::AppError::Io(e)),
        }
    }

    fn save(&self, subject: &str, tokens: &StoredTokens) -> Result<()> {
        self.ensure_dir()?;
        let path = self.token_path(subject)?;
        atomic_write_json(&path, tokens, true, FileMode::UserPrivate)
    }

    fn clear(&self, subject: &str) -> Result<()> {
        let path = self.token_path(subject)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(crate::AppError::Io(e)),
        }
    }

    async fn acquire_refresh_lock(&self, subject: &str) -> Result<Option<RefreshLock>> {
        let store = self.clone();
        let subject = subject.to_owned();
        tokio::task::spawn_blocking(move || store.acquire_refresh_lock_blocking(&subject))
            .await
            .map_err(|error| crate::AppError::Unsupported {
                reason: format!("token refresh lock task failed: {error}"),
            })?
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PlatformTokenStore {
    platform: std::sync::Arc<crate::identity_core::platform_secrets::PlatformSecretStore>,
    file: FileTokenStore,
}

impl PlatformTokenStore {
    pub(crate) fn default_location() -> Result<Self> {
        let file = FileTokenStore::default_location()?;
        let platform = std::sync::Arc::new(
            crate::identity_core::platform_secrets::PlatformSecretStore::new(
                Self::SERVICE_NAME,
                file.dir.clone(),
            ),
        );
        if crate::support::storage::paths::isolated_root()?.is_some() {
            platform.disable_secure_store();
        }
        Ok(Self { platform, file })
    }

    pub(crate) fn at(dir: PathBuf, service_name: &str) -> Self {
        let file = FileTokenStore { dir };
        let platform = std::sync::Arc::new(
            crate::identity_core::platform_secrets::PlatformSecretStore::new(
                service_name,
                file.dir.clone(),
            ),
        );
        platform.disable_secure_store();
        Self { platform, file }
    }

    pub(crate) fn peek_tokens_read_only(&self, subject: &str) -> Result<Option<StoredTokens>> {
        let label = Self::account_label(subject)?;
        let file_tokens = self.file.load(subject);
        match self.platform.load(&label) {
            Ok(PlatformSecretLoadResult::Loaded(json)) => {
                let platform_tokens: StoredTokens =
                    serde_json::from_str(&json).map_err(crate::AppError::Json)?;
                match file_tokens {
                    Ok(Some(file_tokens))
                        if Self::file_tokens_take_precedence(&file_tokens, &platform_tokens) =>
                    {
                        Ok(Some(file_tokens))
                    }
                    Ok(_) | Err(_) => Ok(Some(platform_tokens)),
                }
            }
            Ok(
                PlatformSecretLoadResult::Missing | PlatformSecretLoadResult::Unavailable { .. },
            )
            | Err(_) => file_tokens,
        }
    }

    #[cfg(any(test, feature = "cli"))]
    pub(crate) fn peek_backend_account(
        &self,
        subject: &str,
    ) -> Result<Option<StoredBackendAccount>> {
        Ok(self
            .peek_tokens_read_only(subject)?
            .and_then(|tokens| tokens.backend_account()))
    }

    const SERVICE_NAME: &'static str = "com.kodosi.tokens";

    fn account_label(subject: &str) -> Result<String> {
        FileTokenStore::validate_subject(subject)?;
        Ok(subject.to_owned())
    }

    fn same_token_generation(lhs: &StoredTokens, rhs: &StoredTokens) -> bool {
        bool::from(
            lhs.access_token
                .as_str()
                .as_bytes()
                .ct_eq(rhs.access_token.as_str().as_bytes()),
        )
    }

    fn file_tokens_take_precedence(file: &StoredTokens, platform: &StoredTokens) -> bool {
        if !Self::same_token_generation(file, platform) {
            return false;
        }
        if file.expires_at != platform.expires_at {
            return file.expires_at > platform.expires_at;
        }
        file.has_backend_account() && !platform.has_backend_account()
    }

    fn reconcile_loaded_tokens(
        &self,
        label: &str,
        subject: &str,
        json: &str,
    ) -> Result<Option<StoredTokens>> {
        let platform_tokens: StoredTokens =
            serde_json::from_str(json).map_err(crate::AppError::Json)?;
        let file_tokens = match self.file.load(subject) {
            Ok(Some(tokens)) => tokens,
            Ok(None) => return Ok(Some(platform_tokens)),
            Err(error) => {
                tracing::warn!(
                    %error,
                    "ignoring unreadable token fallback because platform storage is valid"
                );
                return Ok(Some(platform_tokens));
            }
        };
        if Self::file_tokens_take_precedence(&file_tokens, &platform_tokens) {
            tracing::warn!("private fallback takes precedence over platform token storage");
            let file_json = serde_json::to_string(&file_tokens).map_err(crate::AppError::Json)?;
            if matches!(
                self.platform.store(label, &file_json),
                Ok(PlatformSecretStoreResult::Stored)
            ) {
                drop(self.file.clear(subject));
            }
            return Ok(Some(file_tokens));
        }
        drop(self.file.clear(subject));
        Ok(Some(platform_tokens))
    }

    fn migrate_fallback_tokens(&self, label: &str, subject: &str) -> Result<Option<StoredTokens>> {
        let Some(tokens) = self.file.load(subject)? else {
            return Ok(None);
        };
        let json = serde_json::to_string(&tokens).map_err(crate::AppError::Json)?;
        match self.platform.store(label, &json) {
            Ok(PlatformSecretStoreResult::Stored) => {
                if let Err(error) = self.file.clear(subject) {
                    tracing::warn!(
                        %error,
                        "migrated tokens to platform storage but could not remove fallback file"
                    );
                }
            }
            Ok(PlatformSecretStoreResult::Unavailable) => {}
            Err(error) => {
                tracing::warn!(
                    %error,
                    "could not migrate fallback tokens to platform storage; using the valid fallback"
                );
            }
        }
        Ok(Some(tokens))
    }

    fn fall_back_to_file_after_platform_error(
        &self,
        subject: &str,
        platform_error: crate::AppError,
    ) -> Result<Option<StoredTokens>> {
        match self.file.load(subject)? {
            Some(tokens) => {
                tracing::warn!(
                    error = %platform_error,
                    "platform token storage failed; using the valid private fallback"
                );
                Ok(Some(tokens))
            }
            None => Err(platform_error),
        }
    }
}

impl TokenStore for PlatformTokenStore {
    fn load(&self, subject: &str) -> Result<Option<StoredTokens>> {
        let label = Self::account_label(subject)?;
        match self.platform.load(&label) {
            Ok(PlatformSecretLoadResult::Loaded(json)) => {
                self.reconcile_loaded_tokens(&label, subject, &json)
            }
            Ok(PlatformSecretLoadResult::Missing) => self.migrate_fallback_tokens(&label, subject),
            Ok(PlatformSecretLoadResult::Unavailable { .. }) => self.file.load(subject),
            Err(platform_error) => {
                self.fall_back_to_file_after_platform_error(subject, platform_error)
            }
        }
    }

    fn save(&self, subject: &str, tokens: &StoredTokens) -> Result<()> {
        let label = Self::account_label(subject)?;
        let json = serde_json::to_string(tokens).map_err(crate::AppError::Json)?;
        match self.platform.store(&label, &json) {
            Ok(PlatformSecretStoreResult::Stored) => self.file.clear(subject),
            Ok(PlatformSecretStoreResult::Unavailable) => self.file.save(subject, tokens),
            Err(error) => {
                tracing::warn!(
                    %error,
                    "platform token storage failed; persisting to the private fallback"
                );
                self.file.save(subject, tokens)?;
                if let Err(delete_error) = self.platform.delete(&label) {
                    tracing::warn!(
                        %delete_error,
                        "could not remove stale platform tokens after fallback save"
                    );
                }
                Ok(())
            }
        }
    }

    fn clear(&self, subject: &str) -> Result<()> {
        let label = Self::account_label(subject)?;

        let platform_result = self.platform.delete(&label);
        let file_result = self.file.clear(subject);
        platform_result?;
        file_result
    }

    async fn acquire_refresh_lock(&self, subject: &str) -> Result<Option<RefreshLock>> {
        self.file.acquire_refresh_lock(subject).await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use time::OffsetDateTime;

    use super::{FileTokenStore, PlatformTokenStore, StoredTokens, TokenStore};

    #[test]
    fn legacy_token_json_remains_readable_but_has_no_account_binding() {
        let tokens = StoredTokens::new(
            "old".to_owned(),
            Some("refresh".to_owned()),
            OffsetDateTime::UNIX_EPOCH + time::Duration::hours(1),
        );
        let mut legacy = serde_json::to_value(tokens).expect("serialize old shape");
        legacy
            .as_object_mut()
            .expect("token object")
            .remove("backend_origin");
        legacy
            .as_object_mut()
            .expect("token object")
            .remove("backend_user_id");
        let tokens: StoredTokens = serde_json::from_value(legacy).expect("legacy tokens");

        assert!(tokens.backend_account().is_none());
    }

    #[test]
    fn token_account_binding_round_trips_without_exposing_bearers() {
        let tokens = StoredTokens::new(
            "secret-access".to_owned(),
            Some("secret-refresh".to_owned()),
            OffsetDateTime::UNIX_EPOCH + time::Duration::hours(1),
        )
        .bind_backend_account(
            "https://backend.example:443/base".to_owned(),
            "01900000-0000-7000-8000-000000000001".to_owned(),
        );
        let json = serde_json::to_string(&tokens).expect("serialize");
        let round: StoredTokens = serde_json::from_str(&json).expect("deserialize");

        let binding = round.backend_account().expect("binding");
        assert_eq!(binding.backend_origin, "https://backend.example:443/base");
        assert_eq!(
            binding.backend_user_id,
            "01900000-0000-7000-8000-000000000001"
        );
        assert!(!format!("{round:?}").contains("secret-access"));
        assert!(!format!("{round:?}").contains("secret-refresh"));
    }

    #[tokio::test]
    async fn stale_token_generation_cannot_overwrite_new_account_binding() {
        let directory = tempfile::tempdir().expect("token directory");
        let store = PlatformTokenStore::at(directory.path().to_path_buf(), "kodosi.tokens.test");
        let current = StoredTokens::new(
            "new-access".to_owned(),
            None,
            OffsetDateTime::now_utc() + time::Duration::hours(1),
        )
        .bind_backend_account(
            "https://backend.example:443/".to_owned(),
            "22222222-2222-2222-2222-222222222222".to_owned(),
        );
        store.save("default", &current).expect("save current");

        assert!(
            !store
                .bind_backend_account_if_current(
                    "default",
                    "old-access",
                    "https://backend.example:443/",
                    "11111111-1111-1111-1111-111111111111",
                )
                .await
                .expect("stale bind verdict")
        );
        let binding = store
            .peek_backend_account("default")
            .expect("peek")
            .expect("binding");
        assert_eq!(
            binding.backend_user_id,
            "22222222-2222-2222-2222-222222222222"
        );
    }

    #[test]
    fn default_platform_location_constructs_without_reading_credentials() {
        PlatformTokenStore::default_location()
            .unwrap_or_else(|error| panic!("default token store should construct: {error}"));
    }

    #[test]
    fn newer_fallback_from_another_generation_does_not_override_platform() {
        let platform = StoredTokens::new(
            "platform-access".to_owned(),
            Some("platform-refresh".to_owned()),
            OffsetDateTime::UNIX_EPOCH + time::Duration::hours(1),
        );
        let file = StoredTokens::new(
            "fallback-access".to_owned(),
            Some("fallback-refresh".to_owned()),
            OffsetDateTime::UNIX_EPOCH + time::Duration::hours(2),
        );

        assert!(!PlatformTokenStore::file_tokens_take_precedence(
            &file, &platform
        ));
        assert!(!PlatformTokenStore::file_tokens_take_precedence(
            &platform, &file
        ));
    }

    #[test]
    fn newer_fallback_from_same_generation_outranks_platform() {
        let platform = StoredTokens::new(
            "same-access".to_owned(),
            Some("old-refresh".to_owned()),
            OffsetDateTime::UNIX_EPOCH + time::Duration::hours(1),
        );
        let file = StoredTokens::new(
            "same-access".to_owned(),
            Some("new-refresh".to_owned()),
            OffsetDateTime::UNIX_EPOCH + time::Duration::hours(2),
        );

        assert!(PlatformTokenStore::file_tokens_take_precedence(
            &file, &platform
        ));
    }

    #[test]
    fn equal_expiry_same_generation_richer_fallback_takes_precedence() {
        let expiry = OffsetDateTime::UNIX_EPOCH + time::Duration::hours(1);
        let platform = StoredTokens::new("same-access".to_owned(), None, expiry);
        let file = StoredTokens::new("same-access".to_owned(), None, expiry).bind_backend_account(
            "https://backend.example/".to_owned(),
            "11111111-1111-1111-1111-111111111111".to_owned(),
        );

        assert!(PlatformTokenStore::file_tokens_take_precedence(
            &file, &platform
        ));
    }

    #[test]
    fn equal_expiry_different_generation_does_not_override_platform() {
        let expiry = OffsetDateTime::UNIX_EPOCH + time::Duration::hours(1);
        let platform = StoredTokens::new("platform-access".to_owned(), None, expiry);
        let file = StoredTokens::new("fallback-access".to_owned(), None, expiry)
            .bind_backend_account(
                "https://backend.example/".to_owned(),
                "11111111-1111-1111-1111-111111111111".to_owned(),
            );

        assert!(!PlatformTokenStore::file_tokens_take_precedence(
            &file, &platform
        ));
    }

    #[test]
    fn equal_expiry_conflicting_complete_binding_does_not_override_platform() {
        let expiry = OffsetDateTime::UNIX_EPOCH + time::Duration::hours(1);
        let platform = StoredTokens::new("same-access".to_owned(), None, expiry)
            .bind_backend_account(
                "https://backend.example/".to_owned(),
                "11111111-1111-1111-1111-111111111111".to_owned(),
            );
        let file = StoredTokens::new("same-access".to_owned(), None, expiry).bind_backend_account(
            "https://backend.example/".to_owned(),
            "22222222-2222-2222-2222-222222222222".to_owned(),
        );

        assert!(!PlatformTokenStore::file_tokens_take_precedence(
            &file, &platform
        ));
    }

    #[test]
    fn corrupt_fallback_does_not_override_valid_platform_tokens() {
        let directory = tempfile::tempdir().expect("token directory");
        let store = PlatformTokenStore::at(directory.path().to_path_buf(), "kodosi.tokens.test");
        store.file.ensure_dir().expect("fallback directory");
        std::fs::write(
            store.file.token_path("default").expect("fallback path"),
            "{not valid json",
        )
        .expect("write corrupt fallback");
        let platform = StoredTokens::new(
            "platform-access".to_owned(),
            Some("platform-refresh".to_owned()),
            OffsetDateTime::UNIX_EPOCH + time::Duration::hours(1),
        );
        let json = serde_json::to_string(&platform).expect("serialize platform tokens");

        let loaded = store
            .reconcile_loaded_tokens("default", "default", &json)
            .expect("valid platform tokens should survive corrupt fallback")
            .expect("platform tokens should remain available");

        assert_eq!(loaded.access_token.as_str(), "platform-access");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn contended_refresh_lock_waits_without_blocking_async_runtime() {
        let dir = tempfile::tempdir().expect("token lock tempdir");
        let store = FileTokenStore {
            dir: dir.path().to_path_buf(),
        };
        let first = store
            .acquire_refresh_lock("default")
            .await
            .expect("first lock");
        let waiting_store = store.clone();
        let waiter = tokio::spawn(async move {
            waiting_store
                .acquire_refresh_lock("default")
                .await
                .expect("second lock")
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!waiter.is_finished());
        drop(first);
        let second = tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("second lock should acquire")
            .expect("lock task should finish");
        drop(second);
    }

    #[test]
    fn validate_subject_accepts_default() {
        assert!(FileTokenStore::validate_subject("default").is_ok());
    }

    #[test]
    fn validate_subject_accepts_safe_punctuation() {
        for s in ["user.1", "user_1", "user-1", "u.s_e-r.42"] {
            assert!(
                FileTokenStore::validate_subject(s).is_ok(),
                "expected {s:?} to be accepted"
            );
        }
    }

    #[test]
    fn validate_subject_rejects_path_traversal() {
        for s in [
            "..",
            "../etc/passwd",
            "../../foo",
            "foo/bar",
            "foo\\bar",
            ".\u{0}.",
        ] {
            assert!(
                FileTokenStore::validate_subject(s).is_err(),
                "expected {s:?} to be rejected"
            );
        }
    }

    #[test]
    fn validate_subject_rejects_empty_and_overlong() {
        assert!(FileTokenStore::validate_subject("").is_err());
        let long = "a".repeat(129);
        assert!(FileTokenStore::validate_subject(&long).is_err());
    }

    #[test]
    fn validate_subject_rejects_non_ascii() {
        assert!(FileTokenStore::validate_subject("üser").is_err());
        assert!(FileTokenStore::validate_subject("user\u{202E}name").is_err());
    }
}

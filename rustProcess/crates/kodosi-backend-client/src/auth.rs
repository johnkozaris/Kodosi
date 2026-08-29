use std::{error::Error, fmt, future::Future, pin::Pin, sync::Arc};

use zeroize::Zeroizing;

pub type BackendAccessTokenFuture<'a> =
    Pin<Box<dyn Future<Output = Result<BackendAccessTokenState, BackendAuthError>> + Send + 'a>>;

#[derive(Debug, Clone)]
pub enum BackendAccessTokenState {
    Ready {
        access_token: Zeroizing<String>,
        refreshed: bool,
    },
    RequiresLogin {
        reason: BackendAccessTokenIssue,
    },
    TemporarilyUnavailable {
        reason: BackendAccessTokenIssue,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendAccessTokenIssue {
    AuthNotConfigured,
    NoStoredSession,
    RefreshRejected(String),
    RefreshTemporarilyUnavailable(String),
}

impl fmt::Display for BackendAccessTokenIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthNotConfigured => write!(f, "auth is not configured"),
            Self::NoStoredSession => write!(f, "no stored session is available"),
            Self::RefreshRejected(reason) | Self::RefreshTemporarilyUnavailable(reason) => {
                write!(f, "{reason}")
            }
        }
    }
}

pub trait BackendAccessTokenProvider: Send + Sync + 'static {
    fn access_token(&self, revoked_access_token: Option<String>) -> BackendAccessTokenFuture<'_>;
}

#[derive(Clone)]
pub struct BackendAuthProvider {
    provider: Arc<dyn BackendAccessTokenProvider>,
}

impl BackendAuthProvider {
    pub fn new(provider: impl BackendAccessTokenProvider) -> Self {
        Self {
            provider: Arc::new(provider),
        }
    }

    pub fn access_token(
        &self,
        revoked_access_token: Option<String>,
    ) -> BackendAccessTokenFuture<'_> {
        self.provider.access_token(revoked_access_token)
    }
}

impl fmt::Debug for BackendAuthProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BackendAuthProvider")
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct BackendAuthError {
    source: Box<dyn Error + Send + Sync + 'static>,
}

impl BackendAuthError {
    pub fn from_error(error: impl Error + Send + Sync + 'static) -> Self {
        Self {
            source: Box::new(error),
        }
    }

    pub fn into_source(self) -> Box<dyn Error + Send + Sync + 'static> {
        self.source
    }
}

impl fmt::Display for BackendAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "backend access token provider failed: {}", self.source)
    }
}

impl Error for BackendAuthError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

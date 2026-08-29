pub(crate) mod migration;

use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
};

use kodosi_domain::{
    ids::SessionId,
    session::{LocalSessionRecoveryState, SessionState, SessionSummary},
    terminal::TerminalSize,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AppError, Result,
    session_runtime::registry::SessionRecord,
    support::storage::atomic_file::{FileMode, atomic_write_json},
};

const CATALOG_VERSION: u32 = 2;
const LEGACY_CATALOG_VERSION: u32 = 1;

const fn default_launch_committed() -> bool {
    true
}

#[derive(Debug)]
pub(crate) struct LocalSessionCatalog {
    root: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct DiscoveredLocalSession {
    pub(crate) summary: SessionSummary,
    pub(crate) create_request_id: Option<String>,
    pub(crate) resume_source:
        Option<kodosi_domain::provider_conversation::ProviderConversationIdentity>,
    pub(crate) launch_committed: bool,
    pub(crate) local_incarnation_id: Uuid,
    pub(crate) recovery: LocalSessionRecoveryState,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemovedMarker {
    version: u32,
    session_id: SessionId,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionDescriptor {
    version: u32,
    local_incarnation_id: Uuid,
    lifecycle_state: SessionState,
    summary: SessionSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    create_request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resume_source: Option<kodosi_domain::provider_conversation::ProviderConversationIdentity>,
    #[serde(default = "default_launch_committed")]
    launch_committed: bool,
}

impl LocalSessionCatalog {
    pub(crate) fn at(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)?;
        Ok(Self { root: Some(root) })
    }

    pub(crate) fn load_default() -> Result<Self> {
        let root = crate::support::storage::paths::session_catalog_dir()?;
        fs::create_dir_all(&root)?;
        Ok(Self { root: Some(root) })
    }

    pub(crate) fn persist_record(&self, record: &SessionRecord) -> Result<()> {
        self.persist(
            &record.summary,
            record.create_request_id.clone(),
            record.resume_source.clone(),
            record.launch_committed,
            record.local_incarnation_id,
        )
    }

    pub(crate) fn persist_new(
        &self,
        summary: &SessionSummary,
        create_request_id: Option<String>,
        resume_source: Option<kodosi_domain::provider_conversation::ProviderConversationIdentity>,
        launch_committed: bool,
        local_incarnation_id: Uuid,
    ) -> Result<()> {
        self.persist(
            summary,
            create_request_id,
            resume_source,
            launch_committed,
            local_incarnation_id,
        )
    }

    pub(crate) fn persist_reopened(
        &self,
        summary: &SessionSummary,
        create_request_id: Option<String>,
        resume_source: Option<kodosi_domain::provider_conversation::ProviderConversationIdentity>,
        launch_committed: bool,
        local_incarnation_id: Uuid,
    ) -> Result<()> {
        self.persist(
            summary,
            create_request_id,
            resume_source,
            launch_committed,
            local_incarnation_id,
        )
    }

    fn persist(
        &self,
        summary: &SessionSummary,
        create_request_id: Option<String>,
        resume_source: Option<kodosi_domain::provider_conversation::ProviderConversationIdentity>,
        launch_committed: bool,
        local_incarnation_id: Uuid,
    ) -> Result<()> {
        let Some(path) = self.path(summary.id) else {
            return Ok(());
        };
        let marker = removed_marker_path(&path);
        match fs::remove_file(marker) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(AppError::Io(error)),
        }
        atomic_write_json(
            &path,
            &SessionDescriptor {
                version: CATALOG_VERSION,
                local_incarnation_id,
                lifecycle_state: summary.state,
                summary: summary.clone(),
                create_request_id,
                resume_source,
                launch_committed,
            },
            true,
            FileMode::UserPrivate,
        )
    }

    pub(crate) fn remove_from_list(&self, id: SessionId) -> Result<()> {
        let Some(path) = self.path(id) else {
            return Ok(());
        };
        let marker = removed_marker_path(&path);
        atomic_write_json(
            &marker,
            &RemovedMarker {
                version: CATALOG_VERSION,
                session_id: id,
            },
            false,
            FileMode::UserPrivate,
        )?;
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(session_id = %id, %error, "removed session descriptor could not be unlinked");
            }
        }
        Ok(())
    }

    pub(crate) fn discover(
        &self,
        cache_root: Option<&Path>,
        legacy_cache: Vec<migration::CachedOwnedSession>,
    ) -> Vec<DiscoveredLocalSession> {
        let removed = self.removed_ids();
        let mut result = HashMap::new();
        self.load_descriptors(&removed, &mut result);
        self.migrate_legacy(cache_root, legacy_cache, &removed, &mut result);
        let mut result = result.into_values().collect::<Vec<_>>();
        result.sort_by_key(|entry| entry.summary.created_at);
        result
    }

    fn load_descriptors(
        &self,
        removed: &HashSet<SessionId>,
        result: &mut HashMap<SessionId, DiscoveredLocalSession>,
    ) {
        let Some(root) = self.root.as_ref() else {
            return;
        };
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(id) = descriptor_id(&path) else {
                continue;
            };
            if removed.contains(&id) {
                continue;
            }
            let loaded = fs::read_to_string(&path)
                .map_err(AppError::Io)
                .and_then(|text| {
                    serde_json::from_str::<SessionDescriptor>(&text).map_err(AppError::Json)
                });
            let discovered = match loaded {
                Ok(document) if descriptor_valid(&document, id) => {
                    let legacy = document.version == LEGACY_CATALOG_VERSION;
                    let discovered = reconcile(document);
                    if legacy {
                        drop(self.persist(
                            &discovered.summary,
                            discovered.create_request_id.clone(),
                            discovered.resume_source.clone(),
                            discovered.launch_committed,
                            discovered.local_incarnation_id,
                        ));
                    }
                    discovered
                }
                Ok(_) => quarantine(
                    id,
                    fallback_incarnation_id(id),
                    "descriptor identity/version mismatch",
                ),
                Err(error) => quarantine(
                    id,
                    fallback_incarnation_id(id),
                    &format!("invalid descriptor: {error}"),
                ),
            };
            result.insert(id, discovered);
        }
    }

    fn migrate_legacy(
        &self,
        cache_root: Option<&Path>,
        cached: Vec<migration::CachedOwnedSession>,
        removed: &HashSet<SessionId>,
        result: &mut HashMap<SessionId, DiscoveredLocalSession>,
    ) {
        for cached in cached {
            let id = cached.summary.id;
            if result.contains_key(&id) || removed.contains(&id) {
                continue;
            }

            let local_incarnation_id = fallback_incarnation_id(id);
            let discovered =
                reconcile_summary(cached.summary, None, None, true, local_incarnation_id);
            if self
                .persist(
                    &discovered.summary,
                    discovered.create_request_id.clone(),
                    discovered.resume_source.clone(),
                    discovered.launch_committed,
                    local_incarnation_id,
                )
                .is_ok()
            {
                drop(migration::delete_cached_session(
                    cache_root,
                    &discovered.summary.runtime_name,
                ));
                result.insert(id, discovered);
            }
        }
    }

    fn removed_ids(&self) -> HashSet<SessionId> {
        let Some(root) = self.root.as_ref() else {
            return HashSet::new();
        };
        let Ok(entries) = fs::read_dir(root) else {
            return HashSet::new();
        };
        entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let id = removed_marker_id(&path)?;
                match fs::read_to_string(&path)
                    .ok()
                    .and_then(|text| serde_json::from_str::<RemovedMarker>(&text).ok())
                {
                    Some(marker)
                        if matches!(marker.version, LEGACY_CATALOG_VERSION | CATALOG_VERSION)
                            && marker.session_id == id =>
                    {
                        if marker.version == LEGACY_CATALOG_VERSION
                            && let Err(error) = atomic_write_json(
                                &path,
                                &RemovedMarker {
                                    version: CATALOG_VERSION,
                                    ..marker
                                },
                                false,
                                FileMode::UserPrivate,
                            )
                        {
                            tracing::warn!(
                                session_id = %id,
                                %error,
                                "legacy remove-from-list marker could not be upgraded"
                            );
                        }
                        Some(id)
                    }
                    _ => {
                        tracing::warn!(session_id = %id, path = %path.display(), "invalid remove-from-list marker; keeping session hidden");
                        Some(id)
                    }
                }
            })
            .collect()
    }

    fn path(&self, id: SessionId) -> Option<PathBuf> {
        self.root
            .as_ref()
            .map(|root| root.join(format!("{}.json", id.simple())))
    }
}

fn descriptor_valid(document: &SessionDescriptor, id: SessionId) -> bool {
    matches!(document.version, LEGACY_CATALOG_VERSION | CATALOG_VERSION)
        && document.summary.id == id
        && document.lifecycle_state == document.summary.state
        && document.resume_source.as_ref().is_none_or(|source| {
            Uuid::parse_str(&source.native_conversation_id).is_ok_and(|id| !id.is_nil())
        })
}

fn reconcile(document: SessionDescriptor) -> DiscoveredLocalSession {
    reconcile_summary(
        document.summary,
        document.create_request_id,
        document.resume_source,
        document.launch_committed,
        document.local_incarnation_id,
    )
}

fn reconcile_summary(
    mut summary: SessionSummary,
    create_request_id: Option<String>,
    resume_source: Option<kodosi_domain::provider_conversation::ProviderConversationIdentity>,
    launch_committed: bool,
    local_incarnation_id: Uuid,
) -> DiscoveredLocalSession {
    let recovery = if summary.state == SessionState::Stopped {
        LocalSessionRecoveryState::Recoverable
    } else {
        summary.state = SessionState::Failed;
        LocalSessionRecoveryState::Crashed
    };
    DiscoveredLocalSession {
        summary,
        create_request_id,
        resume_source,
        launch_committed,
        local_incarnation_id,
        recovery,
    }
}

fn quarantine(id: SessionId, local_incarnation_id: Uuid, reason: &str) -> DiscoveredLocalSession {
    tracing::warn!(session_id = %id, %reason, "quarantined local session descriptor");
    let mut summary = SessionSummary::new_owned(
        id,
        format!("Quarantined {}", id.short()),
        format!("kodosi-quarantined-{}", id.simple()),
        TerminalSize::default(),
        None,
    );
    summary.state = SessionState::Failed;
    DiscoveredLocalSession {
        summary,
        create_request_id: None,
        resume_source: None,
        launch_committed: true,
        local_incarnation_id,
        recovery: LocalSessionRecoveryState::Quarantined,
    }
}

fn fallback_incarnation_id(id: SessionId) -> Uuid {
    Uuid::parse_str(&id.to_string()).unwrap_or_else(|_| Uuid::nil())
}

fn removed_marker_path(descriptor: &Path) -> PathBuf {
    descriptor.with_extension("removed")
}

fn removed_marker_id(path: &Path) -> Option<SessionId> {
    (path.extension()?.to_str()? == "removed").then_some(())?;
    let id = Uuid::parse_str(path.file_stem()?.to_str()?).ok()?;
    SessionId::try_from(id.to_string().as_str()).ok()
}

fn descriptor_id(path: &Path) -> Option<SessionId> {
    (path.extension()?.to_str()? == "json").then_some(())?;
    let id = Uuid::parse_str(path.file_stem()?.to_str()?).ok()?;
    SessionId::try_from(id.to_string().as_str()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crashed_descriptor_retains_stable_incarnation_across_reload() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = LocalSessionCatalog::at(dir.path().join("catalog")).unwrap();
        let id = SessionId::new();
        let incarnation = Uuid::now_v7();
        let mut summary = SessionSummary::new_owned(
            id,
            "live".to_owned(),
            "runtime-live".to_owned(),
            TerminalSize::default(),
            None,
        );
        summary.state = SessionState::Running;
        catalog
            .persist_new(&summary, None, None, true, incarnation)
            .unwrap();

        let first = catalog.discover(None, Vec::new());
        let second = catalog.discover(None, Vec::new());
        assert_eq!(first[0].recovery, LocalSessionRecoveryState::Crashed);
        assert_eq!(first[0].local_incarnation_id, incarnation);
        assert_eq!(second[0].local_incarnation_id, incarnation);
    }

    #[test]
    fn legacy_descriptor_is_preserved_and_upgraded_without_external_history() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("catalog");
        let catalog = LocalSessionCatalog::at(root.clone()).unwrap();
        let id = SessionId::new();
        let incarnation = Uuid::now_v7();
        let mut summary = SessionSummary::new_owned(
            id,
            "legacy".to_owned(),
            "runtime-legacy".to_owned(),
            TerminalSize::default(),
            None,
        );
        summary.state = SessionState::Running;
        let descriptor = SessionDescriptor {
            version: LEGACY_CATALOG_VERSION,
            local_incarnation_id: incarnation,
            lifecycle_state: summary.state,
            summary,
            create_request_id: None,
            resume_source: None,
            launch_committed: true,
        };
        fs::write(
            root.join(format!("{}.json", id.simple())),
            serde_json::to_vec_pretty(&descriptor).unwrap(),
        )
        .unwrap();

        let discovered = catalog.discover(None, Vec::new());
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].local_incarnation_id, incarnation);
        assert_eq!(discovered[0].recovery, LocalSessionRecoveryState::Crashed);
        let upgraded: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join(format!("{}.json", id.simple()))).unwrap())
                .unwrap();
        assert_eq!(upgraded["version"], CATALOG_VERSION);
    }

    #[test]
    fn legacy_session_policy_fields_are_ignored_and_not_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("catalog");
        let catalog = LocalSessionCatalog::at(root.clone()).unwrap();
        let id = SessionId::new();
        let mut summary = SessionSummary::new_owned(
            id,
            "unsafe".to_owned(),
            "runtime-unsafe".to_owned(),
            TerminalSize::default(),
            None,
        );
        summary.state = SessionState::Stopped;
        let descriptor = serde_json::json!({
            "version": LEGACY_CATALOG_VERSION,
            "localIncarnationId": Uuid::now_v7(),
            "lifecycleState": "Stopped",
            "recoveryState": "Recoverable",
            "resumable": true,
            "summary": summary,
            "permissionTimeoutOverride": {
                "policy": "failOpen",
                "timeoutSecs": 30
            },
            "approvalRouting": {
                "approvers": ["legacy-user"],
                "escalateAfterSecs": 30,
                "fallback": "failClosed"
            }
        });
        fs::write(
            root.join(format!("{}.json", id.simple())),
            serde_json::to_vec_pretty(&descriptor).unwrap(),
        )
        .unwrap();

        let discovered = catalog.discover(None, Vec::new());
        assert_eq!(
            discovered[0].recovery,
            LocalSessionRecoveryState::Recoverable
        );
        let rewritten: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join(format!("{}.json", id.simple()))).unwrap())
                .unwrap();
        assert_eq!(rewritten["version"], CATALOG_VERSION);
        assert!(rewritten.get("permissionTimeoutOverride").is_none());
        assert!(rewritten.get("approvalRouting").is_none());
    }

    #[test]
    fn corrupt_descriptor_surfaces_quarantined_row() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("catalog");
        let catalog = LocalSessionCatalog::at(root.clone()).unwrap();
        let id = SessionId::new();
        fs::write(root.join(format!("{}.json", id.simple())), b"{").unwrap();
        let discovered = catalog.discover(None, Vec::new());
        assert_eq!(
            discovered[0].recovery,
            LocalSessionRecoveryState::Quarantined
        );
    }

    #[test]
    fn uncommitted_launch_reservation_survives_reconciliation_for_retirement() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = LocalSessionCatalog::at(dir.path().join("catalog")).unwrap();
        let id = SessionId::new();
        let summary = SessionSummary::new_owned(
            id,
            "reserved".to_owned(),
            "runtime-reserved".to_owned(),
            TerminalSize::default(),
            None,
        );
        catalog
            .persist_new(
                &summary,
                Some(Uuid::now_v7().to_string()),
                None,
                false,
                Uuid::now_v7(),
            )
            .unwrap();

        let discovered = catalog.discover(None, Vec::new());
        assert_eq!(discovered.len(), 1);
        assert!(!discovered[0].launch_committed);
    }

    #[test]
    fn removed_marker_suppresses_descriptor_that_could_not_be_unlinked() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = LocalSessionCatalog::at(dir.path().join("catalog")).unwrap();
        let id = SessionId::new();
        let summary = SessionSummary::new_owned(
            id,
            "done".to_owned(),
            "runtime".to_owned(),
            TerminalSize::default(),
            None,
        );
        catalog
            .persist_new(&summary, None, None, true, Uuid::now_v7())
            .unwrap();
        let descriptor = catalog.path(id).unwrap();
        let marker = descriptor.with_extension("removed");
        fs::write(
            marker,
            serde_json::to_vec(&RemovedMarker {
                version: CATALOG_VERSION,
                session_id: id,
            })
            .unwrap(),
        )
        .unwrap();

        assert!(descriptor.exists());
        assert!(catalog.discover(None, Vec::new()).is_empty());
    }

    #[test]
    fn remove_from_list_survives_catalog_restart() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = LocalSessionCatalog::at(dir.path().join("catalog")).unwrap();
        let id = SessionId::new();
        let summary = SessionSummary::new_owned(
            id,
            "done".to_owned(),
            "runtime".to_owned(),
            TerminalSize::default(),
            None,
        );
        catalog
            .persist_new(&summary, None, None, true, Uuid::now_v7())
            .unwrap();
        let legacy = migration::CachedOwnedSession {
            summary: summary.clone(),
            cache_age: std::time::Duration::ZERO,
        };
        catalog.remove_from_list(id).unwrap();
        let marker = catalog.path(id).unwrap().with_extension("removed");
        assert!(
            catalog.discover(None, vec![legacy.clone()]).is_empty(),
            "legacy cache cannot resurrect an explicitly removed row"
        );
        let restarted = LocalSessionCatalog::at(dir.path().join("catalog")).unwrap();
        assert!(
            restarted.discover(None, vec![legacy]).is_empty(),
            "remove-from-list survives catalog reconstruction"
        );
        let marker_document: RemovedMarker =
            serde_json::from_slice(&fs::read(&marker).unwrap()).unwrap();
        assert_eq!(marker_document.session_id, id);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(&marker).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::write(
            &marker,
            serde_json::to_vec(&RemovedMarker {
                version: LEGACY_CATALOG_VERSION,
                session_id: id,
            })
            .unwrap(),
        )
        .unwrap();
        assert!(
            restarted.discover(None, Vec::new()).is_empty(),
            "legacy remove marker must remain authoritative"
        );
        let upgraded: RemovedMarker = serde_json::from_slice(&fs::read(&marker).unwrap()).unwrap();
        assert_eq!(upgraded.version, CATALOG_VERSION);
        fs::write(&marker, b"{").unwrap();
        assert!(
            restarted.discover(None, Vec::new()).is_empty(),
            "corrupt remove marker fails hidden rather than resurrecting provenance"
        );
    }
}

use std::{
    collections::{HashMap, VecDeque},
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{
    bound_memory::MemorySelectionScope,
    external_sessions::{ExternalAgent, ExternalSession, LivenessStatus},
};

const DEFAULT_SELECTION_CAPACITY: usize = 512;
const DEFAULT_SELECTION_TTL: Duration = Duration::from_mins(5);
const MAX_EXTERNAL_ITEMS: usize = 512;
const MAX_EXTERNAL_RESPONSE_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoundExternalMcpSummary {
    pub selection_token: String,
    pub source: String,
    pub server_name: String,
    pub transport: Option<String>,
    pub can_copy_source_path: bool,
    pub can_open_source: bool,
    pub can_reveal_source: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoundExternalSessionSummary {
    pub selection_token: String,
    pub agent: String,
    pub session_id: String,
    pub pid: Option<u32>,
    pub liveness: String,
    pub workspace_label: Option<String>,
    pub name: Option<String>,
    pub started_at: Option<String>,
    pub can_copy_source_path: bool,
    pub can_open_source: bool,
    pub can_reveal_source: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoundExternalDiscovery {
    pub mcp_servers: Vec<BoundExternalMcpSummary>,
    pub sessions: Vec<BoundExternalSessionSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BoundExternalAction {
    Open {
        source_path: Option<String>,
        handoff_id: String,
        handoff_path: String,
        display_name: String,
    },
    CopyPath {
        source_path: String,
        handoff_id: Option<String>,
        handoff_path: Option<String>,
        display_name: String,
    },
    Reveal {
        source_path: String,
        handoff_id: Option<String>,
        handoff_path: Option<String>,
        display_name: String,
    },
}

#[derive(Debug)]
pub struct BoundExternalRegistry {
    selections: HashMap<uuid::Uuid, BoundExternalSelection>,
    insertion_order: VecDeque<uuid::Uuid>,
    capacity: usize,
    ttl: Duration,
}

impl Default for BoundExternalRegistry {
    fn default() -> Self {
        Self {
            selections: HashMap::new(),
            insertion_order: VecDeque::new(),
            capacity: DEFAULT_SELECTION_CAPACITY,
            ttl: DEFAULT_SELECTION_TTL,
        }
    }
}

impl BoundExternalRegistry {
    pub async fn discover(
        &mut self,
        home: &Path,
        scope: MemorySelectionScope,
    ) -> Result<BoundExternalDiscovery, String> {
        let home = home.to_owned();
        let sessions = super::external_sessions::discover_external_sessions(&home, None).await;
        let prepared = tokio::task::spawn_blocking(move || {
            let servers = crate::mcp::discover_external_mcp_servers(&home);
            prepare(&home, servers, sessions)
        })
        .await
        .map_err(|error| format!("bound external discovery task join: {error}"))?;
        self.install(prepared, &scope, Instant::now())
    }

    pub fn take(
        &mut self,
        selection_token: &str,
        scope: &MemorySelectionScope,
    ) -> Result<ResolvedExternalSelection, String> {
        let token = parse_token(selection_token)?;
        let selection = self
            .selections
            .remove(&token)
            .ok_or_else(|| "external source selection is stale or already consumed".to_owned())?;
        self.insertion_order.retain(|candidate| *candidate != token);
        if Instant::now() >= selection.expires_at {
            return Err("external source selection expired".to_owned());
        }
        if selection.scope != *scope {
            return Err("external source selection belongs to another account".to_owned());
        }
        selection.source.verify_current()?;
        Ok(ResolvedExternalSelection {
            path: selection.source.path.clone(),
            display_name: selection.source.display_name.clone(),
            file: selection.source.open_current()?,
        })
    }

    pub fn clear(&mut self) {
        self.selections.clear();
        self.insertion_order.clear();
    }

    pub fn purge_expired_now(&mut self) {
        self.purge_expired(Instant::now());
    }

    fn install(
        &mut self,
        prepared: PreparedDiscovery,
        scope: &MemorySelectionScope,
        now: Instant,
    ) -> Result<BoundExternalDiscovery, String> {
        self.purge_expired(now);
        let items = prepared
            .items
            .into_iter()
            .take(MAX_EXTERNAL_ITEMS)
            .collect::<Vec<_>>();
        if items.len() > self.capacity {
            return Err("external discovery exceeds capability capacity".to_owned());
        }
        let mut mcp_servers = Vec::new();
        let mut sessions = Vec::new();
        let mut planned = Vec::with_capacity(items.len());
        for item in items {
            let token = uuid::Uuid::now_v7();
            let can_open_source = !item.source.directory;
            match &item.summary {
                PreparedSummary::Mcp {
                    source,
                    server_name,
                    transport,
                } => {
                    mcp_servers.push(BoundExternalMcpSummary {
                        selection_token: token.to_string(),
                        source: source.clone(),
                        server_name: server_name.clone(),
                        transport: transport.clone(),
                        can_copy_source_path: true,
                        can_open_source,
                        can_reveal_source: true,
                    });
                }
                PreparedSummary::Session {
                    agent,
                    session_id,
                    pid,
                    liveness,
                    workspace_label,
                    name,
                    started_at,
                } => {
                    sessions.push(BoundExternalSessionSummary {
                        selection_token: token.to_string(),
                        agent: agent.clone(),
                        session_id: session_id.clone(),
                        pid: *pid,
                        liveness: liveness.clone(),
                        workspace_label: workspace_label.clone(),
                        name: name.clone(),
                        started_at: started_at.clone(),
                        can_copy_source_path: true,
                        can_open_source,
                        can_reveal_source: true,
                    });
                }
            }
            planned.push((token, item.source));
        }
        let discovery = BoundExternalDiscovery {
            mcp_servers,
            sessions,
        };
        let bytes = serde_json::to_vec(&discovery)
            .map_err(|error| format!("encode external discovery: {error}"))?
            .len();
        if bytes > MAX_EXTERNAL_RESPONSE_BYTES {
            return Err("external discovery exceeds response byte bound".to_owned());
        }
        while self.selections.len().saturating_add(planned.len()) > self.capacity {
            let Some(oldest) = self.insertion_order.pop_front() else {
                return Err("external capability registry is inconsistent".to_owned());
            };
            self.selections.remove(&oldest);
        }
        for (token, source) in planned {
            self.insertion_order.push_back(token);
            self.selections.insert(
                token,
                BoundExternalSelection {
                    scope: scope.clone(),
                    expires_at: now + self.ttl,
                    source: Arc::new(source),
                },
            );
        }
        Ok(discovery)
    }

    fn purge_expired(&mut self, now: Instant) {
        self.selections
            .retain(|_, selection| selection.expires_at > now);
        self.insertion_order
            .retain(|token| self.selections.contains_key(token));
    }

    #[cfg(test)]
    fn with_limits(capacity: usize, ttl: Duration) -> Self {
        Self {
            selections: HashMap::new(),
            insertion_order: VecDeque::new(),
            capacity,
            ttl,
        }
    }
}

#[derive(Debug)]
pub struct ResolvedExternalSelection {
    pub path: PathBuf,
    pub display_name: String,
    pub file: std::fs::File,
}

#[derive(Debug)]
struct BoundExternalSelection {
    scope: MemorySelectionScope,
    expires_at: Instant,
    source: Arc<BoundPath>,
}

#[derive(Debug)]
struct PreparedDiscovery {
    items: Vec<PreparedItem>,
}

#[derive(Debug)]
struct PreparedItem {
    source: BoundPath,
    summary: PreparedSummary,
}

#[derive(Debug)]
enum PreparedSummary {
    Mcp {
        source: String,
        server_name: String,
        transport: Option<String>,
    },
    Session {
        agent: String,
        session_id: String,
        pid: Option<u32>,
        liveness: String,
        workspace_label: Option<String>,
        name: Option<String>,
        started_at: Option<String>,
    },
}

#[derive(Debug)]
struct BoundPath {
    path: PathBuf,
    identity: PathIdentity,
    held: std::fs::File,
    display_name: String,
    directory: bool,
}

impl BoundPath {
    fn verify_current(&self) -> Result<(), String> {
        if PathIdentity::from_file(&self.held, self.directory)? != self.identity {
            return Err("held external source identity changed".to_owned());
        }
        let current = open_absolute(&self.path, self.directory)?;
        if PathIdentity::from_file(&current, self.directory)? != self.identity {
            return Err("external source was replaced".to_owned());
        }
        Ok(())
    }

    fn open_current(&self) -> Result<std::fs::File, String> {
        let current = open_absolute(&self.path, self.directory)?;
        if PathIdentity::from_file(&current, self.directory)? != self.identity {
            return Err("external source was replaced".to_owned());
        }
        Ok(current)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PathIdentity {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

impl PathIdentity {
    fn from_file(file: &std::fs::File, directory: bool) -> Result<Self, String> {
        use std::os::unix::fs::MetadataExt;
        let metadata = file
            .metadata()
            .map_err(|error| format!("inspect external source: {error}"))?;
        if directory != metadata.is_dir() || (!directory && !metadata.is_file()) {
            return Err("external source type changed".to_owned());
        }
        if directory {
            return Ok(Self {
                device: metadata.dev(),
                inode: metadata.ino(),
                size: 0,
                modified_seconds: 0,
                modified_nanoseconds: 0,
                changed_seconds: 0,
                changed_nanoseconds: 0,
            });
        }
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.size(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanoseconds: metadata.ctime_nsec(),
        })
    }
}

fn prepare(
    home: &Path,
    servers: Vec<crate::mcp::DiscoveredMcpServer>,
    sessions: Vec<ExternalSession>,
) -> PreparedDiscovery {
    let mut items = Vec::new();
    for server in servers {
        if items.len() >= MAX_EXTERNAL_ITEMS {
            break;
        }
        let Ok(source) = bind_path(&server.config_path) else {
            continue;
        };
        items.push(PreparedItem {
            source,
            summary: PreparedSummary::Mcp {
                source: format!("{:?}", server.source_app).replace("Desktop", " Desktop"),
                server_name: bound_text(&server.server_name, 256),
                transport: server.transport.map(|value| bound_text(&value, 128)),
            },
        });
    }
    for session in sessions {
        if items.len() >= MAX_EXTERNAL_ITEMS {
            break;
        }
        let Ok(source) = bind_path(&session.source_path) else {
            continue;
        };
        items.push(PreparedItem {
            source,
            summary: PreparedSummary::Session {
                agent: match session.agent {
                    ExternalAgent::Claude => "claude",
                    ExternalAgent::Copilot => "copilot",
                }
                .to_owned(),
                session_id: bound_text(&session.session_id, 256),
                pid: session.pid,
                liveness: match session.liveness {
                    LivenessStatus::Unknown => "unknown",
                    LivenessStatus::Alive => "alive",
                    LivenessStatus::Dead => "dead",
                }
                .to_owned(),
                workspace_label: session
                    .cwd
                    .as_deref()
                    .map(Path::new)
                    .and_then(Path::file_name)
                    .and_then(std::ffi::OsStr::to_str)
                    .map(|value| bound_text(value, 512)),
                name: session.name.map(|value| bound_text(&value, 512)),
                started_at: session.started_at.map(|value| bound_text(&value, 128)),
            },
        });
    }
    let _ = home;
    PreparedDiscovery { items }
}

fn bind_path(path: &Path) -> Result<BoundPath, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("inspect external source: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("external source must not be a symlink".to_owned());
    }
    let directory = metadata.is_dir();
    if !directory && !metadata.is_file() {
        return Err("external source must be a file or directory".to_owned());
    }
    let path = path
        .canonicalize()
        .map_err(|error| format!("canonicalize external source: {error}"))?;
    let held = open_absolute(&path, directory)?;
    let identity = PathIdentity::from_file(&held, directory)?;
    let display_name = path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .map_or_else(|| "Source".to_owned(), |value| bound_text(value, 512));
    Ok(BoundPath {
        path,
        identity,
        held,
        display_name,
        directory,
    })
}

fn open_absolute(path: &Path, directory: bool) -> Result<std::fs::File, String> {
    use rustix::fs::{Mode, OFlags};
    if !path.is_absolute() {
        return Err("external source path must be absolute".to_owned());
    }
    let mut current =
        std::fs::File::open("/").map_err(|error| format!("open filesystem root: {error}"))?;
    let components = path.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => {
                let last = index + 1 == components.len();
                let mut flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
                if !last || directory {
                    flags |= OFlags::DIRECTORY;
                } else {
                    flags |= OFlags::NONBLOCK;
                }
                let fd = rustix::fs::openat(&current, *name, flags, Mode::empty())
                    .map_err(|error| format!("open external source: {error}"))?;
                current = std::fs::File::from(fd);
            }
            _ => return Err("external source path is not canonical".to_owned()),
        }
    }
    Ok(current)
}

fn parse_token(value: &str) -> Result<uuid::Uuid, String> {
    let token =
        uuid::Uuid::parse_str(value).map_err(|_| "invalid external source selection".to_owned())?;
    if token.get_version_num() != 7 || token.hyphenated().to_string() != value {
        return Err("invalid external source selection".to_owned());
    }
    Ok(token)
}

fn bound_text(value: &str, maximum: usize) -> String {
    let end = value.floor_char_boundary(value.len().min(maximum));
    value[..end]
        .chars()
        .filter(|character| !character.is_control())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> MemorySelectionScope {
        MemorySelectionScope {
            account_user_id: Some("account".to_owned()),
            account_epoch: 9,
        }
    }

    #[test]
    fn replacement_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source.json");
        std::fs::write(&path, "{}").unwrap();
        let source = bind_path(&path).unwrap();
        std::fs::rename(&path, directory.path().join("old.json")).unwrap();
        std::fs::write(&path, "{}").unwrap();
        let error = source.verify_current().unwrap_err();
        assert!(
            error.contains("replaced") || error.contains("identity changed"),
            "{error}"
        );
    }

    #[test]
    fn symlink_source_is_rejected() {
        use std::os::unix::fs::symlink;
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target.json");
        let link = directory.path().join("source.json");
        std::fs::write(&target, "{}").unwrap();
        symlink(target, &link).unwrap();
        assert!(bind_path(&link).unwrap_err().contains("symlink"));
    }

    #[test]
    fn selection_is_account_bound_and_one_shot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source.json");
        std::fs::write(&path, "{}").unwrap();
        let source = bind_path(&path).unwrap();
        let token = uuid::Uuid::now_v7();
        let mut registry = BoundExternalRegistry::default();
        registry.insertion_order.push_back(token);
        registry.selections.insert(
            token,
            BoundExternalSelection {
                scope: scope(),
                expires_at: Instant::now() + Duration::from_mins(1),
                source: Arc::new(source),
            },
        );
        assert!(registry.take(&token.to_string(), &scope()).is_ok());
        assert!(registry.take(&token.to_string(), &scope()).is_err());
    }

    #[test]
    fn directories_are_revealable_but_not_advertised_as_openable() {
        let directory = tempfile::tempdir().unwrap();
        let source_directory = directory.path().join("session");
        std::fs::create_dir(&source_directory).unwrap();
        let prepared = PreparedDiscovery {
            items: vec![PreparedItem {
                source: bind_path(&source_directory).unwrap(),
                summary: PreparedSummary::Session {
                    agent: "copilot".to_owned(),
                    session_id: "session".to_owned(),
                    pid: None,
                    liveness: "unknown".to_owned(),
                    workspace_label: None,
                    name: None,
                    started_at: None,
                },
            }],
        };
        let mut registry = BoundExternalRegistry::default();
        let discovery = registry
            .install(prepared, &scope(), Instant::now())
            .unwrap();
        let session = discovery.sessions.first().unwrap();
        assert!(!session.can_open_source);
        assert!(session.can_copy_source_path);
        assert!(session.can_reveal_source);
    }

    #[test]
    fn cap_plus_one_is_atomic_and_expiry_is_one_shot() {
        let directory = tempfile::tempdir().unwrap();
        let old_path = directory.path().join("old.json");
        let first_path = directory.path().join("first.json");
        let second_path = directory.path().join("second.json");
        for path in [&old_path, &first_path, &second_path] {
            std::fs::write(path, "{}").unwrap();
        }
        let old_token = uuid::Uuid::now_v7();
        let mut registry = BoundExternalRegistry::with_limits(1, Duration::from_mins(1));
        registry.insertion_order.push_back(old_token);
        registry.selections.insert(
            old_token,
            BoundExternalSelection {
                scope: scope(),
                expires_at: Instant::now() + Duration::from_mins(1),
                source: Arc::new(bind_path(&old_path).unwrap()),
            },
        );
        let prepared = PreparedDiscovery {
            items: [&first_path, &second_path]
                .into_iter()
                .map(|path| PreparedItem {
                    source: bind_path(path).unwrap(),
                    summary: PreparedSummary::Mcp {
                        source: "fixture".to_owned(),
                        server_name: path.file_stem().unwrap().to_string_lossy().into_owned(),
                        transport: None,
                    },
                })
                .collect(),
        };
        let error = registry
            .install(prepared, &scope(), Instant::now())
            .unwrap_err();
        assert!(error.contains("capacity"), "{error}");
        assert!(registry.take(&old_token.to_string(), &scope()).is_ok());

        let token = uuid::Uuid::now_v7();
        let mut expired = BoundExternalRegistry::with_limits(1, Duration::ZERO);
        expired.insertion_order.push_back(token);
        expired.selections.insert(
            token,
            BoundExternalSelection {
                scope: scope(),
                expires_at: Instant::now(),
                source: Arc::new(bind_path(&first_path).unwrap()),
            },
        );
        assert!(
            expired
                .take(&token.to_string(), &scope())
                .unwrap_err()
                .contains("expired")
        );
        assert!(
            expired
                .take(&token.to_string(), &scope())
                .unwrap_err()
                .contains("stale")
        );
    }

    #[test]
    fn idle_expiry_purge_drops_external_authority_without_followup_operation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source.json");
        std::fs::write(&path, "{}").unwrap();
        let token = uuid::Uuid::now_v7();
        let mut registry = BoundExternalRegistry::with_limits(1, Duration::ZERO);
        registry.insertion_order.push_back(token);
        registry.selections.insert(
            token,
            BoundExternalSelection {
                scope: scope(),
                expires_at: Instant::now(),
                source: Arc::new(bind_path(&path).unwrap()),
            },
        );
        registry.purge_expired_now();
        assert!(registry.selections.is_empty());
        assert!(registry.insertion_order.is_empty());
    }
}

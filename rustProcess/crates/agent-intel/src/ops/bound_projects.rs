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
    copilot_repos,
    dto::{ClaudeProjectRef, RepositoryRow},
    projects,
    settings_tree::AgentSettingsTreeBundle,
};

const DEFAULT_SOURCE_CAPACITY: usize = 512;
const DEFAULT_SOURCE_TTL: Duration = Duration::from_mins(5);
const DEFAULT_PAGE_LIMIT: usize = 100;
const MAX_PAGE_LIMIT: usize = 256;
const DEFAULT_PAGE_BYTES: usize = 256 * 1024;
const MAX_PAGE_BYTES: usize = 512 * 1024;
const MAX_LABEL_BYTES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum BoundProjectSourceKind {
    Active,
    ClaudeArchive,
    CopilotArchive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoundProjectSourceSummary {
    pub selection_token: String,
    pub source_kind: BoundProjectSourceKind,
    pub agent: Option<String>,
    pub label: String,
    pub session_count: u64,
    pub memory_count: u32,
    pub active_session_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoundProjectSourcePage {
    pub items: Vec<BoundProjectSourceSummary>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub response_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoundProjectSessionSummary {
    pub session_id: String,
    pub runtime_incarnation_id: Option<String>,
    pub title: String,
    pub agent: String,
    pub status: String,
    pub mode: Option<String>,
    pub started_at: Option<String>,
    pub updated_at: Option<String>,
    pub size_bytes: Option<u64>,
    pub host_type: Option<String>,
    pub transcript_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoundProjectCustomizationSummary {
    pub kind: String,
    pub name: String,
    pub scope: String,
    pub enabled: bool,
    pub status: String,
    pub description: Option<String>,
    pub status_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoundProjectMemorySummary {
    pub read_selection_token: String,
    pub open_selection_token: String,
    pub copy_selection_token: String,
    pub filename: String,
    pub memory_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoundProjectCustomAgentSummary {
    pub detail_selection_token: String,
    pub open_selection_token: String,
    pub target: String,
    pub name: String,
    pub description: String,
    pub model: Option<String>,
    pub tools: Vec<String>,
    pub error_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoundProjectAgentSettings {
    pub agent: String,
    pub settings: AgentSettingsTreeBundle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoundProjectSnapshot {
    pub source_selection_token: String,
    pub source_kind: BoundProjectSourceKind,
    pub agent: Option<String>,
    pub label: String,
    pub sessions: Vec<BoundProjectSessionSummary>,
    pub memories: Vec<BoundProjectMemorySummary>,
    pub custom_agents: Vec<BoundProjectCustomAgentSummary>,
    pub customizations: Vec<BoundProjectCustomizationSummary>,
    pub settings: Vec<BoundProjectAgentSettings>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveProjectSource {
    pub working_directory: String,
    pub session_count: u64,
    pub session_ids: Vec<String>,
}

#[derive(Debug)]
pub struct ResolvedProjectSource {
    pub source_kind: BoundProjectSourceKind,
    pub agent: Option<String>,
    pub label: String,
    pub locator: ProjectSourceLocator,
    pub next_selection_token: String,
    source: Arc<BoundSource>,
}

#[derive(Debug)]
pub struct StagedProjectResolution {
    pub resolved: ResolvedProjectSource,
    original_token: uuid::Uuid,
    replacement_token: uuid::Uuid,
    scope: MemorySelectionScope,
    expires_at: Instant,
}

impl ResolvedProjectSource {
    pub fn verify_current(&self) -> Result<(), String> {
        self.source.authority.verify_current()
    }

    pub fn memory_destination(&self) -> Result<ResolvedMemoryDestination, String> {
        self.verify_current()?;
        let authority = self
            .source
            .memory_destination
            .as_ref()
            .ok_or_else(|| "selected project has no Claude memory destination".to_owned())?;
        authority.verify_current()?;
        Ok(ResolvedMemoryDestination {
            project_slug: match &self.locator {
                ProjectSourceLocator::Active { canonical_cwd } => {
                    crate::claude::ClaudeCodeProvider::encode_project_path(
                        &canonical_cwd.to_string_lossy(),
                    )
                }
                ProjectSourceLocator::ClaudeArchive { project_slug } => project_slug.clone(),
                ProjectSourceLocator::CopilotArchive { .. } => {
                    return Err("Claude memory cannot be copied to a Copilot archive".to_owned());
                }
            },
            project_path: authority.project.path.clone(),
            project_parent: authority
                .project
                .parent
                .try_clone()
                .map_err(|error| format!("retain selected project parent: {error}"))?,
            project_name: authority.project.entry_name.clone(),
            project_identity: authority.project.identity,
            project_directory: authority
                .project
                .held
                .try_clone()
                .map_err(|error| format!("retain selected project destination: {error}"))?,
            memory_identity: authority.memory_identity,
            memory_directory: authority
                .memory
                .try_clone()
                .map_err(|error| format!("retain selected memory destination: {error}"))?,
        })
    }

    pub fn held_projection_path(&self) -> Result<PathBuf, String> {
        match &self.source.authority {
            SourceAuthority::Directory(authority) => held_fd_path(&authority.held),
            SourceAuthority::CopilotDatabase(authority) => held_fd_path(&authority.held),
        }
    }

    pub fn held_directory(&self) -> Result<std::fs::File, String> {
        match &self.source.authority {
            SourceAuthority::Directory(authority) => authority
                .held
                .try_clone()
                .map_err(|error| format!("retain held project directory: {error}")),
            SourceAuthority::CopilotDatabase(_) => {
                Err("selected project source is not a directory".to_owned())
            }
        }
    }
}

#[derive(Debug)]
pub struct ResolvedMemoryDestination {
    pub project_slug: String,
    pub(crate) project_path: PathBuf,
    pub(crate) project_parent: std::fs::File,
    pub(crate) project_name: std::ffi::OsString,
    pub(crate) project_identity: FsIdentity,
    pub(crate) project_directory: std::fs::File,
    pub(crate) memory_identity: FsIdentity,
    pub(crate) memory_directory: std::fs::File,
}

impl ResolvedMemoryDestination {
    pub(crate) fn verify_current(&self) -> Result<(), String> {
        let project = DirectoryAuthority {
            path: self.project_path.clone(),
            parent: self
                .project_parent
                .try_clone()
                .map_err(|error| format!("retain selected project parent: {error}"))?,
            entry_name: self.project_name.clone(),
            identity: self.project_identity,
            mutation_stamp: None,
            held: self
                .project_directory
                .try_clone()
                .map_err(|error| format!("retain selected project destination: {error}"))?,
        };
        project.verify_current()?;
        if FsIdentity::directory(&self.memory_directory)? != self.memory_identity {
            return Err("held project memory destination identity changed".to_owned());
        }
        let current = open_directory_at(&self.project_directory, "memory")?;
        if FsIdentity::directory(&current)? != self.memory_identity {
            return Err("project memory destination was replaced".to_owned());
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn from_directory(project_slug: String, path: &Path) -> Result<Self, String> {
        let project_directory = open_absolute_directory(path)?;
        let memory_directory = open_directory_at(&project_directory, "memory")?;
        let authority = directory_authority_unstamped(path.to_owned(), project_directory)?;
        Ok(Self {
            project_slug,
            project_path: path.to_owned(),
            project_parent: authority
                .parent
                .try_clone()
                .map_err(|error| format!("retain selected project parent: {error}"))?,
            project_name: authority.entry_name.clone(),
            project_identity: authority.identity,
            project_directory: authority.held,
            memory_identity: FsIdentity::directory(&memory_directory)?,
            memory_directory,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectSourceLocator {
    Active { canonical_cwd: PathBuf },
    ClaudeArchive { project_slug: String },
    CopilotArchive { repository: String },
}

#[derive(Debug)]
pub struct BoundProjectSourceRegistry {
    selections: HashMap<uuid::Uuid, BoundSourceSelection>,
    insertion_order: VecDeque<uuid::Uuid>,
    capacity: usize,
    ttl: Duration,
}

impl Default for BoundProjectSourceRegistry {
    fn default() -> Self {
        Self {
            selections: HashMap::new(),
            insertion_order: VecDeque::new(),
            capacity: DEFAULT_SOURCE_CAPACITY,
            ttl: DEFAULT_SOURCE_TTL,
        }
    }
}

impl BoundProjectSourceRegistry {
    pub async fn list(
        &mut self,
        home: &Path,
        active_sources: Vec<ActiveProjectSource>,
        cursor: Option<&str>,
        limit: Option<usize>,
        max_bytes: Option<usize>,
        scope: MemorySelectionScope,
    ) -> Result<BoundProjectSourcePage, String> {
        let offset = parse_cursor(cursor)?;
        let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT).clamp(1, MAX_PAGE_LIMIT);
        let max_bytes = max_bytes
            .unwrap_or(DEFAULT_PAGE_BYTES)
            .clamp(1024, MAX_PAGE_BYTES);
        let claude_projects = projects::list_claude_projects(home).await?;
        let copilot_repositories = copilot_repos::list_repositories(home).await?;
        let home = home.to_owned();
        let prepared = tokio::task::spawn_blocking(move || {
            prepare_sources(
                &home,
                active_sources,
                claude_projects,
                copilot_repositories,
                offset,
                limit,
            )
        })
        .await
        .map_err(|error| format!("bound project source task join: {error}"))??;
        self.install_page(
            prepared.sources,
            offset,
            prepared.available,
            max_bytes,
            &scope,
            Instant::now(),
        )
    }

    pub fn resolve(
        &mut self,
        selection_token: &str,
        scope: &MemorySelectionScope,
    ) -> Result<ResolvedProjectSource, String> {
        let staged = self.stage_resolve(selection_token, scope)?;
        self.commit_resolution(&staged);
        Ok(staged.resolved)
    }

    pub fn stage_resolve(
        &self,
        selection_token: &str,
        scope: &MemorySelectionScope,
    ) -> Result<StagedProjectResolution, String> {
        let token = parse_token(selection_token)?;
        let now = Instant::now();
        let selection = self
            .selections
            .get(&token)
            .ok_or_else(|| "project source selection is stale or already consumed".to_owned())?;
        if now >= selection.expires_at {
            return Err("project source selection expired".to_owned());
        }
        if selection.scope != *scope {
            return Err(
                "project source selection does not belong to the active account".to_owned(),
            );
        }
        selection.source.authority.verify_current()?;
        let replacement_token = uuid::Uuid::now_v7();
        Ok(StagedProjectResolution {
            resolved: ResolvedProjectSource {
                source_kind: selection.source.source_kind,
                agent: selection.source.agent.clone(),
                label: selection.source.label.clone(),
                locator: selection.source.locator.clone(),
                next_selection_token: replacement_token.to_string(),
                source: Arc::clone(&selection.source),
            },
            original_token: token,
            replacement_token,
            scope: scope.clone(),
            expires_at: now + self.ttl,
        })
    }

    pub fn commit_resolution(&mut self, staged: &StagedProjectResolution) {
        self.selections.remove(&staged.original_token);
        self.insertion_order
            .retain(|candidate| *candidate != staged.original_token);
        self.insertion_order.push_back(staged.replacement_token);
        self.selections.insert(
            staged.replacement_token,
            BoundSourceSelection {
                scope: staged.scope.clone(),
                expires_at: staged.expires_at,
                source: Arc::clone(&staged.resolved.source),
            },
        );
    }

    pub fn consume_resolution(&mut self, staged: &StagedProjectResolution) {
        self.selections.remove(&staged.original_token);
        self.insertion_order
            .retain(|candidate| *candidate != staged.original_token);
    }

    pub fn clear(&mut self) {
        self.selections.clear();
        self.insertion_order.clear();
    }

    pub fn purge_expired_now(&mut self) {
        self.purge_expired(Instant::now());
    }

    fn install_page(
        &mut self,
        prepared: Vec<PreparedSource>,
        offset: usize,
        available: usize,
        max_bytes: usize,
        scope: &MemorySelectionScope,
        now: Instant,
    ) -> Result<BoundProjectSourcePage, String> {
        let mut items = Vec::new();
        let mut planned = Vec::new();
        for source in prepared {
            let source = Arc::new(source.into_bound());
            let token = uuid::Uuid::now_v7();
            let summary = BoundProjectSourceSummary {
                selection_token: token.to_string(),
                source_kind: source.source_kind,
                agent: source.agent.clone(),
                label: source.label.clone(),
                session_count: source.session_count,
                memory_count: source.memory_count,
                active_session_ids: source.active_session_ids.clone(),
            };
            let mut trial_items = items.clone();
            trial_items.push(summary.clone());
            let trial_consumed = trial_items.len();
            let trial_has_more = trial_consumed < available;
            let trial_cursor = trial_has_more.then(|| (offset + trial_consumed).to_string());
            let trial_page = finalized_page(trial_items, trial_cursor, trial_has_more)?;
            if trial_page.response_bytes > max_bytes {
                if items.is_empty() {
                    return Err("project source page exceeds requested byte bound".to_owned());
                }
                break;
            }
            items.push(summary);
            planned.push((
                token,
                BoundSourceSelection {
                    scope: scope.clone(),
                    expires_at: now + self.ttl,
                    source,
                },
            ));
        }
        let consumed = items.len();
        let has_more = consumed < available;
        let next_cursor = has_more.then(|| (offset + consumed).to_string());
        let page = finalized_page(items, next_cursor, has_more)?;
        if page.response_bytes > max_bytes {
            return Err("project source page exceeds requested byte bound".to_owned());
        }
        if planned.len() > self.capacity {
            return Err("project source page exceeds capability capacity".to_owned());
        }
        self.purge_expired(now);
        self.evict_for(planned.len());
        for (token, selection) in planned {
            self.insertion_order.push_back(token);
            self.selections.insert(token, selection);
        }
        Ok(page)
    }

    #[cfg(test)]
    fn install_selection(
        &mut self,
        source: Arc<BoundSource>,
        scope: MemorySelectionScope,
        now: Instant,
    ) -> String {
        while self.selections.len() >= self.capacity {
            let Some(oldest) = self.insertion_order.pop_front() else {
                break;
            };
            self.selections.remove(&oldest);
        }
        let token = uuid::Uuid::now_v7();
        self.insertion_order.push_back(token);
        self.selections.insert(
            token,
            BoundSourceSelection {
                scope,
                expires_at: now + self.ttl,
                source,
            },
        );
        token.to_string()
    }

    fn purge_expired(&mut self, now: Instant) {
        self.selections
            .retain(|_, selection| selection.expires_at > now);
        self.insertion_order
            .retain(|token| self.selections.contains_key(token));
    }

    fn evict_for(&mut self, required: usize) {
        while self.selections.len().saturating_add(required) > self.capacity {
            let Some(oldest) = self.insertion_order.pop_front() else {
                self.selections.clear();
                break;
            };
            self.selections.remove(&oldest);
        }
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
struct BoundSourceSelection {
    scope: MemorySelectionScope,
    expires_at: Instant,
    source: Arc<BoundSource>,
}

#[derive(Debug)]
struct BoundSource {
    source_kind: BoundProjectSourceKind,
    agent: Option<String>,
    label: String,
    session_count: u64,
    memory_count: u32,
    active_session_ids: Vec<String>,
    locator: ProjectSourceLocator,
    authority: SourceAuthority,
    memory_destination: Option<MemoryDestinationAuthority>,
}

#[derive(Debug)]
struct PreparedSource {
    source_kind: BoundProjectSourceKind,
    agent: Option<String>,
    label: String,
    session_count: u64,
    memory_count: u32,
    active_session_ids: Vec<String>,
    locator: ProjectSourceLocator,
    authority: SourceAuthority,
    memory_destination: Option<MemoryDestinationAuthority>,
}

struct PreparedSourcePage {
    sources: Vec<PreparedSource>,
    available: usize,
}

struct UnboundSource {
    source_kind: BoundProjectSourceKind,
    agent: Option<String>,
    label: String,
    session_count: u64,
    memory_count: u32,
    active_session_ids: Vec<String>,
    locator: ProjectSourceLocator,
}

impl PreparedSource {
    fn into_bound(self) -> BoundSource {
        BoundSource {
            source_kind: self.source_kind,
            agent: self.agent,
            label: self.label,
            session_count: self.session_count,
            memory_count: self.memory_count,
            active_session_ids: self.active_session_ids,
            locator: self.locator,
            authority: self.authority,
            memory_destination: self.memory_destination,
        }
    }
}

#[derive(Debug)]
enum SourceAuthority {
    Directory(DirectoryAuthority),
    CopilotDatabase(CopilotDatabaseAuthority),
}

impl SourceAuthority {
    fn verify_current(&self) -> Result<(), String> {
        match self {
            Self::Directory(authority) => authority.verify_current(),
            Self::CopilotDatabase(authority) => authority.verify_current(),
        }
    }
}

#[derive(Debug)]
struct DirectoryAuthority {
    path: PathBuf,
    parent: std::fs::File,
    entry_name: std::ffi::OsString,
    identity: FsIdentity,
    mutation_stamp: Option<DirectoryMutationStamp>,
    held: std::fs::File,
}

#[derive(Debug)]
struct MemoryDestinationAuthority {
    project: DirectoryAuthority,
    memory_identity: FsIdentity,
    memory: std::fs::File,
}

impl MemoryDestinationAuthority {
    fn verify_current(&self) -> Result<(), String> {
        self.project.verify_current()?;
        if FsIdentity::directory(&self.memory)? != self.memory_identity {
            return Err("held project memory destination identity changed".to_owned());
        }
        let current = open_directory_at(&self.project.held, "memory")?;
        if FsIdentity::directory(&current)? != self.memory_identity {
            return Err("project memory destination was replaced".to_owned());
        }
        Ok(())
    }
}

impl DirectoryAuthority {
    fn verify_current(&self) -> Result<(), String> {
        if FsIdentity::directory(&self.held)? != self.identity {
            return Err("held project source identity changed".to_owned());
        }
        if self
            .mutation_stamp
            .is_some_and(|stamp| DirectoryMutationStamp::from_file(&self.held) != Ok(stamp))
        {
            return Err("held project source changed during projection".to_owned());
        }
        let current = open_directory_at(&self.parent, &self.entry_name)?;
        if FsIdentity::directory(&current)? != self.identity {
            return Err("project source was replaced".to_owned());
        }
        if self
            .mutation_stamp
            .is_some_and(|stamp| DirectoryMutationStamp::from_file(&current) != Ok(stamp))
        {
            return Err("project source changed during projection".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug)]
struct CopilotDatabaseAuthority {
    parent: std::fs::File,
    entry_name: std::ffi::OsString,
    identity: FsIdentity,
    held: std::fs::File,
}

impl CopilotDatabaseAuthority {
    fn verify_current(&self) -> Result<(), String> {
        if FsIdentity::regular(&self.held)? != self.identity {
            return Err("held Copilot repository catalog identity changed".to_owned());
        }
        let current = open_regular_at(&self.parent, &self.entry_name)?;
        if FsIdentity::regular(&current)? != self.identity {
            return Err("Copilot repository catalog was replaced".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FsIdentity {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

impl FsIdentity {
    fn directory(file: &std::fs::File) -> Result<Self, String> {
        use std::os::unix::fs::MetadataExt;

        let metadata = file
            .metadata()
            .map_err(|error| format!("inspect project source: {error}"))?;
        if !metadata.is_dir() {
            return Err("project source must be a directory".to_owned());
        }
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: 0,
            modified_seconds: 0,
            modified_nanoseconds: 0,
            changed_seconds: 0,
            changed_nanoseconds: 0,
        })
    }

    fn regular(file: &std::fs::File) -> Result<Self, String> {
        let identity = Self::from_file(file)?;
        if !file
            .metadata()
            .map_err(|error| format!("inspect project source: {error}"))?
            .is_file()
        {
            return Err("project source must be a regular file".to_owned());
        }
        Ok(identity)
    }

    fn from_file(file: &std::fs::File) -> Result<Self, String> {
        use std::os::unix::fs::MetadataExt;
        let metadata = file
            .metadata()
            .map_err(|error| format!("inspect project source: {error}"))?;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DirectoryMutationStamp {
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

impl DirectoryMutationStamp {
    fn from_file(file: &std::fs::File) -> Result<Self, String> {
        use std::os::unix::fs::MetadataExt as _;

        let metadata = file
            .metadata()
            .map_err(|error| format!("inspect project source mutation stamp: {error}"))?;
        Ok(Self {
            size: metadata.size(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanoseconds: metadata.ctime_nsec(),
        })
    }
}

fn prepare_sources(
    home: &Path,
    active_sources: Vec<ActiveProjectSource>,
    claude_projects: Vec<ClaudeProjectRef>,
    copilot_repositories: Vec<RepositoryRow>,
    offset: usize,
    limit: usize,
) -> Result<PreparedSourcePage, String> {
    let mut unbound = Vec::new();
    let mut active_paths = std::collections::BTreeSet::new();
    for active in active_sources {
        let Ok(canonical) = super::path_safety::canonicalize_user_dir(
            "workingDirectory",
            &active.working_directory,
        ) else {
            continue;
        };
        if !active_paths.insert(canonical.clone()) {
            continue;
        }
        unbound.push(UnboundSource {
            source_kind: BoundProjectSourceKind::Active,
            agent: None,
            label: directory_label(&canonical),
            session_count: active.session_count,
            memory_count: 0,
            active_session_ids: active.session_ids,
            locator: ProjectSourceLocator::Active {
                canonical_cwd: canonical,
            },
        });
    }
    let claude_root = home.join(".claude").join("projects");
    for project in claude_projects {
        let path = claude_root.join(&project.slug);
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            continue;
        }
        unbound.push(UnboundSource {
            source_kind: BoundProjectSourceKind::ClaudeArchive,
            agent: Some("claude".to_owned()),
            label: sanitize_archive_label(&project.label, &project.slug),
            session_count: u64::from(project.session_count),
            memory_count: project.memory_count,
            active_session_ids: Vec::new(),
            locator: ProjectSourceLocator::ClaudeArchive {
                project_slug: project.slug,
            },
        });
    }
    for repository in copilot_repositories {
        unbound.push(UnboundSource {
            source_kind: BoundProjectSourceKind::CopilotArchive,
            agent: Some("copilot".to_owned()),
            label: sanitize_archive_label(&repository.repository, "Copilot repository"),
            session_count: repository.session_count,
            memory_count: 0,
            active_session_ids: Vec::new(),
            locator: ProjectSourceLocator::CopilotArchive {
                repository: repository.repository,
            },
        });
    }
    unbound.sort_by(|left, right| {
        source_sort_rank(left.source_kind)
            .cmp(&source_sort_rank(right.source_kind))
            .then(
                left.label
                    .to_ascii_lowercase()
                    .cmp(&right.label.to_ascii_lowercase()),
            )
            .then(left.label.cmp(&right.label))
    });
    if offset > unbound.len() {
        return Err("project source cursor is stale".to_owned());
    }
    let available = unbound.len().saturating_sub(offset);
    let copilot_path = home.join(".copilot").join("session-store.db");
    let mut copilot_held: Option<std::fs::File> = None;
    let mut prepared = Vec::with_capacity(limit.min(available));
    for source in unbound.into_iter().skip(offset).take(limit) {
        let (authority, memory_destination) = match &source.locator {
            ProjectSourceLocator::Active { canonical_cwd } => {
                let held = open_absolute_directory(canonical_cwd)?;
                (
                    SourceAuthority::Directory(directory_authority(canonical_cwd.clone(), held)?),
                    prepare_active_memory_destination(home, canonical_cwd)?,
                )
            }
            ProjectSourceLocator::ClaudeArchive { project_slug } => {
                let path = claude_root.join(project_slug);
                let held = open_absolute_directory(&path)?;
                (
                    SourceAuthority::Directory(directory_authority(
                        path.clone(),
                        held.try_clone()
                            .map_err(|error| format!("retain Claude archive source: {error}"))?,
                    )?),
                    prepare_memory_destination(path, held)?,
                )
            }
            ProjectSourceLocator::CopilotArchive { .. } => {
                let held = if let Some(held) = copilot_held.as_ref() {
                    held.try_clone()
                        .map_err(|error| format!("retain Copilot repository catalog: {error}"))?
                } else {
                    let held = open_absolute_regular(&copilot_path)?;
                    let selected = held
                        .try_clone()
                        .map_err(|error| format!("retain Copilot repository catalog: {error}"))?;
                    copilot_held = Some(held);
                    selected
                };
                (
                    SourceAuthority::CopilotDatabase(copilot_database_authority(
                        &copilot_path,
                        held,
                    )?),
                    None,
                )
            }
        };
        prepared.push(PreparedSource {
            source_kind: source.source_kind,
            agent: source.agent,
            label: source.label,
            session_count: source.session_count,
            memory_count: source.memory_count,
            active_session_ids: source.active_session_ids,
            locator: source.locator,
            authority,
            memory_destination,
        });
    }
    Ok(PreparedSourcePage {
        sources: prepared,
        available,
    })
}

fn prepare_active_memory_destination(
    home: &Path,
    canonical: &Path,
) -> Result<Option<MemoryDestinationAuthority>, String> {
    let slug = crate::claude::ClaudeCodeProvider::encode_project_path(&canonical.to_string_lossy());
    let path = home.join(".claude").join("projects").join(slug);
    match open_absolute_directory(&path) {
        Ok(held) => prepare_memory_destination(path, held),
        Err(error) if error.contains("No such file") || error.contains("no such file") => Ok(None),
        Err(error) => Err(error),
    }
}

fn prepare_memory_destination(
    path: PathBuf,
    held: std::fs::File,
) -> Result<Option<MemoryDestinationAuthority>, String> {
    let memory = match open_directory_at_raw(&held, "memory") {
        Ok(memory) => memory,
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(error) => return Err(format!("open project memory destination: {error}")),
    };
    Ok(Some(MemoryDestinationAuthority {
        project: directory_authority_unstamped(path, held)?,
        memory_identity: FsIdentity::directory(&memory)?,
        memory,
    }))
}

fn source_sort_rank(kind: BoundProjectSourceKind) -> u8 {
    match kind {
        BoundProjectSourceKind::Active => 0,
        BoundProjectSourceKind::ClaudeArchive => 1,
        BoundProjectSourceKind::CopilotArchive => 2,
    }
}

fn directory_label(path: &Path) -> String {
    path.file_name()
        .and_then(std::ffi::OsStr::to_str)
        .filter(|value| !value.is_empty())
        .map_or_else(|| "Project".to_owned(), bound_label)
}

fn sanitize_archive_label(value: &str, fallback: &str) -> String {
    let candidate = if value.starts_with('/') || value.starts_with("~/") {
        Path::new(value)
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or(fallback)
    } else {
        value
    };
    let candidate = candidate.trim();
    if candidate.is_empty() {
        bound_label(fallback)
    } else {
        bound_label(candidate)
    }
}

fn bound_label(value: &str) -> String {
    let mut end = value.len().min(MAX_LABEL_BYTES);
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    value[..end]
        .chars()
        .filter(|character| !character.is_control())
        .collect()
}

fn finalized_page(
    items: Vec<BoundProjectSourceSummary>,
    next_cursor: Option<String>,
    has_more: bool,
) -> Result<BoundProjectSourcePage, String> {
    let mut page = BoundProjectSourcePage {
        items,
        next_cursor,
        has_more,
        response_bytes: 0,
    };
    loop {
        let encoded = serde_json::to_vec(&page)
            .map_err(|error| format!("encode project source page: {error}"))?
            .len();
        if encoded == page.response_bytes {
            return Ok(page);
        }
        page.response_bytes = encoded;
    }
}

fn parse_cursor(cursor: Option<&str>) -> Result<usize, String> {
    match cursor {
        None => Ok(0),
        Some(value) if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) => {
            value
                .parse()
                .map_err(|_| "invalid project source cursor".to_owned())
        }
        Some(_) => Err("invalid project source cursor".to_owned()),
    }
}

fn parse_token(value: &str) -> Result<uuid::Uuid, String> {
    let token =
        uuid::Uuid::parse_str(value).map_err(|_| "invalid project source selection".to_owned())?;
    if token.get_version_num() != 7 || token.hyphenated().to_string() != value {
        return Err("invalid project source selection".to_owned());
    }
    Ok(token)
}

fn open_absolute_directory(path: &Path) -> Result<std::fs::File, String> {
    open_absolute(path, true)
}

#[cfg(target_os = "linux")]
fn held_fd_path(file: &std::fs::File) -> Result<PathBuf, String> {
    use std::os::fd::AsRawFd as _;

    let path = PathBuf::from(format!(
        "/proc/{}/fd/{}",
        std::process::id(),
        file.as_raw_fd()
    ));
    std::fs::metadata(&path)
        .map_err(|error| format!("resolve held project authority path: {error}"))?;
    Ok(path)
}

#[cfg(target_os = "macos")]
fn held_fd_path(file: &std::fs::File) -> Result<PathBuf, String> {
    use std::os::fd::AsRawFd as _;

    let path = PathBuf::from(format!("/dev/fd/{}", file.as_raw_fd()));
    std::fs::metadata(&path)
        .map_err(|error| format!("resolve held project authority path: {error}"))?;
    Ok(path)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn held_fd_path(_file: &std::fs::File) -> Result<PathBuf, String> {
    Err("held project authority paths are unavailable on this platform".to_owned())
}

fn open_directory_at(
    parent: &std::fs::File,
    name: impl AsRef<std::ffi::OsStr>,
) -> Result<std::fs::File, String> {
    open_directory_at_raw(parent, name)
        .map_err(|error| format!("open project source directory: {error}"))
}

fn open_directory_at_raw(
    parent: &std::fs::File,
    name: impl AsRef<std::ffi::OsStr>,
) -> Result<std::fs::File, rustix::io::Errno> {
    use rustix::fs::{Mode, OFlags};

    let name = name.as_ref();
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(rustix::io::Errno::INVAL);
    }
    let fd = rustix::fs::openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    Ok(std::fs::File::from(fd))
}

fn open_regular_at(
    parent: &std::fs::File,
    name: impl AsRef<std::ffi::OsStr>,
) -> Result<std::fs::File, String> {
    use rustix::fs::{Mode, OFlags};

    let name = name.as_ref();
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err("project source file name is invalid".to_owned());
    }
    let fd = rustix::fs::openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(|error| format!("open project source file: {error}"))?;
    Ok(std::fs::File::from(fd))
}

fn directory_authority(path: PathBuf, held: std::fs::File) -> Result<DirectoryAuthority, String> {
    directory_authority_with_stamp(path, held, true)
}

fn directory_authority_unstamped(
    path: PathBuf,
    held: std::fs::File,
) -> Result<DirectoryAuthority, String> {
    directory_authority_with_stamp(path, held, false)
}

fn directory_authority_with_stamp(
    path: PathBuf,
    held: std::fs::File,
    track_mutations: bool,
) -> Result<DirectoryAuthority, String> {
    let entry_name = path
        .file_name()
        .ok_or_else(|| "project source must have a parent entry".to_owned())?
        .to_owned();
    let parent_path = path
        .parent()
        .ok_or_else(|| "project source must have a parent directory".to_owned())?;
    let parent = open_absolute_directory(parent_path)?;
    let identity = FsIdentity::directory(&held)?;
    let visible = open_directory_at(&parent, &entry_name)?;
    if FsIdentity::directory(&visible)? != identity {
        return Err("project source changed while authority was prepared".to_owned());
    }
    Ok(DirectoryAuthority {
        path,
        parent,
        entry_name,
        identity,
        mutation_stamp: track_mutations
            .then(|| DirectoryMutationStamp::from_file(&held))
            .transpose()?,
        held,
    })
}

fn copilot_database_authority(
    path: &Path,
    held: std::fs::File,
) -> Result<CopilotDatabaseAuthority, String> {
    let entry_name = path
        .file_name()
        .ok_or_else(|| "Copilot repository catalog must have a parent entry".to_owned())?
        .to_owned();
    let parent_path = path
        .parent()
        .ok_or_else(|| "Copilot repository catalog must have a parent directory".to_owned())?;
    let parent = open_absolute_directory(parent_path)?;
    let identity = FsIdentity::regular(&held)?;
    let visible = open_regular_at(&parent, &entry_name)?;
    if FsIdentity::regular(&visible)? != identity {
        return Err("Copilot repository catalog changed while authority was prepared".to_owned());
    }
    Ok(CopilotDatabaseAuthority {
        parent,
        entry_name,
        identity,
        held,
    })
}

fn open_absolute_regular(path: &Path) -> Result<std::fs::File, String> {
    open_absolute(path, false)
}

fn open_absolute(path: &Path, directory: bool) -> Result<std::fs::File, String> {
    use rustix::fs::{Mode, OFlags};
    if !path.is_absolute() {
        return Err("project source path must be absolute".to_owned());
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
                    .map_err(|error| format!("open project source: {error}"))?;
                current = std::fs::File::from(fd);
            }
            _ => return Err("project source path is not canonical".to_owned()),
        }
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> MemorySelectionScope {
        MemorySelectionScope {
            account_user_id: Some("account".to_owned()),
            account_epoch: 7,
        }
    }

    fn prepared_source(path: &Path, label: &str) -> PreparedSource {
        let held = open_absolute_directory(path).unwrap();
        PreparedSource {
            source_kind: BoundProjectSourceKind::Active,
            agent: None,
            label: label.to_owned(),
            session_count: 0,
            memory_count: 0,
            active_session_ids: Vec::new(),
            locator: ProjectSourceLocator::Active {
                canonical_cwd: path.to_owned(),
            },
            authority: SourceAuthority::Directory(
                directory_authority(path.to_owned(), held).unwrap(),
            ),
            memory_destination: None,
        }
    }

    #[cfg(target_os = "linux")]
    fn open_fd_count_for(path: &Path) -> usize {
        std::fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|entry| std::fs::read_link(entry.path()).ok())
            .filter(|target| target == path)
            .count()
    }

    #[test]
    fn source_selection_is_one_shot_and_account_bound() {
        let directory = tempfile::tempdir().unwrap();
        let held = open_absolute_directory(directory.path()).unwrap();
        let source = Arc::new(BoundSource {
            source_kind: BoundProjectSourceKind::Active,
            agent: None,
            label: "project".to_owned(),
            session_count: 0,
            memory_count: 0,
            active_session_ids: Vec::new(),
            locator: ProjectSourceLocator::Active {
                canonical_cwd: directory.path().to_owned(),
            },
            authority: SourceAuthority::Directory(
                directory_authority(directory.path().to_owned(), held).unwrap(),
            ),
            memory_destination: None,
        });
        let mut registry = BoundProjectSourceRegistry::with_limits(2, Duration::from_mins(1));
        let token = registry.install_selection(source, scope(), Instant::now());
        let resolved = registry.resolve(&token, &scope()).unwrap();
        assert_ne!(resolved.next_selection_token, token);
        assert!(registry.resolve(&token, &scope()).is_err());
    }

    #[test]
    fn staged_resolution_is_not_consumed_before_commit() {
        let directory = tempfile::tempdir().unwrap();
        let source = Arc::new(prepared_source(directory.path(), "project").into_bound());
        let mut registry = BoundProjectSourceRegistry::default();
        let token = registry.install_selection(source, scope(), Instant::now());
        let staged = registry.stage_resolve(&token, &scope()).unwrap();
        drop(staged);
        assert!(registry.resolve(&token, &scope()).is_ok());
    }

    #[test]
    fn mutation_destination_consumption_does_not_publish_hidden_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let source = Arc::new(prepared_source(directory.path(), "project").into_bound());
        let mut registry = BoundProjectSourceRegistry::default();
        let token = registry.install_selection(source, scope(), Instant::now());
        let staged = registry.stage_resolve(&token, &scope()).unwrap();
        registry.consume_resolution(&staged);
        assert!(registry.selections.is_empty());
        assert!(registry.insertion_order.is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn page_late_size_failure_preserves_registry_and_fd_count() {
        let existing_dir = tempfile::tempdir().unwrap();
        let candidate_dir = tempfile::tempdir().unwrap();
        let mut registry = BoundProjectSourceRegistry::with_limits(4, Duration::from_mins(1));
        let existing = Arc::new(prepared_source(existing_dir.path(), "existing").into_bound());
        let existing_token = registry.install_selection(existing, scope(), Instant::now());
        let before_tokens = registry.selections.keys().copied().collect::<Vec<_>>();
        let before_order = registry.insertion_order.clone();
        let before_fds = open_fd_count_for(candidate_dir.path());

        let error = registry
            .install_page(
                vec![prepared_source(candidate_dir.path(), "candidate")],
                0,
                1,
                1,
                &scope(),
                Instant::now(),
            )
            .unwrap_err();
        assert!(error.contains("byte bound"), "{error}");
        assert_eq!(
            registry.selections.keys().copied().collect::<Vec<_>>(),
            before_tokens
        );
        assert_eq!(registry.insertion_order, before_order);
        assert!(
            registry
                .selections
                .contains_key(&parse_token(&existing_token).unwrap())
        );
        assert_eq!(open_fd_count_for(candidate_dir.path()), before_fds);
    }

    #[test]
    fn page_capacity_preflight_never_returns_evicted_tokens() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let mut registry = BoundProjectSourceRegistry::with_limits(1, Duration::from_mins(1));
        let error = registry
            .install_page(
                vec![
                    prepared_source(first.path(), "first"),
                    prepared_source(second.path(), "second"),
                ],
                0,
                2,
                MAX_PAGE_BYTES,
                &scope(),
                Instant::now(),
            )
            .unwrap_err();
        assert!(error.contains("capacity"), "{error}");
        assert!(registry.selections.is_empty());
        assert!(registry.insertion_order.is_empty());
    }

    #[test]
    fn replacement_directory_invalidates_selection() {
        let parent = tempfile::tempdir().unwrap();
        let path = parent.path().join("project");
        std::fs::create_dir(&path).unwrap();
        let held = open_absolute_directory(&path).unwrap();
        let source = Arc::new(BoundSource {
            source_kind: BoundProjectSourceKind::Active,
            agent: None,
            label: "project".to_owned(),
            session_count: 0,
            memory_count: 0,
            active_session_ids: Vec::new(),
            locator: ProjectSourceLocator::Active {
                canonical_cwd: path.clone(),
            },
            authority: SourceAuthority::Directory(directory_authority(path.clone(), held).unwrap()),
            memory_destination: None,
        });
        let mut registry = BoundProjectSourceRegistry::default();
        let token = registry.install_selection(source, scope(), Instant::now());
        std::fs::rename(&path, parent.path().join("retired")).unwrap();
        std::fs::create_dir(&path).unwrap();
        let error = registry.resolve(&token, &scope()).unwrap_err();
        assert!(
            error.contains("replaced") || error.contains("changed"),
            "{error}"
        );
    }

    #[test]
    fn resolved_source_retains_authority_after_token_consumption() {
        let parent = tempfile::tempdir().unwrap();
        let path = parent.path().join("project");
        std::fs::create_dir(&path).unwrap();
        let held = open_absolute_directory(&path).unwrap();
        let source = Arc::new(BoundSource {
            source_kind: BoundProjectSourceKind::Active,
            agent: None,
            label: "project".to_owned(),
            session_count: 0,
            memory_count: 0,
            active_session_ids: Vec::new(),
            locator: ProjectSourceLocator::Active {
                canonical_cwd: path.clone(),
            },
            authority: SourceAuthority::Directory(directory_authority(path.clone(), held).unwrap()),
            memory_destination: None,
        });
        let mut registry = BoundProjectSourceRegistry::default();
        let token = registry.install_selection(source, scope(), Instant::now());
        let resolved = registry.resolve(&token, &scope()).unwrap();
        std::fs::rename(&path, parent.path().join("retired")).unwrap();
        std::fs::create_dir(&path).unwrap();
        let error = resolved.verify_current().unwrap_err();
        assert!(
            error.contains("replaced") || error.contains("changed"),
            "{error}"
        );
        assert!(registry.resolve(&token, &scope()).is_err());
    }

    #[test]
    fn expired_selection_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let held = open_absolute_directory(directory.path()).unwrap();
        let source = Arc::new(BoundSource {
            source_kind: BoundProjectSourceKind::Active,
            agent: None,
            label: "project".to_owned(),
            session_count: 0,
            memory_count: 0,
            active_session_ids: Vec::new(),
            locator: ProjectSourceLocator::Active {
                canonical_cwd: directory.path().to_owned(),
            },
            authority: SourceAuthority::Directory(
                directory_authority(directory.path().to_owned(), held).unwrap(),
            ),
            memory_destination: None,
        });
        let mut registry = BoundProjectSourceRegistry::with_limits(1, Duration::ZERO);
        let token = registry.install_selection(source, scope(), Instant::now());
        assert!(registry.resolve(&token, &scope()).is_err());
    }

    #[test]
    fn idle_expiry_purge_drops_project_authority_without_followup_operation() {
        let directory = tempfile::tempdir().unwrap();
        let source = Arc::new(prepared_source(directory.path(), "project").into_bound());
        let mut registry = BoundProjectSourceRegistry::with_limits(1, Duration::ZERO);
        registry.install_selection(source, scope(), Instant::now());
        registry.purge_expired_now();
        assert!(registry.selections.is_empty());
        assert!(registry.insertion_order.is_empty());
    }
}

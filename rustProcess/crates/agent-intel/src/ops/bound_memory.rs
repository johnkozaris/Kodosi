use std::{
    collections::{HashMap, VecDeque},
    io::Read,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{
    bound_projects::ResolvedMemoryDestination,
    project_mutations::{
        ProjectMemoryCopyOutcome, ProjectMemoryCopyReceipt, ProjectMutationLedger,
    },
};
use super::{io::MAX_AGENT_INTEL_FILE_BYTES, path_safety};

const MAX_DISCOVERED_FILES: usize = 4096;
pub const MAX_BOUND_MEMORY_ITEMS: usize = 512;
const PROJECT_SNAPSHOT_CAPABILITIES_PER_ITEM: usize = 3;
const DEFAULT_SELECTION_CAPACITY: usize =
    MAX_BOUND_MEMORY_ITEMS * PROJECT_SNAPSHOT_CAPABILITIES_PER_ITEM;
const DEFAULT_SELECTION_TTL: Duration = Duration::from_mins(5);
const MAX_MEMORY_FRONTMATTER_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemorySelectionScope {
    pub account_user_id: Option<String>,
    pub account_epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoundClaudeMemoryList {
    pub canonical_cwd: String,
    pub project_slug: String,
    pub items: Vec<BoundClaudeMemoryItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoundClaudeMemoryItem {
    pub selection_token: String,
    pub filename: String,
    pub memory_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundClaudeMemoryCapabilityItem {
    pub read_selection_token: String,
    pub open_selection_token: String,
    pub copy_selection_token: String,
    pub filename: String,
    pub memory_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoundClaudeMemoryRead {
    pub content: String,
    pub selection_token: String,
    pub canonical_cwd: String,
    pub project_slug: String,
    pub filename: String,
    pub memory_type: Option<String>,
}

#[derive(Debug)]
pub struct BoundClaudeMemoryRegistry {
    selections: HashMap<uuid::Uuid, BoundSelection>,
    insertion_order: VecDeque<uuid::Uuid>,
    capacity: usize,
    ttl: Duration,
}

struct InstalledItem {
    tokens: Vec<String>,
    filename: String,
    memory_type: Option<String>,
}

#[derive(Debug)]
pub struct StagedMemoryCapabilities {
    items: Vec<BoundClaudeMemoryCapabilityItem>,
    selections: Vec<(uuid::Uuid, BoundSelection)>,
}

impl StagedMemoryCapabilities {
    pub fn items(&self) -> &[BoundClaudeMemoryCapabilityItem] {
        &self.items
    }

    fn required(&self) -> usize {
        self.selections.len()
    }
}

impl Default for BoundClaudeMemoryRegistry {
    fn default() -> Self {
        Self {
            selections: HashMap::new(),
            insertion_order: VecDeque::new(),
            capacity: DEFAULT_SELECTION_CAPACITY,
            ttl: DEFAULT_SELECTION_TTL,
        }
    }
}

impl BoundClaudeMemoryRegistry {
    pub async fn list(
        &mut self,
        home: &Path,
        cwd: &str,
        scope: MemorySelectionScope,
    ) -> Result<BoundClaudeMemoryList, String> {
        let home = home.to_owned();
        let cwd = cwd.to_owned();
        let item_limit = self.capacity.min(MAX_BOUND_MEMORY_ITEMS);
        let prepared = tokio::task::spawn_blocking(move || prepare_list(&home, &cwd, item_limit))
            .await
            .map_err(|error| format!("bound memory list task join: {error}"))??;
        self.install_single(prepared, &scope, Instant::now())
    }

    pub async fn list_archive(
        &mut self,
        home: &Path,
        project_slug: &str,
        scope: MemorySelectionScope,
    ) -> Result<BoundClaudeMemoryList, String> {
        let home = home.to_owned();
        let project_slug = project_slug.to_owned();
        let item_limit = self.capacity.min(MAX_BOUND_MEMORY_ITEMS);
        let prepared = tokio::task::spawn_blocking(move || {
            prepare_archive_list(&home, &project_slug, item_limit)
        })
        .await
        .map_err(|error| format!("bound archive memory list task join: {error}"))??;
        self.install_single(prepared, &scope, Instant::now())
    }

    pub async fn list_capabilities(
        &mut self,
        home: &Path,
        cwd: &str,
        scope: MemorySelectionScope,
    ) -> Result<Vec<BoundClaudeMemoryCapabilityItem>, String> {
        let home = home.to_owned();
        let cwd = cwd.to_owned();
        let prepared =
            tokio::task::spawn_blocking(move || prepare_list(&home, &cwd, MAX_BOUND_MEMORY_ITEMS))
                .await
                .map_err(|error| format!("bound memory capability task join: {error}"))??;
        let staged = self.stage_capabilities_prepared(prepared, &scope, Instant::now())?;
        let items = staged.items.clone();
        self.commit_staged_capabilities(staged);
        Ok(items)
    }

    pub async fn list_archive_capabilities(
        &mut self,
        home: &Path,
        project_slug: &str,
        scope: MemorySelectionScope,
    ) -> Result<Vec<BoundClaudeMemoryCapabilityItem>, String> {
        let home = home.to_owned();
        let project_slug = project_slug.to_owned();
        let prepared = tokio::task::spawn_blocking(move || {
            prepare_archive_list(&home, &project_slug, MAX_BOUND_MEMORY_ITEMS)
        })
        .await
        .map_err(|error| format!("bound archive memory capability task join: {error}"))??;
        let staged = self.stage_capabilities_prepared(prepared, &scope, Instant::now())?;
        let items = staged.items.clone();
        self.commit_staged_capabilities(staged);
        Ok(items)
    }

    pub async fn stage_capabilities(
        &self,
        home: &Path,
        cwd: &str,
        scope: MemorySelectionScope,
    ) -> Result<StagedMemoryCapabilities, String> {
        let home = home.to_owned();
        let cwd = cwd.to_owned();
        let prepared =
            tokio::task::spawn_blocking(move || prepare_list(&home, &cwd, MAX_BOUND_MEMORY_ITEMS))
                .await
                .map_err(|error| format!("bound memory capability task join: {error}"))??;
        self.stage_capabilities_prepared(prepared, &scope, Instant::now())
    }

    pub async fn stage_capabilities_from_root(
        &self,
        home: &Path,
        canonical_cwd: &Path,
        cwd_directory: &std::fs::File,
        scope: MemorySelectionScope,
    ) -> Result<StagedMemoryCapabilities, String> {
        let home = home.to_owned();
        let canonical_cwd = canonical_cwd.to_owned();
        let cwd_directory = cwd_directory
            .try_clone()
            .map_err(|error| format!("retain project root for memory capabilities: {error}"))?;
        let prepared = tokio::task::spawn_blocking(move || {
            prepare_list_from_root(&home, &canonical_cwd, cwd_directory, MAX_BOUND_MEMORY_ITEMS)
        })
        .await
        .map_err(|error| format!("bound memory capability task join: {error}"))??;
        self.stage_capabilities_prepared(prepared, &scope, Instant::now())
    }

    pub async fn stage_archive_capabilities(
        &self,
        home: &Path,
        project_slug: &str,
        scope: MemorySelectionScope,
    ) -> Result<StagedMemoryCapabilities, String> {
        let home = home.to_owned();
        let project_slug = project_slug.to_owned();
        let prepared = tokio::task::spawn_blocking(move || {
            prepare_archive_list(&home, &project_slug, MAX_BOUND_MEMORY_ITEMS)
        })
        .await
        .map_err(|error| format!("bound archive memory capability task join: {error}"))??;
        self.stage_capabilities_prepared(prepared, &scope, Instant::now())
    }

    pub fn preflight_staged_capabilities(
        &self,
        staged: &StagedMemoryCapabilities,
    ) -> Result<(), String> {
        if staged.required() > self.capacity {
            return Err("memory snapshot exceeds capability capacity".to_owned());
        }
        Ok(())
    }

    pub fn validate_staged_capabilities(
        &self,
        staged: &StagedMemoryCapabilities,
    ) -> Result<(), String> {
        for (_, selection) in &staged.selections {
            selection.verify_current()?;
        }
        Ok(())
    }

    pub fn commit_staged_capabilities(&mut self, staged: StagedMemoryCapabilities) {
        self.purge_expired(Instant::now());
        self.evict_for(staged.required());
        for (token, selection) in staged.selections {
            self.insertion_order.push_back(token);
            self.selections.insert(token, selection);
        }
    }

    pub async fn read(
        &mut self,
        selection_token: &str,
        scope: &MemorySelectionScope,
    ) -> Result<BoundClaudeMemoryRead, String> {
        let selection = self.take(selection_token, scope, Instant::now())?;
        tokio::task::spawn_blocking(move || selection.read())
            .await
            .map_err(|error| format!("bound memory read task join: {error}"))?
    }

    pub async fn open(
        &mut self,
        selection_token: &str,
        scope: &MemorySelectionScope,
    ) -> Result<(std::fs::File, String), String> {
        let selection = self.take(selection_token, scope, Instant::now())?;
        tokio::task::spawn_blocking(move || selection.open_file())
            .await
            .map_err(|error| format!("bound memory open task join: {error}"))?
    }

    pub async fn copy_to_project(
        &mut self,
        selection_token: &str,
        destination: ResolvedMemoryDestination,
        mutation_id: &str,
        target_label: String,
        ledger: ProjectMutationLedger,
        scope: &MemorySelectionScope,
    ) -> Result<ProjectMemoryCopyReceipt, String> {
        require_single_component(&destination.project_slug)?;
        let selection = self.take(selection_token, scope, Instant::now())?;
        if selection.authority.project_slug == destination.project_slug {
            return Err("source and target projects are the same".to_owned());
        }
        let mutation_id = mutation_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let read = selection.read()?;
            Self::copy_into_destination(
                &destination,
                &mutation_id,
                target_label,
                &read.filename,
                read.content.as_bytes(),
                &ledger,
            )
        })
        .await
        .map_err(|error| format!("bound memory copy task join: {error}"))?
    }

    fn copy_into_destination(
        destination: &ResolvedMemoryDestination,
        mutation_id: &str,
        target_label: String,
        filename: &str,
        content: &[u8],
        ledger: &ProjectMutationLedger,
    ) -> Result<ProjectMemoryCopyReceipt, String> {
        use rustix::fs::{Mode, OFlags, RenameFlags};
        use std::io::Write as _;

        destination.verify_current()?;
        let memory_dir = destination
            .memory_directory
            .try_clone()
            .map_err(|error| format!("retain project memory destination: {error}"))?;
        let temporary_name = format!(".kodosi-copy-{mutation_id}");
        require_single_component(&temporary_name)?;
        let temporary_fd = rustix::fs::openat(
            &memory_dir,
            &temporary_name,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|error| format!("create temporary memory copy: {error}"))?;
        let mut temporary = std::fs::File::from(temporary_fd);
        if let Err(error) = temporary
            .write_all(content)
            .and_then(|()| temporary.sync_all())
        {
            return Self::cleanup_copy_failure(
                &memory_dir,
                &temporary_name,
                mutation_id,
                filename,
                target_label,
                ledger,
                format!("write temporary memory copy: {error}"),
            );
        }
        if let Err(error) = destination.verify_current() {
            return Self::cleanup_copy_failure(
                &memory_dir,
                &temporary_name,
                mutation_id,
                filename,
                target_label,
                ledger,
                error,
            );
        }
        match rustix::fs::renameat_with(
            &memory_dir,
            &temporary_name,
            &memory_dir,
            filename,
            RenameFlags::NOREPLACE,
        ) {
            Ok(()) => {
                let receipt = ledger.record(
                    mutation_id,
                    ProjectMemoryCopyOutcome::Copied,
                    filename.to_owned(),
                    target_label,
                    None,
                )?;
                if let Err(error) = memory_dir.sync_all() {
                    return ledger.mark_indeterminate(
                        mutation_id,
                        format!("memory copy committed but directory sync failed: {error}"),
                    );
                }
                if let Err(error) = destination.verify_current() {
                    return ledger.mark_indeterminate(
                        mutation_id,
                        format!("memory copy committed but destination identity changed: {error}"),
                    );
                }
                Ok(receipt)
            }
            Err(rustix::io::Errno::EXIST) => {
                let receipt = ledger.record(
                    mutation_id,
                    ProjectMemoryCopyOutcome::AlreadyExists,
                    filename.to_owned(),
                    target_label,
                    None,
                )?;
                if let Err(error) =
                    rustix::fs::unlinkat(&memory_dir, &temporary_name, rustix::fs::AtFlags::empty())
                {
                    return ledger.mark_indeterminate(
                        mutation_id,
                        format!("target existed but temporary cleanup failed: {error}"),
                    );
                }
                if let Err(error) = open_regular_at(&memory_dir, filename) {
                    return ledger.mark_indeterminate(
                        mutation_id,
                        format!("target existed but its file type was invalid: {error}"),
                    );
                }
                if let Err(error) = destination.verify_current() {
                    return ledger.mark_indeterminate(
                        mutation_id,
                        format!("target existed but destination identity changed: {error}"),
                    );
                }
                Ok(receipt)
            }
            Err(error) => Self::cleanup_copy_failure(
                &memory_dir,
                &temporary_name,
                mutation_id,
                filename,
                target_label,
                ledger,
                format!("commit memory copy: {error}"),
            ),
        }
    }

    fn cleanup_copy_failure(
        memory_dir: &std::fs::File,
        temporary_name: &str,
        mutation_id: &str,
        filename: &str,
        target_label: String,
        ledger: &ProjectMutationLedger,
        error: String,
    ) -> Result<ProjectMemoryCopyReceipt, String> {
        match rustix::fs::unlinkat(memory_dir, temporary_name, rustix::fs::AtFlags::empty()) {
            Ok(()) | Err(rustix::io::Errno::NOENT) => Err(error),
            Err(cleanup_error) => ledger.record(
                mutation_id,
                ProjectMemoryCopyOutcome::Indeterminate,
                filename.to_owned(),
                target_label,
                Some(format!(
                    "{error}; temporary cleanup failed: {cleanup_error}"
                )),
            ),
        }
    }

    pub fn clear(&mut self) {
        self.selections.clear();
        self.insertion_order.clear();
    }

    pub fn purge_expired_now(&mut self) {
        self.purge_expired(Instant::now());
    }

    fn install_single(
        &mut self,
        prepared: PreparedList,
        scope: &MemorySelectionScope,
        now: Instant,
    ) -> Result<BoundClaudeMemoryList, String> {
        let canonical_cwd = prepared.canonical_cwd.clone();
        let project_slug = prepared.project_slug.clone();
        let installed = self.install(prepared, scope, now, 1)?;
        Ok(BoundClaudeMemoryList {
            canonical_cwd,
            project_slug,
            items: installed
                .into_iter()
                .map(|item| BoundClaudeMemoryItem {
                    selection_token: item.tokens[0].clone(),
                    filename: item.filename,
                    memory_type: item.memory_type,
                })
                .collect(),
        })
    }

    fn stage_capabilities_prepared(
        &self,
        prepared: PreparedList,
        scope: &MemorySelectionScope,
        now: Instant,
    ) -> Result<StagedMemoryCapabilities, String> {
        let (installed, selections) =
            self.stage_install(prepared, scope, now, PROJECT_SNAPSHOT_CAPABILITIES_PER_ITEM)?;
        let items = installed
            .into_iter()
            .map(|item| BoundClaudeMemoryCapabilityItem {
                read_selection_token: item.tokens[0].clone(),
                open_selection_token: item.tokens[1].clone(),
                copy_selection_token: item.tokens[2].clone(),
                filename: item.filename,
                memory_type: item.memory_type,
            })
            .collect();
        Ok(StagedMemoryCapabilities { items, selections })
    }

    fn install(
        &mut self,
        prepared: PreparedList,
        scope: &MemorySelectionScope,
        now: Instant,
        capabilities_per_item: usize,
    ) -> Result<Vec<InstalledItem>, String> {
        let (installed, selections) =
            self.stage_install(prepared, scope, now, capabilities_per_item)?;
        self.purge_expired(now);
        self.evict_for(selections.len());
        for (token, selection) in selections {
            self.insertion_order.push_back(token);
            self.selections.insert(token, selection);
        }
        Ok(installed)
    }

    fn stage_install(
        &self,
        prepared: PreparedList,
        scope: &MemorySelectionScope,
        now: Instant,
        capabilities_per_item: usize,
    ) -> Result<(Vec<InstalledItem>, Vec<(uuid::Uuid, BoundSelection)>), String> {
        if capabilities_per_item == 0 {
            return Err("memory capability count must be positive".to_owned());
        }
        if prepared.files.is_empty() || prepared.authority.is_none() {
            return Ok((Vec::new(), Vec::new()));
        }
        let Some(authority) = prepared.authority else {
            return Ok((Vec::new(), Vec::new()));
        };
        let required = prepared
            .files
            .len()
            .checked_mul(capabilities_per_item)
            .ok_or_else(|| "memory capability reservation overflow".to_owned())?;
        if required > self.capacity {
            return Err("memory snapshot exceeds capability capacity".to_owned());
        }
        let mut installed = Vec::with_capacity(prepared.files.len());
        let mut selections = Vec::with_capacity(required);
        let mut prepared_capabilities = (0..required).map(|_| uuid::Uuid::now_v7());
        for file in prepared.files {
            let filename = file.filename.clone();
            let memory_type = file.memory_type.clone();
            let file = Arc::new(file);
            let mut tokens = Vec::with_capacity(capabilities_per_item);
            for _ in 0..capabilities_per_item {
                let Some(token) = prepared_capabilities.next() else {
                    return Err("memory capability reservation is inconsistent".to_owned());
                };
                selections.push((
                    token,
                    BoundSelection {
                        token,
                        scope: scope.clone(),
                        expires_at: now + self.ttl,
                        authority: Arc::clone(&authority),
                        file: Arc::clone(&file),
                    },
                ));
                tokens.push(token.to_string());
            }
            installed.push(InstalledItem {
                tokens,
                filename,
                memory_type,
            });
        }
        Ok((installed, selections))
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

    fn take(
        &mut self,
        selection_token: &str,
        scope: &MemorySelectionScope,
        now: Instant,
    ) -> Result<BoundSelection, String> {
        let token = parse_token(selection_token)?;
        let selection = self
            .selections
            .remove(&token)
            .ok_or_else(|| "memory selection is stale or already consumed".to_owned())?;
        self.insertion_order.retain(|candidate| *candidate != token);
        if now >= selection.expires_at {
            return Err("memory selection expired".to_owned());
        }
        if selection.scope != *scope {
            return Err("memory selection does not belong to the active account".to_owned());
        }
        Ok(selection)
    }

    fn purge_expired(&mut self, now: Instant) {
        self.selections
            .retain(|_, selection| selection.expires_at > now);
        self.insertion_order
            .retain(|token| self.selections.contains_key(token));
    }

    #[cfg(test)]
    fn with_limits(capacity: usize, ttl: Duration) -> Self {
        assert!(capacity > 0);
        Self {
            selections: HashMap::new(),
            insertion_order: VecDeque::new(),
            capacity,
            ttl,
        }
    }
}

fn parse_token(value: &str) -> Result<uuid::Uuid, String> {
    let token =
        uuid::Uuid::parse_str(value).map_err(|_| "invalid memory selection token".to_owned())?;
    if token.get_version_num() != 7 || token.hyphenated().to_string() != value {
        return Err("invalid memory selection token".to_owned());
    }
    Ok(token)
}

#[derive(Debug)]
struct PreparedList {
    canonical_cwd: String,
    project_slug: String,
    authority: Option<Arc<ProjectAuthority>>,
    files: Vec<PreparedFile>,
}

#[derive(Debug)]
struct PreparedFile {
    filename: String,
    memory_type: Option<String>,
    identity: FileIdentity,
    held_file: std::fs::File,
}

#[derive(Debug)]
struct BoundSelection {
    token: uuid::Uuid,
    scope: MemorySelectionScope,
    expires_at: Instant,
    authority: Arc<ProjectAuthority>,
    file: Arc<PreparedFile>,
}

impl BoundSelection {
    fn verify_current(&self) -> Result<(), String> {
        self.authority.verify_current()?;
        let current = open_regular_at(&self.authority.memory_dir, &self.file.filename)?;
        if FileIdentity::from_file(&current)? != self.file.identity
            || FileIdentity::from_file(&self.file.held_file)? != self.file.identity
        {
            return Err("memory capability no longer names the staged file".to_owned());
        }
        self.authority.verify_current()
    }

    fn open_file(self) -> Result<(std::fs::File, String), String> {
        self.authority.verify_current()?;
        let current = open_regular_at(&self.authority.memory_dir, &self.file.filename)?;
        if FileIdentity::from_file(&current)? != self.file.identity {
            return Err("memory selection no longer names the listed file".to_owned());
        }
        if FileIdentity::from_file(&self.file.held_file)? != self.file.identity {
            return Err("memory selection file identity changed".to_owned());
        }
        self.authority.verify_current()?;
        Ok((current, self.file.filename.clone()))
    }

    fn read(self) -> Result<BoundClaudeMemoryRead, String> {
        self.authority.verify_current()?;
        let mut current = open_regular_at(&self.authority.memory_dir, &self.file.filename)?;
        let identity = FileIdentity::from_file(&current)?;
        if identity != self.file.identity {
            return Err("memory selection no longer names the listed file".to_owned());
        }
        let held_identity = FileIdentity::from_file(&self.file.held_file)?;
        if held_identity != self.file.identity {
            return Err("memory selection file identity changed".to_owned());
        }

        let mut bytes = Vec::new();
        current
            .by_ref()
            .take(MAX_AGENT_INTEL_FILE_BYTES.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| format!("read selected memory file: {error}"))?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_AGENT_INTEL_FILE_BYTES {
            return Err(format!(
                "file too large (> {MAX_AGENT_INTEL_FILE_BYTES} bytes)"
            ));
        }
        if FileIdentity::from_file(&current)? != self.file.identity {
            return Err("memory selection changed while it was read".to_owned());
        }
        self.authority.verify_current()?;
        let content = String::from_utf8(bytes)
            .map_err(|error| format!("file is not valid UTF-8: {error}"))?;
        Ok(BoundClaudeMemoryRead {
            content,
            selection_token: self.token.to_string(),
            canonical_cwd: self.authority.canonical_cwd.clone(),
            project_slug: self.authority.project_slug.clone(),
            filename: self.file.filename.clone(),
            memory_type: self.file.memory_type.clone(),
        })
    }
}

#[derive(Debug)]
struct ProjectAuthority {
    canonical_cwd_path: Option<PathBuf>,
    canonical_cwd: String,
    cwd_identity: Option<DirectoryIdentity>,
    cwd_dir: Option<std::fs::File>,
    projects_root_path: PathBuf,
    projects_root_identity: DirectoryIdentity,
    projects_root: std::fs::File,
    project_slug: String,
    project_identity: DirectoryIdentity,
    project_dir: std::fs::File,
    memory_identity: DirectoryIdentity,
    memory_dir: std::fs::File,
}

impl ProjectAuthority {
    fn verify_current(&self) -> Result<(), String> {
        if let (Some(cwd_dir), Some(cwd_identity)) = (&self.cwd_dir, self.cwd_identity) {
            require_directory_identity("held project working directory", cwd_dir, cwd_identity)?;
        }
        require_directory_identity(
            "held Claude projects root",
            &self.projects_root,
            self.projects_root_identity,
        )?;
        require_directory_identity(
            "held Claude project directory",
            &self.project_dir,
            self.project_identity,
        )?;
        require_directory_identity(
            "held Claude memory directory",
            &self.memory_dir,
            self.memory_identity,
        )?;
        if let (Some(cwd_path), Some(cwd_identity)) = (&self.canonical_cwd_path, self.cwd_identity)
        {
            let cwd = open_absolute_directory(cwd_path)?;
            require_directory_identity("project working directory", &cwd, cwd_identity)?;
        }
        let projects_root = open_absolute_directory(&self.projects_root_path)?;
        require_directory_identity(
            "Claude projects root",
            &projects_root,
            self.projects_root_identity,
        )?;
        let project = open_directory_at(&projects_root, &self.project_slug)?;
        require_directory_identity("Claude project directory", &project, self.project_identity)?;
        let memory = open_directory_at(&project, "memory")?;
        require_directory_identity("Claude memory directory", &memory, self.memory_identity)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DirectoryIdentity {
    device: u64,
    inode: u64,
}

impl DirectoryIdentity {
    fn from_file(file: &std::fs::File) -> Result<Self, String> {
        use std::os::unix::fs::MetadataExt;

        let metadata = file
            .metadata()
            .map_err(|error| format!("inspect selected directory: {error}"))?;
        if !metadata.is_dir() {
            return Err("selected path is not a directory".to_owned());
        }
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

impl FileIdentity {
    fn from_file(file: &std::fs::File) -> Result<Self, String> {
        use std::os::unix::fs::MetadataExt;

        let metadata = file
            .metadata()
            .map_err(|error| format!("inspect selected memory file: {error}"))?;
        if !metadata.is_file() {
            return Err("selected memory path must be a regular file".to_owned());
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

fn prepare_list(home: &Path, cwd: &str, capacity: usize) -> Result<PreparedList, String> {
    let canonical_cwd_path = path_safety::canonicalize_user_dir("cwd", cwd)
        .map_err(|error| format!("invalid cwd: {error}"))?;
    let canonical_cwd = canonical_cwd_path.to_string_lossy().into_owned();
    let cwd_dir = open_absolute_directory(&canonical_cwd_path)?;
    prepare_list_with_root(
        home,
        Some(canonical_cwd_path),
        canonical_cwd,
        cwd_dir,
        capacity,
    )
}

fn prepare_list_from_root(
    home: &Path,
    canonical_cwd_path: &Path,
    cwd_dir: std::fs::File,
    capacity: usize,
) -> Result<PreparedList, String> {
    let canonical_cwd = canonical_cwd_path.to_string_lossy().into_owned();
    prepare_list_with_root(home, None, canonical_cwd, cwd_dir, capacity)
}

fn prepare_list_with_root(
    home: &Path,
    canonical_cwd_path: Option<PathBuf>,
    canonical_cwd: String,
    cwd_dir: std::fs::File,
    capacity: usize,
) -> Result<PreparedList, String> {
    let project_slug = crate::claude::ClaudeCodeProvider::encode_project_path(&canonical_cwd);
    require_single_component(&project_slug)?;
    let cwd_identity = DirectoryIdentity::from_file(&cwd_dir)?;

    let projects_root_input = home.join(".claude").join("projects");
    let projects_root_path = match projects_root_input.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(empty_prepared_list(canonical_cwd, project_slug));
        }
        Err(error) => return Err(format!("resolve Claude projects root: {error}")),
    };
    let projects_root = open_absolute_directory(&projects_root_path)?;
    let projects_root_identity = DirectoryIdentity::from_file(&projects_root)?;
    let project = match open_directory_at_raw(&projects_root, &project_slug) {
        Ok(file) => file,
        Err(rustix::io::Errno::NOENT) => {
            return Ok(empty_prepared_list(canonical_cwd, project_slug));
        }
        Err(error) => return Err(format!("open selected directory: {error}")),
    };
    let project_identity = DirectoryIdentity::from_file(&project)?;
    let memory_dir = match open_directory_at_raw(&project, "memory") {
        Ok(file) => file,
        Err(rustix::io::Errno::NOENT) => {
            return Ok(empty_prepared_list(canonical_cwd, project_slug));
        }
        Err(error) => return Err(format!("open selected directory: {error}")),
    };
    let memory_identity = DirectoryIdentity::from_file(&memory_dir)?;
    let authority = Arc::new(ProjectAuthority {
        canonical_cwd_path,
        canonical_cwd: canonical_cwd.clone(),
        cwd_identity: Some(cwd_identity),
        cwd_dir: Some(cwd_dir),
        projects_root_path,
        projects_root_identity,
        projects_root,
        project_slug: project_slug.clone(),
        project_identity,
        project_dir: project,
        memory_identity,
        memory_dir,
    });
    let mut files = list_files(&authority.memory_dir)?;
    files.sort_by(|left, right| {
        let left_index = left.filename == "MEMORY.md";
        let right_index = right.filename == "MEMORY.md";
        right_index
            .cmp(&left_index)
            .then(left.filename.cmp(&right.filename))
    });
    files.truncate(capacity);
    Ok(PreparedList {
        canonical_cwd,
        project_slug,
        authority: Some(authority),
        files,
    })
}

fn prepare_archive_list(
    home: &Path,
    project_slug: &str,
    capacity: usize,
) -> Result<PreparedList, String> {
    require_single_component(project_slug)?;
    let projects_root_input = home.join(".claude").join("projects");
    let projects_root_path = match projects_root_input.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(empty_prepared_list(String::new(), project_slug.to_owned()));
        }
        Err(error) => return Err(format!("resolve Claude projects root: {error}")),
    };
    let projects_root = open_absolute_directory(&projects_root_path)?;
    let projects_root_identity = DirectoryIdentity::from_file(&projects_root)?;
    let project = match open_directory_at_raw(&projects_root, project_slug) {
        Ok(file) => file,
        Err(rustix::io::Errno::NOENT) => {
            return Ok(empty_prepared_list(String::new(), project_slug.to_owned()));
        }
        Err(error) => return Err(format!("open selected archive project: {error}")),
    };
    let project_identity = DirectoryIdentity::from_file(&project)?;
    let memory_dir = match open_directory_at_raw(&project, "memory") {
        Ok(file) => file,
        Err(rustix::io::Errno::NOENT) => {
            return Ok(empty_prepared_list(String::new(), project_slug.to_owned()));
        }
        Err(error) => return Err(format!("open selected archive memory directory: {error}")),
    };
    let memory_identity = DirectoryIdentity::from_file(&memory_dir)?;
    let authority = Arc::new(ProjectAuthority {
        canonical_cwd_path: None,
        canonical_cwd: String::new(),
        cwd_identity: None,
        cwd_dir: None,
        projects_root_path,
        projects_root_identity,
        projects_root,
        project_slug: project_slug.to_owned(),
        project_identity,
        project_dir: project,
        memory_identity,
        memory_dir,
    });
    let mut files = list_files(&authority.memory_dir)?;
    files.sort_by(|left, right| {
        let left_index = left.filename == "MEMORY.md";
        let right_index = right.filename == "MEMORY.md";
        right_index
            .cmp(&left_index)
            .then(left.filename.cmp(&right.filename))
    });
    files.truncate(capacity);
    Ok(PreparedList {
        canonical_cwd: String::new(),
        project_slug: project_slug.to_owned(),
        authority: Some(authority),
        files,
    })
}

fn empty_prepared_list(canonical_cwd: String, project_slug: String) -> PreparedList {
    PreparedList {
        canonical_cwd,
        project_slug,
        authority: None,
        files: Vec::new(),
    }
}

fn list_files(memory_dir: &std::fs::File) -> Result<Vec<PreparedFile>, String> {
    let mut entries = rustix::fs::Dir::read_from(memory_dir)
        .map_err(|error| format!("list selected memory directory: {error}"))?;
    let mut files = Vec::new();
    for entry in entries.by_ref().take(MAX_DISCOVERED_FILES) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => return Err(format!("read selected memory directory entry: {error}")),
        };
        let bytes = entry.file_name().to_bytes();
        let Ok(filename) = std::str::from_utf8(bytes) else {
            continue;
        };
        if filename == "."
            || filename == ".."
            || Path::new(filename).extension() != Some(std::ffi::OsStr::new("md"))
        {
            continue;
        }
        let Ok(file) = open_regular_at(memory_dir, filename) else {
            continue;
        };
        let identity = FileIdentity::from_file(&file)?;
        let memory_type = read_frontmatter_type(&file);
        files.push(PreparedFile {
            filename: filename.to_owned(),
            memory_type,
            identity,
            held_file: file,
        });
    }
    Ok(files)
}

fn read_frontmatter_type(file: &std::fs::File) -> Option<String> {
    let mut file = file.try_clone().ok()?;
    let mut content = String::new();
    file.by_ref()
        .take(MAX_MEMORY_FRONTMATTER_BYTES.saturating_add(1))
        .read_to_string(&mut content)
        .ok()?;
    if u64::try_from(content.len()).unwrap_or(u64::MAX) > MAX_MEMORY_FRONTMATTER_BYTES {
        return None;
    }
    extract_frontmatter_type(&content)
}

fn extract_frontmatter_type(content: &str) -> Option<String> {
    if !content.starts_with("---") {
        return None;
    }
    let end = content[3..].find("---")?;
    let frontmatter = &content[3..3 + end];
    frontmatter.lines().find_map(|line| {
        line.trim()
            .strip_prefix("type:")
            .map(|value| value.trim().to_owned())
    })
}

fn open_absolute_directory(path: &Path) -> Result<std::fs::File, String> {
    if !path.is_absolute() {
        return Err("selected directory must be absolute".to_owned());
    }
    let mut current =
        std::fs::File::open("/").map_err(|error| format!("open filesystem root: {error}"))?;
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => {
                current = open_directory_at_os(&current, name)?;
            }
            _ => return Err("selected directory path is not canonical".to_owned()),
        }
    }
    Ok(current)
}

fn open_directory_at(parent: &std::fs::File, name: &str) -> Result<std::fs::File, String> {
    open_directory_at_raw(parent, name).map_err(|error| format!("open selected directory: {error}"))
}

fn open_directory_at_raw(
    parent: &std::fs::File,
    name: &str,
) -> Result<std::fs::File, rustix::io::Errno> {
    require_single_component(name).map_err(|_| rustix::io::Errno::INVAL)?;
    open_directory_at_os_raw(parent, std::ffi::OsStr::new(name))
}

fn open_directory_at_os(
    parent: &std::fs::File,
    name: &std::ffi::OsStr,
) -> Result<std::fs::File, String> {
    open_directory_at_os_raw(parent, name)
        .map_err(|error| format!("open selected directory: {error}"))
}

fn open_directory_at_os_raw(
    parent: &std::fs::File,
    name: &std::ffi::OsStr,
) -> Result<std::fs::File, rustix::io::Errno> {
    use rustix::fs::{Mode, OFlags};

    let fd = rustix::fs::openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    Ok(std::fs::File::from(fd))
}

fn open_regular_at(parent: &std::fs::File, name: &str) -> Result<std::fs::File, String> {
    use rustix::fs::{Mode, OFlags};

    require_single_component(name)?;
    let fd = rustix::fs::openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(|error| format!("open selected memory file: {error}"))?;
    let file = std::fs::File::from(fd);
    FileIdentity::from_file(&file)?;
    Ok(file)
}

fn require_single_component(value: &str) -> Result<(), String> {
    let mut components = Path::new(value).components();
    if matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none() {
        Ok(())
    } else {
        Err("selected path component is invalid".to_owned())
    }
}

fn require_directory_identity(
    label: &str,
    file: &std::fs::File,
    expected: DirectoryIdentity,
) -> Result<(), String> {
    if DirectoryIdentity::from_file(file)? == expected {
        Ok(())
    } else {
        Err(format!("{label} no longer has its listed identity"))
    }
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

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let home = tempfile::tempdir().unwrap();
        let cwd = home.path().join("work");
        std::fs::create_dir(&cwd).unwrap();
        let canonical = cwd.canonicalize().unwrap().to_string_lossy().into_owned();
        let slug = crate::claude::ClaudeCodeProvider::encode_project_path(&canonical);
        let memory = home
            .path()
            .join(".claude/projects")
            .join(slug)
            .join("memory");
        std::fs::create_dir_all(&memory).unwrap();
        (home, cwd, memory)
    }

    #[tokio::test]
    async fn exact_selection_reads_once_with_verified_identity() {
        let (home, cwd, memory) = fixture();
        std::fs::write(memory.join("fact.md"), "---\ntype: project\n---\ninside").unwrap();
        let mut registry = BoundClaudeMemoryRegistry::default();
        let list = registry
            .list(home.path(), &cwd.to_string_lossy(), scope())
            .await
            .unwrap();
        let item = &list.items[0];
        let read = registry
            .read(&item.selection_token, &scope())
            .await
            .unwrap();
        assert_eq!(read.content, "---\ntype: project\n---\ninside");
        assert_eq!(read.filename, item.filename);
        assert_eq!(read.memory_type.as_deref(), Some("project"));
        assert_eq!(read.canonical_cwd, list.canonical_cwd);
        assert_eq!(read.project_slug, list.project_slug);
        assert_eq!(read.selection_token, item.selection_token);
        assert!(
            registry
                .read(&item.selection_token, &scope())
                .await
                .unwrap_err()
                .contains("already consumed")
        );
    }

    #[tokio::test]
    async fn snapshot_capability_reservation_is_atomic_at_capacity() {
        let (home, cwd, memory) = fixture();
        std::fs::write(memory.join("a.md"), "a").unwrap();
        std::fs::write(memory.join("b.md"), "b").unwrap();
        let mut registry = BoundClaudeMemoryRegistry::with_limits(6, Duration::from_mins(1));
        let items = registry
            .list_capabilities(home.path(), &cwd.to_string_lossy(), scope())
            .await
            .unwrap();
        assert_eq!(items.len(), 2);
        for item in items {
            registry
                .read(&item.read_selection_token, &scope())
                .await
                .unwrap();
            registry
                .open(&item.open_selection_token, &scope())
                .await
                .unwrap();
            assert!(
                registry
                    .take(&item.copy_selection_token, &scope(), Instant::now())
                    .is_ok()
            );
            assert!(
                registry
                    .read(&item.read_selection_token, &scope())
                    .await
                    .unwrap_err()
                    .contains("consumed")
            );
        }
    }

    #[tokio::test]
    async fn protocol_maximum_snapshot_retains_all_tokens_and_omits_cap_plus_one() {
        let (home, cwd, memory) = fixture();
        for index in 0..=MAX_BOUND_MEMORY_ITEMS {
            std::fs::write(memory.join(format!("{index:04}.md")), "x").unwrap();
        }
        let mut registry = BoundClaudeMemoryRegistry::default();
        let items = registry
            .list_capabilities(home.path(), &cwd.to_string_lossy(), scope())
            .await
            .unwrap();
        assert_eq!(items.len(), MAX_BOUND_MEMORY_ITEMS);
        assert!(items.iter().all(|item| item.filename != "0512.md"));
        for index in [0, items.len() - 1] {
            registry
                .read(&items[index].read_selection_token, &scope())
                .await
                .unwrap();
            registry
                .open(&items[index].open_selection_token, &scope())
                .await
                .unwrap();
            assert!(
                registry
                    .take(&items[index].copy_selection_token, &scope(), Instant::now())
                    .is_ok()
            );
        }
    }

    #[tokio::test]
    async fn oversized_snapshot_reservation_does_not_evict_existing_capability() {
        let (home, cwd, memory) = fixture();
        std::fs::write(memory.join("a.md"), "a").unwrap();
        let mut registry = BoundClaudeMemoryRegistry::with_limits(5, Duration::from_mins(1));
        let existing = registry
            .list(home.path(), &cwd.to_string_lossy(), scope())
            .await
            .unwrap()
            .items
            .remove(0);
        std::fs::write(memory.join("b.md"), "b").unwrap();
        let error = registry
            .list_capabilities(home.path(), &cwd.to_string_lossy(), scope())
            .await
            .unwrap_err();
        assert!(error.contains("capacity"), "{error}");
        registry
            .read(&existing.selection_token, &scope())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn every_snapshot_capability_expires_and_replays_fail() {
        let (home, cwd, memory) = fixture();
        std::fs::write(memory.join("fact.md"), "inside").unwrap();
        let mut registry = BoundClaudeMemoryRegistry::with_limits(3, Duration::ZERO);
        let item = registry
            .list_capabilities(home.path(), &cwd.to_string_lossy(), scope())
            .await
            .unwrap()
            .remove(0);
        assert!(
            registry
                .read(&item.read_selection_token, &scope())
                .await
                .unwrap_err()
                .contains("expired")
        );
        assert!(
            registry
                .open(&item.open_selection_token, &scope())
                .await
                .unwrap_err()
                .contains("expired")
        );
        assert!(
            registry
                .take(&item.copy_selection_token, &scope(), Instant::now())
                .unwrap_err()
                .contains("expired")
        );
        assert!(
            registry
                .read(&item.read_selection_token, &scope())
                .await
                .unwrap_err()
                .contains("stale")
        );
    }

    #[tokio::test]
    async fn idle_expiry_purge_drops_all_memory_capabilities_without_followup_operation() {
        let (home, cwd, memory) = fixture();
        std::fs::write(memory.join("fact.md"), "inside").unwrap();
        let mut registry = BoundClaudeMemoryRegistry::with_limits(3, Duration::ZERO);
        registry
            .list_capabilities(home.path(), &cwd.to_string_lossy(), scope())
            .await
            .unwrap();
        assert_eq!(registry.selections.len(), 3);
        registry.purge_expired_now();
        assert!(registry.selections.is_empty());
        assert!(registry.insertion_order.is_empty());
    }

    #[tokio::test]
    async fn copy_commits_inside_held_destination_and_retains_exact_outcomes() {
        let (home, cwd, memory) = fixture();
        std::fs::write(memory.join("fact.md"), "inside").unwrap();
        let target_slug = "target".to_owned();
        let target_project = home.path().join(".claude/projects").join(&target_slug);
        std::fs::create_dir_all(target_project.join("memory")).unwrap();
        let destination =
            ResolvedMemoryDestination::from_directory(target_slug.clone(), &target_project)
                .unwrap();
        let ledger = ProjectMutationLedger::default();
        let mut registry = BoundClaudeMemoryRegistry::default();
        let first = registry
            .list(home.path(), &cwd.to_string_lossy(), scope())
            .await
            .unwrap();
        let mutation_id = uuid::Uuid::now_v7().to_string();
        let receipt = registry
            .copy_to_project(
                &first.items[0].selection_token,
                destination,
                &mutation_id,
                "target".to_owned(),
                ledger.clone(),
                &scope(),
            )
            .await
            .unwrap();
        assert_eq!(receipt.outcome, ProjectMemoryCopyOutcome::Copied);
        assert_eq!(ledger.reconcile(&mutation_id).unwrap(), receipt);
        assert_eq!(
            std::fs::read_to_string(target_project.join("memory/fact.md")).unwrap(),
            "inside"
        );

        let second = registry
            .list(home.path(), &cwd.to_string_lossy(), scope())
            .await
            .unwrap();
        let second_id = uuid::Uuid::now_v7().to_string();
        let already_exists = registry
            .copy_to_project(
                &second.items[0].selection_token,
                ResolvedMemoryDestination::from_directory(target_slug, &target_project).unwrap(),
                &second_id,
                "target".to_owned(),
                ledger.clone(),
                &scope(),
            )
            .await
            .unwrap();
        assert_eq!(
            already_exists.outcome,
            ProjectMemoryCopyOutcome::AlreadyExists
        );
        assert_eq!(ledger.reconcile(&second_id).unwrap(), already_exists);
        assert!(
            std::fs::read_dir(target_project.join("memory"))
                .unwrap()
                .all(|entry| !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".kodosi"))
        );
    }

    #[tokio::test]
    async fn destination_replacement_before_copy_fails_closed() {
        let (home, cwd, memory) = fixture();
        std::fs::write(memory.join("fact.md"), "inside").unwrap();
        let target_project = home.path().join(".claude/projects/target");
        std::fs::create_dir_all(target_project.join("memory")).unwrap();
        let destination =
            ResolvedMemoryDestination::from_directory("target".to_owned(), &target_project)
                .unwrap();
        let retired = home.path().join(".claude/projects/retired");
        std::fs::rename(&target_project, &retired).unwrap();
        std::fs::create_dir_all(target_project.join("memory")).unwrap();
        let mut registry = BoundClaudeMemoryRegistry::default();
        let list = registry
            .list(home.path(), &cwd.to_string_lossy(), scope())
            .await
            .unwrap();
        let ledger = ProjectMutationLedger::default();
        let mutation_id = uuid::Uuid::now_v7().to_string();
        let error = registry
            .copy_to_project(
                &list.items[0].selection_token,
                destination,
                &mutation_id,
                "target".to_owned(),
                ledger.clone(),
                &scope(),
            )
            .await
            .unwrap_err();
        assert!(error.contains("replaced"), "{error}");
        assert!(!target_project.join("memory/fact.md").exists());
        assert!(ledger.existing(&mutation_id).unwrap().is_none());
    }

    #[tokio::test]
    async fn destination_memory_directory_replacement_fails_closed() {
        let (home, cwd, memory) = fixture();
        std::fs::write(memory.join("fact.md"), "inside").unwrap();
        let target_project = home.path().join(".claude/projects/target");
        let target_memory = target_project.join("memory");
        std::fs::create_dir_all(&target_memory).unwrap();
        let destination =
            ResolvedMemoryDestination::from_directory("target".to_owned(), &target_project)
                .unwrap();
        std::fs::rename(&target_memory, target_project.join("memory-retired")).unwrap();
        std::fs::create_dir(&target_memory).unwrap();
        let mut registry = BoundClaudeMemoryRegistry::default();
        let list = registry
            .list(home.path(), &cwd.to_string_lossy(), scope())
            .await
            .unwrap();
        let ledger = ProjectMutationLedger::default();
        let mutation_id = uuid::Uuid::now_v7().to_string();
        let error = registry
            .copy_to_project(
                &list.items[0].selection_token,
                destination,
                &mutation_id,
                "target".to_owned(),
                ledger.clone(),
                &scope(),
            )
            .await
            .unwrap_err();
        assert!(error.contains("memory destination was replaced"), "{error}");
        assert!(!target_memory.join("fact.md").exists());
        assert!(ledger.existing(&mutation_id).unwrap().is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cwd_rename_then_symlink_swap_fails_closed() {
        use std::os::unix::fs::symlink;

        let (home, cwd, memory) = fixture();
        std::fs::write(memory.join("fact.md"), "inside").unwrap();
        let outside = home.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        let mut registry = BoundClaudeMemoryRegistry::default();
        let list = registry
            .list(home.path(), &cwd.to_string_lossy(), scope())
            .await
            .unwrap();
        let renamed = home.path().join("work-old");
        std::fs::rename(&cwd, &renamed).unwrap();
        symlink(&outside, &cwd).unwrap();

        let error = registry
            .read(&list.items[0].selection_token, &scope())
            .await
            .unwrap_err();
        assert!(
            error.contains("directory") || error.contains("symlink"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn replacement_file_fails_closed() {
        let (home, cwd, memory) = fixture();
        let path = memory.join("fact.md");
        std::fs::write(&path, "inside").unwrap();
        let mut registry = BoundClaudeMemoryRegistry::default();
        let list = registry
            .list(home.path(), &cwd.to_string_lossy(), scope())
            .await
            .unwrap();
        std::fs::rename(&path, memory.join("old.md")).unwrap();
        std::fs::write(&path, "replacement").unwrap();

        let error = registry
            .read(&list.items[0].selection_token, &scope())
            .await
            .unwrap_err();
        assert!(error.contains("listed file"), "{error}");
    }

    #[tokio::test]
    async fn expiry_cap_malformed_and_account_scope_fail_closed() {
        let (home, cwd, memory) = fixture();
        std::fs::write(memory.join("fact.md"), "inside").unwrap();
        let mut expired = BoundClaudeMemoryRegistry::with_limits(1, Duration::ZERO);
        let list = expired
            .list(home.path(), &cwd.to_string_lossy(), scope())
            .await
            .unwrap();
        assert!(
            expired
                .read(&list.items[0].selection_token, &scope())
                .await
                .unwrap_err()
                .contains("expired")
        );
        assert!(
            expired
                .read("not-a-token", &scope())
                .await
                .unwrap_err()
                .contains("invalid")
        );

        let mut capped = BoundClaudeMemoryRegistry::with_limits(1, Duration::from_mins(1));
        let first = capped
            .list(home.path(), &cwd.to_string_lossy(), scope())
            .await
            .unwrap();
        let second = capped
            .list(home.path(), &cwd.to_string_lossy(), scope())
            .await
            .unwrap();
        assert!(
            capped
                .read(&first.items[0].selection_token, &scope())
                .await
                .unwrap_err()
                .contains("stale")
        );
        let wrong_scope = MemorySelectionScope {
            account_user_id: Some("other".to_owned()),
            account_epoch: 8,
        };
        assert!(
            capped
                .read(&second.items[0].selection_token, &wrong_scope)
                .await
                .unwrap_err()
                .contains("active account")
        );
    }

    #[tokio::test]
    async fn path_escape_and_symlinked_file_are_never_selected() {
        let (home, cwd, memory) = fixture();
        std::fs::write(memory.join("fact.md"), "inside").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(home.path().join("outside.md"), memory.join("escape.md"))
            .unwrap();
        let mut registry = BoundClaudeMemoryRegistry::default();
        let list = registry
            .list(home.path(), &cwd.to_string_lossy(), scope())
            .await
            .unwrap();
        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].filename, "fact.md");
        assert!(
            registry
                .list(
                    home.path(),
                    &format!("{}/../escape", cwd.display()),
                    scope()
                )
                .await
                .unwrap_err()
                .contains("invalid cwd")
        );
    }
}

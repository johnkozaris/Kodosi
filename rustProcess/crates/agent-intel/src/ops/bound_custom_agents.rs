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
    bound_memory::MemorySelectionScope,
    custom_agents::{AgentTarget, CustomAgentFile, MAX_CUSTOM_AGENT_FILES},
};

pub const MAX_BOUND_PROJECT_CUSTOM_AGENTS: usize = 128;
const PROJECT_SNAPSHOT_CAPABILITIES_PER_ITEM: usize = 2;
const DEFAULT_SELECTION_CAPACITY: usize =
    MAX_BOUND_PROJECT_CUSTOM_AGENTS * PROJECT_SNAPSHOT_CAPABILITIES_PER_ITEM;
const DEFAULT_SELECTION_TTL: Duration = Duration::from_mins(5);
const MAX_CUSTOM_AGENT_FILE_BYTES: u64 = 1024 * 1024;
const MAX_CUSTOM_AGENT_FRONTMATTER_BYTES: u64 = 64 * 1024;
pub const MAX_CUSTOM_AGENT_SUMMARY_BYTES: usize = 256 * 1024;
const MAX_SUMMARY_NAME_BYTES: usize = 256;
const MAX_SUMMARY_DESCRIPTION_BYTES: usize = 2 * 1024;
const MAX_SUMMARY_MODEL_BYTES: usize = 256;
const MAX_SUMMARY_TOOLS: usize = 64;
const MAX_SUMMARY_TOOL_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoundCustomAgentList {
    pub items: Vec<BoundCustomAgentSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoundCustomAgentSummary {
    pub selection_token: String,
    pub target: AgentTarget,
    pub name: String,
    pub description: String,
    pub model: Option<String>,
    pub tools: Vec<String>,
    pub error_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BoundCustomAgentCapabilitySummary {
    pub detail_selection_token: String,
    pub open_selection_token: String,
    pub target: AgentTarget,
    pub name: String,
    pub description: String,
    pub model: Option<String>,
    pub tools: Vec<String>,
    pub error_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BoundCustomAgentDetail {
    pub selection_token: String,
    pub target: AgentTarget,
    pub name: String,
    pub description: String,
    pub model: Option<String>,
    pub tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub frontmatter: String,
    pub prompt: String,
    pub errors: Vec<String>,
}

#[derive(Debug)]
pub struct BoundCustomAgentRegistry {
    selections: HashMap<uuid::Uuid, BoundSelection>,
    insertion_order: VecDeque<uuid::Uuid>,
    capacity: usize,
    ttl: Duration,
}

struct InstalledSummary {
    tokens: Vec<String>,
    summary: PreparedSummary,
}

#[derive(Debug)]
pub struct StagedCustomAgentCapabilities {
    items: Vec<BoundCustomAgentCapabilitySummary>,
    selections: Vec<(uuid::Uuid, BoundSelection)>,
}

impl StagedCustomAgentCapabilities {
    pub fn items(&self) -> &[BoundCustomAgentCapabilitySummary] {
        &self.items
    }

    fn required(&self) -> usize {
        self.selections.len()
    }
}

impl Default for BoundCustomAgentRegistry {
    fn default() -> Self {
        Self {
            selections: HashMap::new(),
            insertion_order: VecDeque::new(),
            capacity: DEFAULT_SELECTION_CAPACITY,
            ttl: DEFAULT_SELECTION_TTL,
        }
    }
}

impl BoundCustomAgentRegistry {
    pub async fn list(
        &mut self,
        directory: &str,
        scope: MemorySelectionScope,
    ) -> Result<BoundCustomAgentList, String> {
        let directory = PathBuf::from(directory);
        let item_limit = self.capacity.min(MAX_CUSTOM_AGENT_FILES);
        let prepared = tokio::task::spawn_blocking(move || prepare_list(&directory, item_limit))
            .await
            .map_err(|error| format!("bound custom-agent list task join: {error}"))??;
        self.install_single(prepared, &scope, Instant::now())
    }

    pub async fn list_capabilities(
        &mut self,
        directory: &Path,
        scope: MemorySelectionScope,
        item_limit: usize,
    ) -> Result<Vec<BoundCustomAgentCapabilitySummary>, String> {
        let directory = directory.to_owned();
        let item_limit = item_limit.min(MAX_BOUND_PROJECT_CUSTOM_AGENTS);
        let prepared = tokio::task::spawn_blocking(move || prepare_list(&directory, item_limit))
            .await
            .map_err(|error| format!("bound custom-agent capability task join: {error}"))??;
        let staged = self.stage_capabilities_prepared(prepared, &scope, Instant::now())?;
        let items = staged.items.clone();
        self.commit_staged_capabilities(vec![staged]);
        Ok(items)
    }

    pub async fn stage_capabilities(
        &self,
        directory: &Path,
        scope: MemorySelectionScope,
        item_limit: usize,
    ) -> Result<StagedCustomAgentCapabilities, String> {
        let directory = directory.to_owned();
        let item_limit = item_limit.min(MAX_BOUND_PROJECT_CUSTOM_AGENTS);
        let prepared = tokio::task::spawn_blocking(move || prepare_list(&directory, item_limit))
            .await
            .map_err(|error| format!("bound custom-agent capability task join: {error}"))??;
        self.stage_capabilities_prepared(prepared, &scope, Instant::now())
    }

    pub async fn stage_capabilities_from_root(
        &self,
        root: &std::fs::File,
        components: &[&str],
        target: AgentTarget,
        scope: MemorySelectionScope,
        item_limit: usize,
    ) -> Result<StagedCustomAgentCapabilities, String> {
        let root = root
            .try_clone()
            .map_err(|error| format!("retain project root for custom agents: {error}"))?;
        let components = components
            .iter()
            .map(|component| (*component).to_owned())
            .collect::<Vec<_>>();
        let item_limit = item_limit.min(MAX_BOUND_PROJECT_CUSTOM_AGENTS);
        let prepared = tokio::task::spawn_blocking(move || {
            prepare_list_from_root(root, &components, target, item_limit)
        })
        .await
        .map_err(|error| format!("bound custom-agent capability task join: {error}"))??;
        self.stage_capabilities_prepared(prepared, &scope, Instant::now())
    }

    pub fn preflight_staged_capabilities(
        &self,
        staged: &[&StagedCustomAgentCapabilities],
    ) -> Result<(), String> {
        let required = staged.iter().try_fold(0_usize, |total, stage| {
            total
                .checked_add(stage.required())
                .ok_or_else(|| "custom-agent capability reservation overflow".to_owned())
        })?;
        if required > self.capacity {
            return Err("custom-agent snapshot exceeds capability capacity".to_owned());
        }
        Ok(())
    }

    pub fn validate_staged_capabilities(
        &self,
        staged: &[&StagedCustomAgentCapabilities],
    ) -> Result<(), String> {
        for stage in staged {
            for (_, selection) in &stage.selections {
                selection.verify_current()?;
            }
        }
        Ok(())
    }

    pub fn commit_staged_capabilities(&mut self, staged: Vec<StagedCustomAgentCapabilities>) {
        self.purge_expired(Instant::now());
        let required = staged
            .iter()
            .map(StagedCustomAgentCapabilities::required)
            .sum();
        self.evict_for(required);
        for stage in staged {
            for (token, selection) in stage.selections {
                self.insertion_order.push_back(token);
                self.selections.insert(token, selection);
            }
        }
    }

    pub async fn read(
        &mut self,
        selection_token: &str,
        scope: &MemorySelectionScope,
    ) -> Result<BoundCustomAgentDetail, String> {
        let selection = self.take(selection_token, scope, Instant::now())?;
        tokio::task::spawn_blocking(move || selection.read())
            .await
            .map_err(|error| format!("bound custom-agent read task join: {error}"))?
    }

    pub async fn open(
        &mut self,
        selection_token: &str,
        scope: &MemorySelectionScope,
    ) -> Result<(std::fs::File, String), String> {
        let selection = self.take(selection_token, scope, Instant::now())?;
        tokio::task::spawn_blocking(move || selection.open_file())
            .await
            .map_err(|error| format!("bound custom-agent open task join: {error}"))?
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
    ) -> Result<BoundCustomAgentList, String> {
        Ok(BoundCustomAgentList {
            items: self
                .install(prepared, scope, now, 1)?
                .into_iter()
                .map(|item| BoundCustomAgentSummary {
                    selection_token: item.tokens[0].clone(),
                    target: item.summary.target,
                    name: item.summary.name,
                    description: item.summary.description,
                    model: item.summary.model,
                    tools: item.summary.tools,
                    error_count: item.summary.error_count,
                })
                .collect(),
        })
    }

    fn stage_capabilities_prepared(
        &self,
        prepared: PreparedList,
        scope: &MemorySelectionScope,
        now: Instant,
    ) -> Result<StagedCustomAgentCapabilities, String> {
        let (installed, selections) =
            self.stage_install(prepared, scope, now, PROJECT_SNAPSHOT_CAPABILITIES_PER_ITEM)?;
        let items = installed
            .into_iter()
            .map(|item| BoundCustomAgentCapabilitySummary {
                detail_selection_token: item.tokens[0].clone(),
                open_selection_token: item.tokens[1].clone(),
                target: item.summary.target,
                name: item.summary.name,
                description: item.summary.description,
                model: item.summary.model,
                tools: item.summary.tools,
                error_count: item.summary.error_count,
            })
            .collect();
        Ok(StagedCustomAgentCapabilities { items, selections })
    }

    fn install(
        &mut self,
        prepared: PreparedList,
        scope: &MemorySelectionScope,
        now: Instant,
        capabilities_per_item: usize,
    ) -> Result<Vec<InstalledSummary>, String> {
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
    ) -> Result<(Vec<InstalledSummary>, Vec<(uuid::Uuid, BoundSelection)>), String> {
        if capabilities_per_item == 0 {
            return Err("custom-agent capability count must be positive".to_owned());
        }
        let mut files = Vec::with_capacity(prepared.files.len());
        let mut serialized_bytes = br#"{"items":[]}"#.len();
        for file in prepared.files {
            let placeholder = "01900000-0000-7000-8000-000000000000".to_owned();
            let item_bytes = if capabilities_per_item == 1 {
                serde_json::to_vec(&BoundCustomAgentSummary {
                    selection_token: placeholder,
                    target: file.summary.target,
                    name: file.summary.name.clone(),
                    description: file.summary.description.clone(),
                    model: file.summary.model.clone(),
                    tools: file.summary.tools.clone(),
                    error_count: file.summary.error_count,
                })
            } else {
                serde_json::to_vec(&BoundCustomAgentCapabilitySummary {
                    detail_selection_token: placeholder.clone(),
                    open_selection_token: placeholder,
                    target: file.summary.target,
                    name: file.summary.name.clone(),
                    description: file.summary.description.clone(),
                    model: file.summary.model.clone(),
                    tools: file.summary.tools.clone(),
                    error_count: file.summary.error_count,
                })
            }
            .map_err(|error| format!("encode custom-agent summary: {error}"))?
            .len();
            let delimiter_bytes = usize::from(!files.is_empty());
            if serialized_bytes
                .saturating_add(item_bytes)
                .saturating_add(delimiter_bytes)
                > MAX_CUSTOM_AGENT_SUMMARY_BYTES
            {
                break;
            }
            serialized_bytes = serialized_bytes
                .saturating_add(item_bytes)
                .saturating_add(delimiter_bytes);
            files.push(file);
        }
        let required = files
            .len()
            .checked_mul(capabilities_per_item)
            .ok_or_else(|| "custom-agent capability reservation overflow".to_owned())?;
        if required > self.capacity {
            return Err("custom-agent snapshot exceeds capability capacity".to_owned());
        }
        let mut installed = Vec::with_capacity(files.len());
        let mut selections = Vec::with_capacity(required);
        let mut prepared_capabilities = (0..required).map(|_| uuid::Uuid::now_v7());
        for file in files {
            let summary = file.summary.clone();
            let file = Arc::new(file);
            let mut tokens = Vec::with_capacity(capabilities_per_item);
            for _ in 0..capabilities_per_item {
                let Some(token) = prepared_capabilities.next() else {
                    return Err("custom-agent capability reservation is inconsistent".to_owned());
                };
                selections.push((
                    token,
                    BoundSelection {
                        token,
                        scope: scope.clone(),
                        expires_at: now + self.ttl,
                        authority: Arc::clone(&prepared.authority),
                        file: Arc::clone(&file),
                    },
                ));
                tokens.push(token.to_string());
            }
            installed.push(InstalledSummary { tokens, summary });
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
            .ok_or_else(|| "custom-agent selection is stale or already consumed".to_owned())?;
        self.insertion_order.retain(|candidate| *candidate != token);
        if now >= selection.expires_at {
            return Err("custom-agent selection expired".to_owned());
        }
        if selection.scope != *scope {
            return Err("custom-agent selection does not belong to the active account".to_owned());
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
    let token = uuid::Uuid::parse_str(value)
        .map_err(|_| "invalid custom-agent selection token".to_owned())?;
    if token.get_version_num() != 7 || token.hyphenated().to_string() != value {
        return Err("invalid custom-agent selection token".to_owned());
    }
    Ok(token)
}

#[derive(Debug)]
struct PreparedList {
    authority: Arc<DirectoryAuthority>,
    files: Vec<PreparedFile>,
}

#[derive(Debug)]
struct PreparedFile {
    filename: String,
    identity: FileIdentity,
    held_file: std::fs::File,
    summary: PreparedSummary,
}

#[derive(Debug, Clone)]
struct PreparedSummary {
    target: AgentTarget,
    name: String,
    description: String,
    model: Option<String>,
    tools: Vec<String>,
    error_count: usize,
}

#[derive(Debug)]
struct BoundSelection {
    token: uuid::Uuid,
    scope: MemorySelectionScope,
    expires_at: Instant,
    authority: Arc<DirectoryAuthority>,
    file: Arc<PreparedFile>,
}

impl BoundSelection {
    fn verify_current(&self) -> Result<(), String> {
        self.authority.verify_current()?;
        let current = open_regular_at(&self.authority.directory, &self.file.filename)?;
        if FileIdentity::from_file(&current)? != self.file.identity
            || FileIdentity::from_file(&self.file.held_file)? != self.file.identity
        {
            return Err("custom-agent capability no longer names the staged file".to_owned());
        }
        self.authority.verify_current()
    }

    fn open_file(self) -> Result<(std::fs::File, String), String> {
        self.authority.verify_current()?;
        let current = open_regular_at(&self.authority.directory, &self.file.filename)?;
        if FileIdentity::from_file(&current)? != self.file.identity {
            return Err("custom-agent selection no longer names the listed file".to_owned());
        }
        if FileIdentity::from_file(&self.file.held_file)? != self.file.identity {
            return Err("custom-agent selection file identity changed".to_owned());
        }
        self.authority.verify_current()?;
        Ok((current, self.file.filename.clone()))
    }

    fn read(self) -> Result<BoundCustomAgentDetail, String> {
        self.authority.verify_current()?;
        let mut current = open_regular_at(&self.authority.directory, &self.file.filename)?;
        let identity = FileIdentity::from_file(&current)?;
        if identity != self.file.identity {
            return Err("custom-agent selection no longer names the listed file".to_owned());
        }
        if FileIdentity::from_file(&self.file.held_file)? != self.file.identity {
            return Err("custom-agent selection file identity changed".to_owned());
        }

        let mut bytes = Vec::new();
        current
            .by_ref()
            .take(MAX_CUSTOM_AGENT_FILE_BYTES.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| format!("read selected custom-agent file: {error}"))?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_CUSTOM_AGENT_FILE_BYTES {
            return Err(format!(
                "custom agent file exceeds {MAX_CUSTOM_AGENT_FILE_BYTES} byte limit"
            ));
        }
        if FileIdentity::from_file(&current)? != self.file.identity {
            return Err("custom-agent selection changed while it was read".to_owned());
        }
        self.authority.verify_current()?;
        let content = String::from_utf8(bytes)
            .map_err(|error| format!("custom agent file is not valid UTF-8: {error}"))?;
        let parsed =
            CustomAgentFile::from_contents(self.authority.path.join(&self.file.filename), &content);
        Ok(BoundCustomAgentDetail {
            selection_token: self.token.to_string(),
            target: self.authority.kind.target(),
            name: parsed.name,
            description: parsed.description,
            model: parsed.model,
            tools: parsed.tools,
            disallowed_tools: parsed.disallowed_tools,
            frontmatter: parsed.frontmatter_raw,
            prompt: parsed.body_raw,
            errors: parsed.errors,
        })
    }
}

#[derive(Debug)]
struct DirectoryAuthority {
    path: PathBuf,
    kind: AgentDirectoryKind,
    identity: DirectoryIdentity,
    directory: std::fs::File,
    visible_parent: Option<(std::fs::File, std::ffi::OsString)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentDirectoryKind {
    Claude,
    Copilot,
}

impl AgentDirectoryKind {
    fn target(self) -> AgentTarget {
        match self {
            Self::Claude => AgentTarget::Claude,
            Self::Copilot => AgentTarget::VsCode,
        }
    }

    fn from_target(target: AgentTarget) -> Result<Self, String> {
        match target {
            AgentTarget::Claude => Ok(Self::Claude),
            AgentTarget::VsCode => Ok(Self::Copilot),
            AgentTarget::Unknown => Err("unknown custom-agent target is not selectable".to_owned()),
        }
    }
}

impl DirectoryAuthority {
    fn verify_current(&self) -> Result<(), String> {
        require_directory_identity(
            "held custom-agent directory",
            &self.directory,
            self.identity,
        )?;
        let current = if let Some((parent, entry_name)) = &self.visible_parent {
            open_directory_at_os_raw(parent, entry_name)
                .map_err(|error| format!("open custom-agent directory: {error}"))?
        } else {
            open_absolute_directory(&self.path)?
        };
        require_directory_identity("custom-agent directory", &current, self.identity)
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
            .map_err(|error| format!("inspect custom-agent directory: {error}"))?;
        if !metadata.is_dir() {
            return Err("custom-agent root must be a directory".to_owned());
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
            .map_err(|error| format!("inspect selected custom-agent file: {error}"))?;
        if !metadata.file_type().is_file() {
            return Err("selected custom-agent path must be a regular file".to_owned());
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

fn prepare_list(directory: &Path, capacity: usize) -> Result<PreparedList, String> {
    let kind = agent_directory_kind(directory)?;
    let directory_file = match open_absolute_directory_raw(directory) {
        Ok(file) => file,
        Err(rustix::io::Errno::NOENT) => {
            return Ok(PreparedList {
                authority: Arc::new(missing_directory_authority(directory)?),
                files: Vec::new(),
            });
        }
        Err(error) => return Err(format!("open custom-agent directory: {error}")),
    };
    let authority = Arc::new(DirectoryAuthority {
        path: directory.to_owned(),
        kind,
        identity: DirectoryIdentity::from_file(&directory_file)?,
        directory: directory_file,
        visible_parent: None,
    });
    let mut files = list_files(&authority, capacity)?;
    files.sort_by(|left, right| {
        left.summary
            .name
            .to_ascii_lowercase()
            .cmp(&right.summary.name.to_ascii_lowercase())
            .then(left.filename.cmp(&right.filename))
    });
    files.truncate(capacity);
    Ok(PreparedList { authority, files })
}

fn missing_directory_authority(directory: &Path) -> Result<DirectoryAuthority, String> {
    validate_absolute_path(directory)?;
    let filesystem_root =
        std::fs::File::open("/").map_err(|error| format!("open filesystem root: {error}"))?;
    Ok(DirectoryAuthority {
        path: directory.to_owned(),
        kind: agent_directory_kind(directory)?,
        identity: DirectoryIdentity::from_file(&filesystem_root)?,
        directory: filesystem_root,
        visible_parent: None,
    })
}

fn prepare_list_from_root(
    root: std::fs::File,
    components: &[String],
    target: AgentTarget,
    capacity: usize,
) -> Result<PreparedList, String> {
    if components.is_empty() {
        return Err("custom-agent directory path is empty".to_owned());
    }
    let kind = AgentDirectoryKind::from_target(target)?;
    let mut parent = root;
    for component in &components[..components.len() - 1] {
        require_single_component(component)?;
        parent = match open_directory_at_os_raw(&parent, std::ffi::OsStr::new(component)) {
            Ok(directory) => directory,
            Err(rustix::io::Errno::NOENT) => {
                return Ok(PreparedList {
                    authority: Arc::new(DirectoryAuthority {
                        path: components.iter().collect(),
                        kind,
                        identity: DirectoryIdentity::from_file(&parent)?,
                        directory: parent,
                        visible_parent: None,
                    }),
                    files: Vec::new(),
                });
            }
            Err(error) => return Err(format!("open custom-agent directory: {error}")),
        };
    }
    let entry_name = &components[components.len() - 1];
    require_single_component(entry_name)?;
    let directory = match open_directory_at_os_raw(&parent, std::ffi::OsStr::new(entry_name)) {
        Ok(directory) => directory,
        Err(rustix::io::Errno::NOENT) => {
            return Ok(PreparedList {
                authority: Arc::new(DirectoryAuthority {
                    path: components.iter().collect(),
                    kind,
                    identity: DirectoryIdentity::from_file(&parent)?,
                    directory: parent,
                    visible_parent: None,
                }),
                files: Vec::new(),
            });
        }
        Err(error) => return Err(format!("open custom-agent directory: {error}")),
    };
    let authority = Arc::new(DirectoryAuthority {
        path: components.iter().collect(),
        kind,
        identity: DirectoryIdentity::from_file(&directory)?,
        directory,
        visible_parent: Some((parent, std::ffi::OsString::from(entry_name))),
    });
    let mut files = list_files(&authority, capacity)?;
    files.sort_by(|left, right| {
        left.summary
            .name
            .to_ascii_lowercase()
            .cmp(&right.summary.name.to_ascii_lowercase())
            .then(left.filename.cmp(&right.filename))
    });
    files.truncate(capacity);
    Ok(PreparedList { authority, files })
}

fn list_files(
    authority: &DirectoryAuthority,
    selection_capacity: usize,
) -> Result<Vec<PreparedFile>, String> {
    let mut entries = rustix::fs::Dir::read_from(&authority.directory)
        .map_err(|error| format!("list custom-agent directory: {error}"))?;
    let mut filenames = Vec::new();
    for entry in entries.by_ref().take(MAX_CUSTOM_AGENT_FILES) {
        let entry = entry.map_err(|error| format!("read custom-agent directory entry: {error}"))?;
        let bytes = entry.file_name().to_bytes();
        let Ok(filename) = std::str::from_utf8(bytes) else {
            continue;
        };
        if filename == "." || filename == ".." || !accepted_filename(filename, authority.kind) {
            continue;
        }
        filenames.push(filename.to_owned());
    }
    filenames.sort();
    filenames.truncate(selection_capacity);

    let mut files = Vec::with_capacity(filenames.len());
    for filename in filenames {
        let Ok(file) = open_regular_at(&authority.directory, &filename) else {
            continue;
        };
        let identity = FileIdentity::from_file(&file)?;
        let summary = read_summary(authority, &filename, &file);
        if FileIdentity::from_file(&file)? != identity {
            return Err("custom-agent file changed while its summary was read".to_owned());
        }
        files.push(PreparedFile {
            filename,
            identity,
            held_file: file,
            summary,
        });
    }
    Ok(files)
}

fn accepted_filename(filename: &str, kind: AgentDirectoryKind) -> bool {
    match kind {
        AgentDirectoryKind::Claude => Path::new(filename)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md")),
        AgentDirectoryKind::Copilot => filename.to_ascii_lowercase().ends_with(".agent.md"),
    }
}

fn read_summary(
    authority: &DirectoryAuthority,
    filename: &str,
    file: &std::fs::File,
) -> PreparedSummary {
    let fallback = || PreparedSummary {
        target: authority.kind.target(),
        name: fallback_agent_name(filename),
        description: String::new(),
        model: None,
        tools: Vec::new(),
        error_count: 1,
    };
    let Ok(mut file) = file.try_clone() else {
        return fallback();
    };
    let mut bytes = Vec::new();
    if file
        .by_ref()
        .take(MAX_CUSTOM_AGENT_FRONTMATTER_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .is_err()
    {
        return fallback();
    }
    let Ok(content) = String::from_utf8(bytes) else {
        return fallback();
    };
    let parsed = CustomAgentFile::from_contents(authority.path.join(filename), &content);
    let fallback_name = fallback_agent_name(filename);
    let name = if parsed.name.is_empty() {
        &fallback_name
    } else {
        &parsed.name
    };
    PreparedSummary {
        target: authority.kind.target(),
        name: truncate_utf8(name, MAX_SUMMARY_NAME_BYTES),
        description: truncate_utf8(&parsed.description, MAX_SUMMARY_DESCRIPTION_BYTES),
        model: parsed
            .model
            .map(|model| truncate_utf8(&model, MAX_SUMMARY_MODEL_BYTES)),
        tools: parsed
            .tools
            .into_iter()
            .take(MAX_SUMMARY_TOOLS)
            .map(|tool| truncate_utf8(&tool, MAX_SUMMARY_TOOL_BYTES))
            .collect(),
        error_count: parsed.errors.len(),
    }
}

fn truncate_utf8(value: &str, maximum_bytes: usize) -> String {
    if value.len() <= maximum_bytes {
        return value.to_owned();
    }
    value[..value.floor_char_boundary(maximum_bytes)].to_owned()
}

fn fallback_agent_name(filename: &str) -> String {
    filename
        .trim_end_matches(".agent.md")
        .trim_end_matches(".md")
        .to_owned()
}

fn agent_directory_kind(path: &Path) -> Result<AgentDirectoryKind, String> {
    validate_absolute_path(path)?;
    let components = path.components().collect::<Vec<_>>();
    let suffix = components.as_slice();
    let Some(Component::Normal(last)) = suffix.last() else {
        return Err("custom-agent directory must end in agents".to_owned());
    };
    if !last.eq_ignore_ascii_case("agents") {
        return Err("custom-agent directory must end in agents".to_owned());
    }
    let Some(Component::Normal(parent)) = suffix.get(suffix.len().saturating_sub(2)) else {
        return Err("custom-agent directory has an unsupported authority root".to_owned());
    };
    if parent.eq_ignore_ascii_case(".claude") {
        Ok(AgentDirectoryKind::Claude)
    } else if parent.eq_ignore_ascii_case(".github") || parent.eq_ignore_ascii_case(".copilot") {
        Ok(AgentDirectoryKind::Copilot)
    } else {
        Err("custom-agent directory has an unsupported authority root".to_owned())
    }
}

fn validate_absolute_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("custom-agent directory must be absolute".to_owned());
    }
    for component in path.components() {
        match component {
            Component::RootDir | Component::Normal(_) => {}
            _ => return Err("custom-agent directory path is not canonical".to_owned()),
        }
    }
    Ok(())
}

fn open_absolute_directory(path: &Path) -> Result<std::fs::File, String> {
    open_absolute_directory_raw(path)
        .map_err(|error| format!("open custom-agent directory: {error}"))
}

fn open_absolute_directory_raw(path: &Path) -> Result<std::fs::File, rustix::io::Errno> {
    validate_absolute_path(path).map_err(|_| rustix::io::Errno::INVAL)?;
    let mut current = std::fs::File::open("/").map_err(|_| rustix::io::Errno::IO)?;
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => {
                current = open_directory_at_os_raw(&current, name)?;
            }
            _ => return Err(rustix::io::Errno::INVAL),
        }
    }
    Ok(current)
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
    .map_err(|error| format!("open selected custom-agent file: {error}"))?;
    let file = std::fs::File::from(fd);
    FileIdentity::from_file(&file)?;
    Ok(file)
}

fn require_single_component(value: &str) -> Result<(), String> {
    let mut components = Path::new(value).components();
    if matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none() {
        Ok(())
    } else {
        Err("selected custom-agent path component is invalid".to_owned())
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

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let agents = root.path().join("work/.claude/agents");
        std::fs::create_dir_all(&agents).unwrap();
        (root, agents)
    }

    fn write_agent(path: &Path, name: &str, prompt: &str) {
        std::fs::write(
            path,
            format!(
                "---\nname: {name}\ndescription: {name} description\nmodel: sonnet\ntools: [Read, Grep]\ndisallowedTools: [Bash]\n---\n{prompt}"
            ),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn exact_detail_reads_once_without_paths() {
        let (_root, agents) = fixture();
        write_agent(&agents.join("reviewer.md"), "reviewer", "Review exactly.");
        let mut registry = BoundCustomAgentRegistry::default();
        let list = registry
            .list(&agents.to_string_lossy(), scope())
            .await
            .unwrap();
        assert_eq!(list.items.len(), 1);
        let summary = &list.items[0];
        assert_eq!(summary.name, "reviewer");
        assert_eq!(summary.tools, ["Read", "Grep"]);
        let detail = registry
            .read(&summary.selection_token, &scope())
            .await
            .unwrap();
        assert_eq!(detail.selection_token, summary.selection_token);
        assert_eq!(detail.name, summary.name);
        assert_eq!(detail.prompt, "Review exactly.");
        assert_eq!(detail.disallowed_tools, ["Bash"]);
        let encoded = serde_json::to_value(&detail).unwrap();
        assert!(encoded.get("path").is_none());
        assert!(
            registry
                .read(&summary.selection_token, &scope())
                .await
                .unwrap_err()
                .contains("already consumed")
        );
    }

    #[tokio::test]
    async fn snapshot_capabilities_are_reserved_without_self_eviction() {
        let (_root, agents) = fixture();
        write_agent(&agents.join("a.md"), "a", "a");
        write_agent(&agents.join("b.md"), "b", "b");
        let mut registry = BoundCustomAgentRegistry::with_limits(4, Duration::from_mins(1));
        let items = registry
            .list_capabilities(&agents, scope(), 2)
            .await
            .unwrap();
        assert_eq!(items.len(), 2);
        for item in items {
            registry
                .read(&item.detail_selection_token, &scope())
                .await
                .unwrap();
            registry
                .open(&item.open_selection_token, &scope())
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn protocol_maximum_custom_agent_snapshot_omits_cap_plus_one_without_eviction() {
        let (_root, agents) = fixture();
        for index in 0..=MAX_BOUND_PROJECT_CUSTOM_AGENTS {
            write_agent(
                &agents.join(format!("{index:04}.md")),
                &format!("agent-{index:04}"),
                "inside",
            );
        }
        let mut registry = BoundCustomAgentRegistry::default();
        let items = registry
            .list_capabilities(&agents, scope(), MAX_BOUND_PROJECT_CUSTOM_AGENTS)
            .await
            .unwrap();
        assert_eq!(items.len(), MAX_BOUND_PROJECT_CUSTOM_AGENTS);
        assert!(items.iter().all(|item| item.name != "agent-0128"));
        for index in [0, items.len() - 1] {
            registry
                .read(&items[index].detail_selection_token, &scope())
                .await
                .unwrap();
            registry
                .open(&items[index].open_selection_token, &scope())
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn cap_plus_one_fails_atomically_and_retains_older_token() {
        let (_root, agents) = fixture();
        write_agent(&agents.join("a.md"), "a", "a");
        let mut registry = BoundCustomAgentRegistry::with_limits(3, Duration::from_mins(1));
        let existing = registry
            .list(&agents.to_string_lossy(), scope())
            .await
            .unwrap()
            .items
            .remove(0);
        write_agent(&agents.join("b.md"), "b", "b");
        let error = registry
            .list_capabilities(&agents, scope(), 2)
            .await
            .unwrap_err();
        assert!(error.contains("capacity"), "{error}");
        registry
            .read(&existing.selection_token, &scope())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn later_snapshot_source_does_not_evict_earlier_reply_capabilities() {
        let root = tempfile::tempdir().unwrap();
        let first_dir = root.path().join("first/.claude/agents");
        let second_dir = root.path().join("second/.github/agents");
        let old_dir = root.path().join("old/.claude/agents");
        for directory in [&first_dir, &second_dir, &old_dir] {
            std::fs::create_dir_all(directory).unwrap();
        }

        write_agent(&old_dir.join("old.md"), "old", "old");
        write_agent(&first_dir.join("first.md"), "first", "first");
        write_agent(&second_dir.join("second.agent.md"), "second", "second");
        let mut registry = BoundCustomAgentRegistry::with_limits(4, Duration::from_mins(1));
        let _old = registry
            .list(&old_dir.to_string_lossy(), scope())
            .await
            .unwrap();
        let first = registry
            .list_capabilities(&first_dir, scope(), 1)
            .await
            .unwrap();
        let second = registry
            .list_capabilities(&second_dir, scope(), 1)
            .await
            .unwrap();
        registry
            .read(&first[0].detail_selection_token, &scope())
            .await
            .unwrap();
        registry
            .open(&first[0].open_selection_token, &scope())
            .await
            .unwrap();
        registry
            .read(&second[0].detail_selection_token, &scope())
            .await
            .unwrap();
        registry
            .open(&second[0].open_selection_token, &scope())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn aggregate_snapshot_preflight_preserves_registry_on_late_capacity_failure() {
        let root = tempfile::tempdir().unwrap();
        let first_dir = root.path().join("first/.claude/agents");
        let second_dir = root.path().join("second/.github/agents");
        let old_dir = root.path().join("old/.claude/agents");
        for directory in [&first_dir, &second_dir, &old_dir] {
            std::fs::create_dir_all(directory).unwrap();
        }
        write_agent(&old_dir.join("old.md"), "old", "old");
        write_agent(&first_dir.join("first.md"), "first", "first");
        write_agent(&second_dir.join("second.agent.md"), "second", "second");

        let mut registry = BoundCustomAgentRegistry::with_limits(3, Duration::from_mins(1));
        let old = registry
            .list(&old_dir.to_string_lossy(), scope())
            .await
            .unwrap()
            .items
            .remove(0);
        let first = registry
            .stage_capabilities(&first_dir, scope(), 1)
            .await
            .unwrap();
        let second = registry
            .stage_capabilities(&second_dir, scope(), 1)
            .await
            .unwrap();
        let error = registry
            .preflight_staged_capabilities(&[&first, &second])
            .unwrap_err();
        assert!(error.contains("capacity"), "{error}");
        drop((first, second));
        assert_eq!(registry.selections.len(), 1);
        registry.read(&old.selection_token, &scope()).await.unwrap();
    }

    #[tokio::test]
    async fn staged_project_capabilities_revalidate_child_entry_before_commit() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let agents = project.join(".claude/agents");
        std::fs::create_dir_all(&agents).unwrap();
        write_agent(&agents.join("agent.md"), "agent", "inside");
        let held_project = std::fs::File::open(&project).unwrap();
        let registry = BoundCustomAgentRegistry::default();
        let staged = registry
            .stage_capabilities_from_root(
                &held_project,
                &[".claude", "agents"],
                AgentTarget::Claude,
                scope(),
                1,
            )
            .await
            .unwrap();
        assert_eq!(staged.items().len(), 1);
        std::fs::rename(&agents, project.join(".claude/agents-retired")).unwrap();
        std::fs::create_dir(&agents).unwrap();
        write_agent(&agents.join("agent.md"), "replacement", "replacement");
        let error = registry
            .validate_staged_capabilities(&[&staged])
            .unwrap_err();
        assert!(error.contains("directory"), "{error}");
    }

    #[tokio::test]
    async fn snapshot_detail_and_open_capabilities_expire_independently() {
        let (_root, agents) = fixture();
        write_agent(&agents.join("agent.md"), "agent", "inside");
        let mut registry = BoundCustomAgentRegistry::with_limits(2, Duration::ZERO);
        let item = registry
            .list_capabilities(&agents, scope(), 1)
            .await
            .unwrap()
            .remove(0);
        assert!(
            registry
                .read(&item.detail_selection_token, &scope())
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
                .read(&item.detail_selection_token, &scope())
                .await
                .unwrap_err()
                .contains("stale")
        );
    }

    #[tokio::test]
    async fn idle_expiry_purge_drops_custom_agent_capabilities_without_followup_operation() {
        let (_root, agents) = fixture();
        write_agent(&agents.join("agent.md"), "agent", "inside");
        let mut registry = BoundCustomAgentRegistry::with_limits(2, Duration::ZERO);
        registry
            .list_capabilities(&agents, scope(), 1)
            .await
            .unwrap();
        assert_eq!(registry.selections.len(), 2);
        registry.purge_expired_now();
        assert!(registry.selections.is_empty());
        assert!(registry.insertion_order.is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlinked_root_and_file_are_rejected() {
        use std::os::unix::fs::symlink;

        let (root, agents) = fixture();
        write_agent(&agents.join("inside.md"), "inside", "inside");
        let linked_root = root.path().join("linked/.claude/agents");
        std::fs::create_dir_all(linked_root.parent().unwrap()).unwrap();
        symlink(&agents, &linked_root).unwrap();
        let mut registry = BoundCustomAgentRegistry::default();
        assert!(
            registry
                .list(&linked_root.to_string_lossy(), scope())
                .await
                .unwrap_err()
                .contains("open custom-agent directory")
        );

        let outside = root.path().join("outside.md");
        write_agent(&outside, "outside", "outside");
        symlink(&outside, agents.join("escape.md")).unwrap();
        let list = registry
            .list(&agents.to_string_lossy(), scope())
            .await
            .unwrap();
        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].name, "inside");
    }

    #[tokio::test]
    async fn copilot_root_only_selects_agent_markdown() {
        let root = tempfile::tempdir().unwrap();
        let agents = root.path().join("work/.github/agents");
        std::fs::create_dir_all(&agents).unwrap();
        write_agent(&agents.join("reviewer.agent.md"), "reviewer", "inside");
        write_agent(&agents.join("README.md"), "readme", "not an agent");

        let mut registry = BoundCustomAgentRegistry::default();
        let list = registry
            .list(&agents.to_string_lossy(), scope())
            .await
            .unwrap();
        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].name, "reviewer");
        assert_eq!(list.items[0].target, AgentTarget::VsCode);
    }

    #[tokio::test]
    async fn claude_root_owns_target_even_for_agent_suffix() {
        let (_root, agents) = fixture();
        write_agent(&agents.join("reviewer.agent.md"), "reviewer", "inside");
        let mut registry = BoundCustomAgentRegistry::default();
        let list = registry
            .list(&agents.to_string_lossy(), scope())
            .await
            .unwrap();
        assert_eq!(list.items[0].target, AgentTarget::Claude);
        let detail = registry
            .read(&list.items[0].selection_token, &scope())
            .await
            .unwrap();
        assert_eq!(detail.target, AgentTarget::Claude);
    }

    #[tokio::test]
    async fn replacement_and_rename_fail_closed() {
        let (root, agents) = fixture();
        let path = agents.join("agent.md");
        write_agent(&path, "original", "original");
        let mut replacement_registry = BoundCustomAgentRegistry::default();
        let replacement = replacement_registry
            .list(&agents.to_string_lossy(), scope())
            .await
            .unwrap();
        std::fs::rename(&path, agents.join("old.md")).unwrap();
        write_agent(&path, "replacement", "replacement");
        assert!(
            replacement_registry
                .read(&replacement.items[0].selection_token, &scope())
                .await
                .unwrap_err()
                .contains("listed file")
        );

        let mut rename_registry = BoundCustomAgentRegistry::default();
        let rename = rename_registry
            .list(&agents.to_string_lossy(), scope())
            .await
            .unwrap();
        let renamed_root = root.path().join("renamed-agents");
        std::fs::rename(&agents, &renamed_root).unwrap();
        assert!(
            rename_registry
                .read(&rename.items[0].selection_token, &scope())
                .await
                .unwrap_err()
                .contains("directory")
        );
    }

    #[tokio::test]
    async fn expiry_capacity_scope_and_path_escape_fail_closed() {
        let (_root, agents) = fixture();
        write_agent(&agents.join("agent.md"), "agent", "inside");
        let mut expired = BoundCustomAgentRegistry::with_limits(1, Duration::ZERO);
        let list = expired
            .list(&agents.to_string_lossy(), scope())
            .await
            .unwrap();
        assert!(
            expired
                .read(&list.items[0].selection_token, &scope())
                .await
                .unwrap_err()
                .contains("expired")
        );

        let mut capped = BoundCustomAgentRegistry::with_limits(1, Duration::from_mins(1));
        let first = capped
            .list(&agents.to_string_lossy(), scope())
            .await
            .unwrap();
        let second = capped
            .list(&agents.to_string_lossy(), scope())
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
        assert!(
            capped
                .list("../escape", scope())
                .await
                .unwrap_err()
                .contains("absolute")
        );
    }

    #[tokio::test]
    async fn aggregate_summary_budget_is_strict() {
        let (_root, agents) = fixture();
        for index in 0..128 {
            std::fs::write(
                agents.join(format!("{index:04}.md")),
                format!(
                    "---\nname: agent-{index:04}\ndescription: {}\nmodel: sonnet\ntools: [Read, Grep]\n---\ninside",
                    "d".repeat(MAX_SUMMARY_DESCRIPTION_BYTES)
                ),
            )
            .unwrap();
        }
        let mut registry = BoundCustomAgentRegistry::default();
        let list = registry
            .list(&agents.to_string_lossy(), scope())
            .await
            .unwrap();
        assert!(list.items.len() < 128);
        assert!(serde_json::to_vec(&list).unwrap().len() <= MAX_CUSTOM_AGENT_SUMMARY_BYTES);
    }
}

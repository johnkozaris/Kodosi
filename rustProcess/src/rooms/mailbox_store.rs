use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, BufRead, BufReader, ErrorKind, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use kodosi_domain::ids::{SessionId, UserId};

const MAX_AGENT_PROFILES: usize = 1_024;
const MAX_AGENT_PROFILE_BYTES: usize = 64 * 1024;
const MAX_AGENT_KIND_BYTES: usize = 256;
const MAX_AGENT_DESCRIPTION_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AgentProfile {
    pub(crate) session_id: SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) incarnation_id: Option<Uuid>,
    pub(crate) owner_user_id: UserId,
    pub(crate) agent_kind: String,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) description: String,
    pub(crate) last_described_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MailboxDestination {
    pub(crate) session_id: SessionId,
    pub(crate) incarnation_id: Uuid,
}

impl MailboxDestination {
    pub(crate) const fn new(session_id: SessionId, incarnation_id: Uuid) -> Self {
        Self {
            session_id,
            incarnation_id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MailboxAddress {
    Legacy(SessionId),
    Incarnation(MailboxDestination),
}

impl From<SessionId> for MailboxAddress {
    fn from(session_id: SessionId) -> Self {
        Self::Legacy(session_id)
    }
}

impl From<MailboxDestination> for MailboxAddress {
    fn from(destination: MailboxDestination) -> Self {
        Self::Incarnation(destination)
    }
}

impl MailboxAddress {
    fn stem(self) -> String {
        match self {
            Self::Legacy(session_id) => session_id.to_string(),
            Self::Incarnation(destination) => {
                format!("{}.{}", destination.session_id, destination.incarnation_id)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum MailboxEntry {
    AgentMessage {
        request_id: String,
        from_session_id: SessionId,
        from_description: Option<String>,
        body: String,
        in_reply_to: Option<String>,
        at: OffsetDateTime,
    },
    RoomChat {
        room_id: String,
        room_name: Option<String>,
        author_user_id: UserId,
        author_session_id: Option<SessionId>,
        #[serde(default)]
        recipient_session_ids: Vec<SessionId>,
        #[serde(default)]
        recipient_user_ids: Vec<UserId>,
        body: String,
        seq: i64,
        at: OffsetDateTime,
    },
    RoomTask {
        room_id: String,
        room_name: Option<String>,
        task_id: String,
        title: String,
        status: String,
        #[serde(default, alias = "taskRevision", alias = "expectedRevision")]
        revision: i64,
        #[serde(default, alias = "assignedSessionId")]
        assigned_session_id: Option<SessionId>,
        #[serde(default, alias = "assignedSessionIncarnationId")]
        assigned_session_incarnation_id: Option<Uuid>,
        #[serde(default, alias = "deliveryReason")]
        delivery_reason: RoomTaskDeliveryReason,
        at: OffsetDateTime,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RoomTaskDeliveryReason {
    #[default]
    CurrentAssignmentOrUpdate,
    ReassignedPreviousAssignee,
    UnassignedPreviousAssignee,
}

impl MailboxEntry {
    pub(crate) fn stable_event_id(&self) -> String {
        match self {
            Self::AgentMessage { request_id, .. } => request_id.clone(),
            Self::RoomChat { room_id, seq, .. } => format!("chat:{room_id}:{seq}"),
            Self::RoomTask {
                room_id,
                task_id,
                status,
                at,
                ..
            } => format!(
                "task:{room_id}:{task_id}:{status}:{}",
                at.unix_timestamp_nanos()
            ),
        }
    }

    fn delivery_key(&self) -> Option<String> {
        match self {
            Self::AgentMessage { .. } => None,
            Self::RoomChat { .. } | Self::RoomTask { .. } => Some(self.stable_event_id()),
        }
    }
}

#[cfg_attr(not(any(test, feature = "cli")), allow(dead_code))]
#[derive(Debug, Clone)]
pub(crate) struct MailboxOffer {
    pub(crate) destination: MailboxDestination,
    #[cfg(feature = "cli")]
    pub(crate) entry: MailboxEntry,
    pub(crate) event_id: String,
    pub(crate) previous_cursor: u64,
    pub(crate) cursor: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RoomTaskDeliveryCursor {
    pub(crate) fingerprint: String,
    pub(crate) assigned_session_id: Option<SessionId>,
    #[serde(default)]
    pub(crate) assigned_session_incarnation_id: Option<Uuid>,
    #[serde(default)]
    pub(crate) mailbox_destination: Option<MailboxDestination>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RoomDeliveryState {
    version: u8,
    pub(crate) chat_cursors: BTreeMap<String, i64>,
    pub(crate) task_cursors: BTreeMap<String, RoomTaskDeliveryCursor>,
    #[serde(default)]
    pub(crate) next_chat_room_id: Option<String>,
    #[serde(default)]
    pub(crate) next_task_room_id: Option<String>,
    #[serde(default)]
    pub(crate) chat_rooms_remaining: Option<usize>,
    #[serde(default)]
    pub(crate) task_rooms_remaining: Option<usize>,
}

impl Default for RoomDeliveryState {
    fn default() -> Self {
        Self {
            version: ROOM_DELIVERY_STATE_VERSION,
            chat_cursors: BTreeMap::new(),
            task_cursors: BTreeMap::new(),
            next_chat_room_id: None,
            next_task_room_id: None,
            chat_rooms_remaining: None,
            task_rooms_remaining: None,
        }
    }
}

impl RoomDeliveryState {
    pub(crate) fn retain_rooms(&mut self, room_ids: &BTreeSet<String>) {
        self.chat_cursors
            .retain(|room_id, _| room_ids.contains(room_id));
        self.task_cursors.retain(|key, _| {
            key.split_once('/')
                .is_some_and(|(room_id, _)| room_ids.contains(room_id))
        });
    }
}

fn is_valid_agent_profile(profile: &AgentProfile) -> bool {
    !profile.agent_kind.is_empty()
        && profile.agent_kind.len() <= MAX_AGENT_KIND_BYTES
        && !profile.description.is_empty()
        && profile.description.len() <= MAX_AGENT_DESCRIPTION_BYTES
}

#[derive(Clone)]
pub(crate) struct AgentRoomStore {
    root: PathBuf,
}

#[derive(Debug)]
struct ScannedMailboxLine {
    #[cfg_attr(not(any(test, feature = "cli")), allow(dead_code))]
    bytes: Vec<u8>,
    total_len: usize,
    complete: bool,
}

fn scan_mailbox_line(
    reader: &mut impl BufRead,
    max_scan_bytes: usize,
) -> io::Result<ScannedMailboxLine> {
    let capture_limit = MAX_MAILBOX_ENTRY_BYTES.saturating_add(1);
    let mut bytes = Vec::new();
    let mut total_len = 0_usize;
    loop {
        if total_len >= max_scan_bytes {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "mailbox record exceeds bounded scan limit before newline",
            ));
        }
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(ScannedMailboxLine {
                bytes,
                total_len,
                complete: false,
            });
        }
        let remaining_scan = max_scan_bytes.saturating_sub(total_len);
        let bounded = &buffer[..buffer.len().min(remaining_scan)];
        let newline = bounded.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(bounded.len(), |index| index.saturating_add(1));
        let remaining = capture_limit.saturating_sub(bytes.len());
        bytes.extend_from_slice(&buffer[..consumed.min(remaining)]);
        reader.consume(consumed);
        total_len = total_len.saturating_add(consumed);
        if newline.is_some() {
            return Ok(ScannedMailboxLine {
                bytes,
                total_len,
                complete: true,
            });
        }
        if total_len >= max_scan_bytes {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "mailbox record exceeds bounded scan limit before newline",
            ));
        }
    }
}

fn room_agent_root(home: &Path) -> PathBuf {
    home.join(".kodosi").join("agent-room")
}

impl AgentRoomStore {
    pub(crate) fn open() -> io::Result<Self> {
        if let Some(root) =
            crate::support::storage::paths::room_agent_root().map_err(app_error_to_io)?
        {
            return Self::open_at(root);
        }
        let home = directories::BaseDirs::new()
            .map(|dirs| dirs.home_dir().to_path_buf())
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::NotFound,
                    "platform home directory is unavailable",
                )
            })?;
        Self::open_at(room_agent_root(&home))
    }

    pub(crate) fn open_at(root: PathBuf) -> io::Result<Self> {
        crate::support::platform::fs::ensure_dir(&root).map_err(app_error_to_io)?;
        crate::support::platform::fs::ensure_dir(&root.join("profiles"))
            .map_err(app_error_to_io)?;
        crate::support::platform::fs::ensure_dir(&root.join("mailbox")).map_err(app_error_to_io)?;
        crate::support::platform::fs::ensure_dir(&root.join("room-delivery"))
            .map_err(app_error_to_io)?;
        Ok(Self { root })
    }

    #[cfg(any(test, feature = "cli"))]
    pub(crate) fn put_profile(&self, profile: &AgentProfile) -> io::Result<()> {
        if !is_valid_agent_profile(profile) {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "agent profile text exceeds bounds",
            ));
        }
        let profiles = self.list_profiles()?;
        if profiles.len() >= MAX_AGENT_PROFILES
            && !profiles
                .iter()
                .any(|existing| existing.session_id == profile.session_id)
        {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "agent profile roster exceeds retention limit",
            ));
        }
        let serialized = serde_json::to_vec_pretty(profile)?;
        if serialized.len() > MAX_AGENT_PROFILE_BYTES {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "agent profile exceeds byte limit",
            ));
        }
        let path = self
            .root
            .join("profiles")
            .join(profile_filename(profile.session_id));
        atomic_write(&path, &serialized)
    }

    pub(crate) fn remove_profile(&self, session_id: SessionId) -> io::Result<()> {
        let path = self
            .root
            .join("profiles")
            .join(profile_filename(session_id));
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn list_profiles(&self) -> io::Result<Vec<AgentProfile>> {
        let dir = self.root.join("profiles");
        let mut profiles = Vec::new();
        for entry in fs::read_dir(&dir)?.take(MAX_AGENT_PROFILES.saturating_add(1)) {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let metadata = entry.metadata()?;
                if metadata.len() > MAX_AGENT_PROFILE_BYTES as u64 {
                    continue;
                }
                let bytes = fs::read(entry.path())?;
                if let Ok(profile) = serde_json::from_slice::<AgentProfile>(&bytes)
                    && is_valid_agent_profile(&profile)
                {
                    profiles.push(profile);
                }
            }
        }
        if profiles.len() > MAX_AGENT_PROFILES {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "agent profile roster exceeds retention limit",
            ));
        }
        profiles.sort_by_key(|p| p.session_id);
        Ok(profiles)
    }

    #[cfg(any(test, feature = "cli"))]
    pub(crate) fn visible_to(&self, viewer_owner: UserId) -> io::Result<Vec<AgentProfile>> {
        Ok(self
            .list_profiles()?
            .into_iter()
            .filter(|p| p.owner_user_id == viewer_owner)
            .collect())
    }

    #[cfg(test)]
    pub(crate) fn enqueue(&self, target: SessionId, entry: &MailboxEntry) -> io::Result<()> {
        self.enqueue_inner(MailboxAddress::Legacy(target), entry, false)
            .map(|_| ())
    }

    #[cfg(any(test, feature = "cli"))]
    pub(crate) fn enqueue_for(
        &self,
        target: MailboxDestination,
        entry: &MailboxEntry,
    ) -> io::Result<()> {
        self.enqueue_inner(target.into(), entry, false).map(|_| ())
    }

    pub(crate) fn enqueue_deduplicated(
        &self,
        target: MailboxDestination,
        entry: &MailboxEntry,
    ) -> io::Result<bool> {
        self.enqueue_inner(target.into(), entry, true)
    }

    fn enqueue_inner(
        &self,
        target: MailboxAddress,
        entry: &MailboxEntry,
        deduplicate: bool,
    ) -> io::Result<bool> {
        use std::io::Write;

        let path = self.root.join("mailbox").join(mailbox_filename(target));
        let mut serialized = serde_json::to_vec(entry)?;
        serialized.push(b'\n');
        if serialized.len() > MAX_MAILBOX_ENTRY_BYTES {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "mailbox entry exceeds size limit",
            ));
        }
        let _guard = self.lock_mailbox_read(target)?;
        repair_torn_mailbox_tail(&path)?;
        if deduplicate
            && let Some(delivery_key) = entry.delivery_key()
            && mailbox_contains_delivery_key(&path, &delivery_key)?
        {
            return Ok(false);
        }
        self.compact_mailbox_if_needed(target, serialized.len())?;
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        crate::support::platform::fs::set_file_permissions(&path).map_err(app_error_to_io)?;
        (&file).write_all(&serialized)?;
        Ok(true)
    }

    #[cfg(test)]
    pub(crate) fn drain_unread(
        &self,
        session_id: SessionId,
    ) -> io::Result<(Vec<MailboxEntry>, u64)> {
        self.read_mailbox_limited(
            session_id.into(),
            MailboxReadStart::Persisted,
            MAX_UNREAD_BYTES_PER_DRAIN,
            MAX_UNREAD_ENTRIES_PER_DRAIN,
        )
    }

    #[cfg_attr(not(any(test, feature = "cli")), allow(dead_code))]
    pub(crate) fn drain_unread_page(
        &self,
        destination: impl Into<MailboxAddress>,
        max_entries: usize,
    ) -> io::Result<(Vec<MailboxEntry>, u64)> {
        self.read_mailbox_limited(
            destination.into(),
            MailboxReadStart::Persisted,
            MAX_MAILBOX_RECORD_BYTES,
            max_entries,
        )
    }

    #[cfg_attr(not(any(test, feature = "cli")), allow(dead_code))]
    pub(crate) fn peek_unread_offer(
        &self,
        destination: MailboxDestination,
    ) -> io::Result<Option<MailboxOffer>> {
        let _guard = self.lock_mailbox_read(destination)?;
        let (_, previous_cursor) = self.drain_unread_page(destination, 0)?;
        let (mut entries, cursor) = self.read_after_cursor_page(destination, previous_cursor, 1)?;
        let Some(entry) = entries.pop() else {
            return Ok(None);
        };
        let event_id = entry.stable_event_id();
        Ok(Some(MailboxOffer {
            destination,
            #[cfg(feature = "cli")]
            entry,
            event_id,
            previous_cursor,
            cursor,
        }))
    }

    #[cfg(test)]
    pub(crate) fn commit_offer(
        &self,
        destination: MailboxDestination,
        offer: &MailboxOffer,
    ) -> io::Result<()> {
        self.reserve_offer(destination, offer)?.commit()
    }

    #[cfg(any(test, feature = "cli"))]
    pub(crate) fn reserve_offer(
        &self,
        destination: MailboxDestination,
        offer: &MailboxOffer,
    ) -> io::Result<MailboxOfferReservation> {
        let guard = self.lock_mailbox_read(destination)?;
        self.validate_offer_locked(destination, offer)?;
        drop(guard);
        Ok(MailboxOfferReservation {
            store: self.clone(),
            destination,
            offer: offer.clone(),
        })
    }

    #[cfg(any(test, feature = "cli"))]
    fn validate_offer_locked(
        &self,
        destination: MailboxDestination,
        offer: &MailboxOffer,
    ) -> io::Result<()> {
        if offer.destination != destination {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "offered mailbox destination does not match the committing session incarnation",
            ));
        }
        let (_, current_cursor) = self.drain_unread_page(destination, 0)?;
        if current_cursor != offer.previous_cursor {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "mailbox cursor changed while event was offered: expected {}, found {current_cursor}",
                    offer.previous_cursor
                ),
            ));
        }
        let (mut entries, cursor) =
            self.read_after_cursor_page(destination, offer.previous_cursor, 1)?;
        let Some(entry) = entries.pop() else {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "offered mailbox event is no longer unread",
            ));
        };
        if cursor != offer.cursor || entry.stable_event_id() != offer.event_id {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "offered mailbox event or cursor no longer matches durable state",
            ));
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn read_after_cursor(
        &self,
        session_id: SessionId,
        cursor: u64,
    ) -> io::Result<(Vec<MailboxEntry>, u64)> {
        self.read_mailbox_limited(
            session_id.into(),
            MailboxReadStart::Explicit(cursor),
            MAX_UNREAD_BYTES_PER_DRAIN,
            MAX_UNREAD_ENTRIES_PER_DRAIN,
        )
    }

    #[cfg_attr(not(any(test, feature = "cli")), allow(dead_code))]
    pub(crate) fn read_after_cursor_page(
        &self,
        destination: impl Into<MailboxAddress>,
        cursor: u64,
        max_entries: usize,
    ) -> io::Result<(Vec<MailboxEntry>, u64)> {
        self.read_mailbox_limited(
            destination.into(),
            MailboxReadStart::Explicit(cursor),
            MAX_MAILBOX_RECORD_BYTES,
            max_entries,
        )
    }

    #[cfg_attr(not(any(test, feature = "cli")), allow(dead_code))]
    fn read_mailbox_limited(
        &self,
        destination: MailboxAddress,
        start: MailboxReadStart,
        max_bytes: usize,
        max_entries: usize,
    ) -> io::Result<(Vec<MailboxEntry>, u64)> {
        let mailbox_path = self
            .root
            .join("mailbox")
            .join(mailbox_filename(destination));
        let mut file = match fs::File::open(&mailbox_path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return match start {
                    MailboxReadStart::Persisted | MailboxReadStart::Explicit(0) => {
                        Ok((Vec::new(), 0))
                    }
                    MailboxReadStart::Explicit(cursor) => Err(io::Error::new(
                        ErrorKind::InvalidInput,
                        format!("mailbox cursor {cursor} is ahead of empty mailbox"),
                    )),
                };
            }
            Err(e) => return Err(e),
        };
        let layout = read_mailbox_layout(&mut file)?;
        let cursor = match start {
            MailboxReadStart::Persisted => {
                resolve_cursor_offset(&self.cursor_path(destination), &mut file, layout)?
            }
            MailboxReadStart::Explicit(cursor) => {
                layout.validate_explicit_cursor(cursor)?;
                cursor
            }
        };
        let file_offset = layout.file_offset(cursor)?;
        validate_cursor_boundary(&mut file, file_offset)?;
        file.seek(SeekFrom::Start(file_offset))?;
        let mut reader = BufReader::new(file);
        let mut next_cursor = cursor;
        let mut bytes_this_pass = 0_usize;
        let mut unread = Vec::new();
        while bytes_this_pass < max_bytes && unread.len() < max_entries {
            let remaining = max_bytes.saturating_sub(bytes_this_pass);
            let scan_limit = remaining.min(MAX_MAILBOX_RECORD_BYTES);
            let line = match scan_mailbox_line(&mut reader, scan_limit) {
                Ok(line) => line,
                Err(error)
                    if scan_limit < MAX_MAILBOX_RECORD_BYTES
                        && error.kind() == ErrorKind::InvalidData =>
                {
                    break;
                }
                Err(error) => return Err(error),
            };
            if line.total_len == 0 {
                break;
            }
            if !line.complete {
                break;
            }
            next_cursor =
                next_cursor.saturating_add(u64::try_from(line.total_len).unwrap_or(u64::MAX));
            bytes_this_pass = bytes_this_pass.saturating_add(line.total_len);
            if line.total_len > MAX_MAILBOX_ENTRY_BYTES {
                continue;
            }
            if let Ok(entry) = serde_json::from_slice::<MailboxEntry>(&line.bytes) {
                unread.push(entry);
            }
        }
        Ok((unread, next_cursor))
    }

    pub(crate) fn set_cursor(
        &self,
        destination: impl Into<MailboxAddress>,
        cursor: u64,
    ) -> io::Result<()> {
        atomic_write(
            &self.cursor_path(destination),
            format!("bytes:{cursor}").as_bytes(),
        )
    }

    pub(crate) fn load_room_delivery_state(
        &self,
        account_user_id: UserId,
    ) -> io::Result<RoomDeliveryState> {
        let path = self.room_delivery_state_path(account_user_id);
        let mut state = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<RoomDeliveryState>(&bytes)?,
            Err(error) if error.kind() == ErrorKind::NotFound => RoomDeliveryState::default(),
            Err(error) => return Err(error),
        };
        if state.version == 1 {
            state.version = ROOM_DELIVERY_STATE_VERSION;
        } else if state.version != ROOM_DELIVERY_STATE_VERSION {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                format!("unsupported room delivery state version {}", state.version),
            ));
        }
        if state.chat_cursors.len() > MAX_ROOM_DELIVERY_CURSORS
            || state.task_cursors.len() > MAX_ROOM_TASK_DELIVERY_CURSORS
        {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "room delivery state exceeds retention bounds",
            ));
        }
        Ok(state)
    }

    pub(crate) fn save_room_delivery_state(
        &self,
        account_user_id: UserId,
        state: &RoomDeliveryState,
    ) -> io::Result<()> {
        if state.chat_cursors.len() > MAX_ROOM_DELIVERY_CURSORS
            || state.task_cursors.len() > MAX_ROOM_TASK_DELIVERY_CURSORS
        {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "room delivery state exceeds retention bounds",
            ));
        }
        atomic_write(
            &self.room_delivery_state_path(account_user_id),
            &serde_json::to_vec(state)?,
        )
    }

    pub(crate) fn lock_mailbox_read(
        &self,
        destination: impl Into<MailboxAddress>,
    ) -> io::Result<MailboxReadGuard> {
        let path = self
            .root
            .join("mailbox")
            .join(format!("{}.lock", destination.into().stem()));
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)?;
        file.lock()?;
        Ok(MailboxReadGuard { file })
    }

    fn cursor_path(&self, destination: impl Into<MailboxAddress>) -> PathBuf {
        self.root
            .join("mailbox")
            .join(format!("{}.cursor", destination.into().stem()))
    }

    fn room_delivery_state_path(&self, account_user_id: UserId) -> PathBuf {
        self.root
            .join("room-delivery")
            .join(format!("{account_user_id}.json"))
    }

    fn compact_mailbox_if_needed(
        &self,
        destination: MailboxAddress,
        incoming_bytes: usize,
    ) -> io::Result<()> {
        let path = self
            .root
            .join("mailbox")
            .join(mailbox_filename(destination));
        let current_len = fs::metadata(&path).map_or(0, |metadata| metadata.len());
        let incoming = u64::try_from(incoming_bytes).unwrap_or(u64::MAX);
        if current_len.saturating_add(incoming) <= MAX_MAILBOX_BYTES {
            return Ok(());
        }
        let mut file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let layout = read_mailbox_layout(&mut file)?;
        let consumed = resolve_cursor_offset(&self.cursor_path(destination), &mut file, layout)?;
        if consumed <= layout.base_cursor {
            return Err(io::Error::new(
                ErrorKind::WouldBlock,
                "mailbox retention limit reached with unread entries",
            ));
        }
        file.seek(SeekFrom::Start(layout.file_offset(consumed)?))?;
        let mut remaining = Vec::new();
        file.read_to_end(&mut remaining)?;
        let mut compacted = encode_mailbox_header(consumed)?;
        compacted.extend_from_slice(&remaining);
        if u64::try_from(compacted.len())
            .unwrap_or(u64::MAX)
            .saturating_add(incoming)
            > MAX_MAILBOX_BYTES
        {
            return Err(io::Error::new(
                ErrorKind::WouldBlock,
                "mailbox retention limit reached",
            ));
        }

        self.set_cursor(destination, consumed)?;
        atomic_write(&path, &compacted)
    }
}

pub(crate) struct MailboxReadGuard {
    file: fs::File,
}

impl Drop for MailboxReadGuard {
    fn drop(&mut self) {
        drop(self.file.unlock());
    }
}

#[must_use = "dropping a mailbox offer reservation leaves the event unread"]
#[cfg(any(test, feature = "cli"))]
pub(crate) struct MailboxOfferReservation {
    store: AgentRoomStore,
    destination: MailboxDestination,
    offer: MailboxOffer,
}

#[cfg(any(test, feature = "cli"))]
impl MailboxOfferReservation {
    pub(crate) fn commit(self) -> io::Result<()> {
        let _guard = self.store.lock_mailbox_read(self.destination)?;
        self.store
            .validate_offer_locked(self.destination, &self.offer)?;
        self.store.set_cursor(self.destination, self.offer.cursor)
    }
}

fn profile_filename(session_id: SessionId) -> String {
    format!("{session_id}.json")
}

fn mailbox_filename(destination: impl Into<MailboxAddress>) -> String {
    format!("{}.jsonl", destination.into().stem())
}

#[cfg_attr(not(any(test, feature = "cli")), allow(dead_code))]
#[derive(Clone, Copy)]
enum MailboxReadStart {
    Persisted,
    Explicit(u64),
}

#[derive(Clone, Copy)]
struct MailboxLayout {
    base_cursor: u64,
    data_start: u64,
    data_len: u64,
}

impl MailboxLayout {
    fn end_cursor(self) -> io::Result<u64> {
        self.base_cursor
            .checked_add(self.data_len)
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "mailbox cursor range overflow"))
    }

    fn validate_explicit_cursor(self, cursor: u64) -> io::Result<()> {
        let end = self.end_cursor()?;
        if cursor < self.base_cursor {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "mailbox cursor {cursor} expired; retained history starts at {}",
                    self.base_cursor
                ),
            ));
        }
        if cursor > end {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                format!("mailbox cursor {cursor} is ahead of current cursor {end}"),
            ));
        }
        Ok(())
    }

    fn file_offset(self, cursor: u64) -> io::Result<u64> {
        self.validate_explicit_cursor(cursor)?;
        Ok(self
            .data_start
            .saturating_add(cursor.saturating_sub(self.base_cursor)))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MailboxHeader {
    #[serde(rename = "_kodosiMailboxBase")]
    base_cursor: u64,
}

enum PersistedMailboxCursor {
    Bytes(u64),
    LegacyLines(u64),
}

fn read_cursor(path: &Path) -> io::Result<PersistedMailboxCursor> {
    match fs::read_to_string(path) {
        Ok(s) => {
            let trimmed = s.trim();
            if let Some(bytes) = trimmed.strip_prefix("bytes:") {
                return bytes
                    .parse::<u64>()
                    .map(PersistedMailboxCursor::Bytes)
                    .map_err(|error| {
                        io::Error::new(
                            ErrorKind::InvalidData,
                            format!("invalid mailbox byte cursor: {error}"),
                        )
                    });
            }
            trimmed
                .parse::<u64>()
                .map(PersistedMailboxCursor::LegacyLines)
                .map_err(|error| {
                    io::Error::new(
                        ErrorKind::InvalidData,
                        format!("invalid legacy mailbox line cursor: {error}"),
                    )
                })
        }
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(PersistedMailboxCursor::Bytes(0)),
        Err(e) => Err(e),
    }
}

fn read_mailbox_layout(file: &mut fs::File) -> io::Result<MailboxLayout> {
    const MAX_HEADER_BYTES: u64 = 128;

    let file_len = file.metadata()?.len();
    file.seek(SeekFrom::Start(0))?;
    let mut first_line = Vec::new();
    let mut reader = BufReader::new(&mut *file);
    let bytes = Read::by_ref(&mut reader)
        .take(MAX_HEADER_BYTES)
        .read_until(b'\n', &mut first_line)?;
    drop(reader);
    let header = first_line
        .ends_with(b"\n")
        .then(|| serde_json::from_slice::<MailboxHeader>(&first_line).ok())
        .flatten();
    file.seek(SeekFrom::Start(0))?;
    let Some(header) = header else {
        return Ok(MailboxLayout {
            base_cursor: 0,
            data_start: 0,
            data_len: file_len,
        });
    };
    let data_start = u64::try_from(bytes)
        .map_err(|_| io::Error::new(ErrorKind::InvalidData, "mailbox header length overflow"))?;
    Ok(MailboxLayout {
        base_cursor: header.base_cursor,
        data_start,
        data_len: file_len.saturating_sub(data_start),
    })
}

fn encode_mailbox_header(base_cursor: u64) -> io::Result<Vec<u8>> {
    let mut header = serde_json::to_vec(&MailboxHeader { base_cursor })?;
    header.push(b'\n');
    Ok(header)
}

fn resolve_cursor_offset(
    path: &Path,
    file: &mut fs::File,
    layout: MailboxLayout,
) -> io::Result<u64> {
    match read_cursor(path)? {
        PersistedMailboxCursor::Bytes(cursor) => {
            Ok(cursor.clamp(layout.base_cursor, layout.end_cursor()?))
        }
        PersistedMailboxCursor::LegacyLines(lines) => {
            let cursor = migrate_legacy_line_cursor(file, layout, lines)?;
            atomic_write(path, format!("bytes:{cursor}").as_bytes())?;
            Ok(cursor)
        }
    }
}

fn migrate_legacy_line_cursor(
    file: &mut fs::File,
    layout: MailboxLayout,
    lines: u64,
) -> io::Result<u64> {
    if layout.base_cursor != 0 {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "legacy line cursor cannot be migrated after mailbox compaction",
        ));
    }
    if lines > MAX_LEGACY_CURSOR_MIGRATION_LINES {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "legacy mailbox line cursor exceeds migration bound",
        ));
    }

    file.seek(SeekFrom::Start(layout.data_start))?;
    let mut reader = BufReader::new(&mut *file);
    let mut consumed = 0_u64;
    for _ in 0..lines {
        let remaining = usize::try_from(MAX_LEGACY_CURSOR_MIGRATION_BYTES.saturating_sub(consumed))
            .unwrap_or(usize::MAX)
            .min(MAX_MAILBOX_RECORD_BYTES);
        let line = scan_mailbox_line(&mut reader, remaining)?;
        if line.total_len == 0 || !line.complete {
            break;
        }
        consumed = consumed
            .checked_add(u64::try_from(line.total_len).unwrap_or(u64::MAX))
            .ok_or_else(|| {
                io::Error::new(ErrorKind::InvalidData, "legacy cursor migration overflow")
            })?;
        if consumed > MAX_LEGACY_CURSOR_MIGRATION_BYTES {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "legacy mailbox cursor migration exceeds byte bound",
            ));
        }
    }
    Ok(consumed.min(layout.end_cursor()?))
}

#[cfg_attr(not(any(test, feature = "cli")), allow(dead_code))]
fn validate_cursor_boundary(file: &mut fs::File, offset: u64) -> io::Result<()> {
    if offset == 0 {
        return Ok(());
    }
    file.seek(SeekFrom::Start(offset.saturating_sub(1)))?;
    let mut byte = [0_u8; 1];
    file.read_exact(&mut byte)?;
    if byte[0] != b'\n' {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "mailbox cursor is not at a record boundary",
        ));
    }
    Ok(())
}

fn repair_torn_mailbox_tail(path: &Path) -> io::Result<()> {
    const SCAN_CHUNK_BYTES: u64 = 64 * 1024;

    let mut file = match fs::OpenOptions::new().read(true).write(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let len = file.metadata()?.len();
    if len == 0 {
        return Ok(());
    }

    file.seek(SeekFrom::Start(len - 1))?;
    let mut last = [0_u8; 1];
    file.read_exact(&mut last)?;
    if last[0] == b'\n' {
        return Ok(());
    }

    let mut scan_end = len;
    let mut buffer = Vec::new();
    while scan_end > 0 {
        let scan_start = scan_end.saturating_sub(SCAN_CHUNK_BYTES);
        let chunk_len = usize::try_from(scan_end - scan_start)
            .map_err(|_| io::Error::new(ErrorKind::InvalidData, "mailbox scan length overflow"))?;
        buffer.resize(chunk_len, 0);
        file.seek(SeekFrom::Start(scan_start))?;
        file.read_exact(&mut buffer)?;
        if let Some(newline) = buffer.iter().rposition(|byte| *byte == b'\n') {
            let retained = scan_start
                .saturating_add(u64::try_from(newline).unwrap_or(u64::MAX))
                .saturating_add(1);
            file.set_len(retained)?;
            return Ok(());
        }
        scan_end = scan_start;
    }
    file.set_len(0)
}

fn mailbox_contains_delivery_key(path: &Path, expected: &str) -> io::Result<bool> {
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let layout = read_mailbox_layout(&mut file)?;
    file.seek(SeekFrom::Start(layout.data_start))?;
    let reader = BufReader::new(file);
    for line in reader.split(b'\n') {
        let line = line?;
        let Ok(entry) = serde_json::from_slice::<MailboxEntry>(&line) else {
            continue;
        };
        if entry.delivery_key().as_deref() == Some(expected) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    crate::support::storage::atomic_file::atomic_write(
        path,
        bytes,
        crate::support::storage::atomic_file::FileMode::UserPrivate,
    )
    .map_err(app_error_to_io)
}

fn app_error_to_io(error: crate::AppError) -> io::Error {
    match error {
        crate::AppError::Io(source) => source,
        other => io::Error::other(other),
    }
}

const MAX_MAILBOX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_MAILBOX_ENTRY_BYTES: usize = 256 * 1024;
const MAX_MAILBOX_RECORD_BYTES: usize = MAX_MAILBOX_ENTRY_BYTES + 1;
#[cfg(test)]
const MAX_UNREAD_BYTES_PER_DRAIN: usize = 512 * 1024;
#[cfg(test)]
const MAX_UNREAD_ENTRIES_PER_DRAIN: usize = 256;
const MAX_LEGACY_CURSOR_MIGRATION_BYTES: u64 = MAX_MAILBOX_BYTES;
const MAX_LEGACY_CURSOR_MIGRATION_LINES: u64 = 100_000;
pub(crate) const MAX_ROOM_DELIVERY_CURSORS: usize = 4_096;
pub(crate) const MAX_ROOM_TASK_DELIVERY_CURSORS: usize = 4_096;
const ROOM_DELIVERY_STATE_VERSION: u8 = 2;

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (tempfile::TempDir, AgentRoomStore) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = AgentRoomStore::open_at(dir.path().to_path_buf()).expect("open store");
        (dir, store)
    }

    #[test]
    fn room_agent_root_is_platform_home_relative() {
        assert_eq!(
            room_agent_root(std::path::Path::new(r"C:\Users\John")),
            std::path::PathBuf::from(r"C:\Users\John")
                .join(".kodosi")
                .join("agent-room")
        );
        assert_eq!(
            room_agent_root(std::path::Path::new("/Users/john")),
            std::path::PathBuf::from("/Users/john/.kodosi/agent-room")
        );
    }

    fn user(id: u128) -> UserId {
        UserId::try_from(format!("{id:032x}").as_str()).expect("uuid")
    }

    fn message(request_id: &str, body: &str) -> MailboxEntry {
        MailboxEntry::AgentMessage {
            request_id: request_id.to_owned(),
            from_session_id: SessionId::new(),
            from_description: None,
            body: body.to_owned(),
            in_reply_to: None,
            at: OffsetDateTime::now_utc(),
        }
    }

    fn room_chat(room_id: &str, seq: i64) -> MailboxEntry {
        MailboxEntry::RoomChat {
            room_id: room_id.to_owned(),
            room_name: Some("Engineering".to_owned()),
            author_user_id: user(2),
            author_session_id: None,
            recipient_session_ids: Vec::new(),
            recipient_user_ids: Vec::new(),
            body: format!("message-{seq}"),
            seq,
            at: OffsetDateTime::now_utc(),
        }
    }

    #[test]
    fn mailbox_entries_have_stable_channel_event_ids() {
        let agent = message("request-7", "hello");
        assert_eq!(agent.stable_event_id(), "request-7");
        assert_eq!(room_chat("room-1", 42).stable_event_id(), "chat:room-1:42");

        let task = MailboxEntry::RoomTask {
            room_id: "room-1".to_owned(),
            room_name: None,
            task_id: "task-9".to_owned(),
            title: "Ship it".to_owned(),
            status: "Review".to_owned(),
            revision: 7,
            assigned_session_id: None,
            assigned_session_incarnation_id: None,
            delivery_reason: RoomTaskDeliveryReason::CurrentAssignmentOrUpdate,
            at: OffsetDateTime::from_unix_timestamp(123).expect("timestamp"),
        };
        let encoded = serde_json::to_vec(&task).expect("serialize task");
        let decoded: MailboxEntry = serde_json::from_slice(&encoded).expect("deserialize task");
        assert_eq!(
            task.stable_event_id(),
            "task:room-1:task-9:Review:123000000000"
        );
        assert_eq!(decoded.stable_event_id(), task.stable_event_id());
    }

    #[test]
    fn historical_room_task_mailbox_entries_decode_with_compatible_defaults_and_aliases() {
        let task = MailboxEntry::RoomTask {
            room_id: "room-1".to_owned(),
            room_name: Some("Engineering".to_owned()),
            task_id: "task-9".to_owned(),
            title: "Ship it".to_owned(),
            status: "Open".to_owned(),
            revision: 0,
            assigned_session_id: None,
            assigned_session_incarnation_id: None,
            delivery_reason: RoomTaskDeliveryReason::CurrentAssignmentOrUpdate,
            at: OffsetDateTime::from_unix_timestamp(123).expect("timestamp"),
        };
        let mut historical = serde_json::to_value(task).expect("serialize historical shape");
        let historical_object = historical.as_object_mut().expect("mailbox object");
        historical_object.remove("revision");
        historical_object.remove("assigned_session_id");
        historical_object.remove("assigned_session_incarnation_id");
        historical_object.remove("delivery_reason");
        let entry: MailboxEntry =
            serde_json::from_value(historical).expect("historical task should decode");
        std::assert_matches!(
            entry,
            MailboxEntry::RoomTask {
                revision: 0,
                assigned_session_id: None,
                assigned_session_incarnation_id: None,
                delivery_reason: RoomTaskDeliveryReason::CurrentAssignmentOrUpdate,
                ..
            }
        );

        let session_id = SessionId::new();
        let incarnation_id = Uuid::now_v7();
        let mut aliased = serde_json::to_value(MailboxEntry::RoomTask {
            room_id: "room-1".to_owned(),
            room_name: None,
            task_id: "task-9".to_owned(),
            title: "Ship it".to_owned(),
            status: "Open".to_owned(),
            revision: 11,
            assigned_session_id: Some(session_id),
            assigned_session_incarnation_id: Some(incarnation_id),
            delivery_reason: RoomTaskDeliveryReason::CurrentAssignmentOrUpdate,
            at: OffsetDateTime::from_unix_timestamp(123).expect("timestamp"),
        })
        .expect("serialize aliased shape");
        let aliased_object = aliased.as_object_mut().expect("mailbox object");
        for (snake, camel) in [
            ("revision", "taskRevision"),
            ("assigned_session_id", "assignedSessionId"),
            (
                "assigned_session_incarnation_id",
                "assignedSessionIncarnationId",
            ),
            ("delivery_reason", "deliveryReason"),
        ] {
            let value = aliased_object.remove(snake).expect("serialized field");
            aliased_object.insert(camel.to_owned(), value);
        }
        let aliased: MailboxEntry =
            serde_json::from_value(aliased).expect("camel-case aliases should decode");
        std::assert_matches!(
            aliased,
            MailboxEntry::RoomTask {
                revision: 11,
                assigned_session_id: Some(actual_session_id),
                assigned_session_incarnation_id: Some(actual_incarnation_id),
                delivery_reason: RoomTaskDeliveryReason::CurrentAssignmentOrUpdate,
                ..
            } if actual_session_id == session_id && actual_incarnation_id == incarnation_id
        );
    }

    #[test]
    fn old_room_chat_mailbox_entry_defaults_recipients_to_broadcast() {
        let mut old = serde_json::to_value(room_chat("room-1", 1)).expect("serialize mailbox");
        let object = old.as_object_mut().expect("mailbox should be an object");
        object.remove("recipient_session_ids");
        object.remove("recipient_user_ids");
        let entry: MailboxEntry =
            serde_json::from_value(old).expect("old mailbox entry should decode");

        std::assert_matches!(
            entry,
            MailboxEntry::RoomChat {
                recipient_session_ids,
                recipient_user_ids,
                ..
            } if recipient_session_ids.is_empty() && recipient_user_ids.is_empty()
        );
    }

    #[test]
    fn profiles_are_bounded_and_removable() {
        let (_dir, store) = temp_store();
        let session = SessionId::new();
        let oversized = AgentProfile {
            session_id: session,
            incarnation_id: None,
            owner_user_id: user(1),
            agent_kind: "Claude".to_owned(),
            cwd: None,
            description: "x".repeat(MAX_AGENT_DESCRIPTION_BYTES + 1),
            last_described_at: OffsetDateTime::now_utc(),
        };
        assert!(store.put_profile(&oversized).is_err());

        let valid = AgentProfile {
            description: "bounded".to_owned(),
            ..oversized
        };
        store.put_profile(&valid).expect("put valid profile");
        assert_eq!(store.list_profiles().expect("list").len(), 1);
        store.remove_profile(session).expect("remove profile");
        assert!(store.list_profiles().expect("list after remove").is_empty());
        store.remove_profile(session).expect("idempotent remove");
    }

    #[test]
    fn profile_round_trip() {
        let (_dir, store) = temp_store();
        let session = SessionId::new();
        let profile = AgentProfile {
            session_id: session,
            incarnation_id: Some(Uuid::now_v7()),
            owner_user_id: user(1),
            agent_kind: "Claude".to_owned(),
            cwd: None,
            description: "auth refactor".to_owned(),
            last_described_at: OffsetDateTime::now_utc(),
        };
        store.put_profile(&profile).expect("put");
        let listed = store.list_profiles().expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].description, "auth refactor");
        assert_eq!(listed[0].incarnation_id, profile.incarnation_id);
    }

    #[test]
    fn historical_profile_without_incarnation_remains_readable() {
        let profile = serde_json::json!({
            "session_id": SessionId::new(),
            "owner_user_id": user(1),
            "agent_kind": "Claude",
            "cwd": null,
            "description": "legacy",
            "last_described_at": OffsetDateTime::UNIX_EPOCH,
        });

        let decoded: AgentProfile =
            serde_json::from_value(profile).expect("historical profile should decode");

        assert!(decoded.incarnation_id.is_none());
    }

    #[test]
    fn visible_to_filters_by_owner() {
        let (_dir, store) = temp_store();
        let alice = user(1);
        let bob = user(2);
        store
            .put_profile(&AgentProfile {
                session_id: SessionId::new(),
                incarnation_id: None,
                owner_user_id: alice,
                agent_kind: "Claude".to_owned(),
                cwd: None,
                description: "a".to_owned(),
                last_described_at: OffsetDateTime::now_utc(),
            })
            .expect("alice");
        store
            .put_profile(&AgentProfile {
                session_id: SessionId::new(),
                incarnation_id: None,
                owner_user_id: bob,
                agent_kind: "Codex".to_owned(),
                cwd: None,
                description: "b".to_owned(),
                last_described_at: OffsetDateTime::now_utc(),
            })
            .expect("bob");
        let alice_view = store.visible_to(alice).expect("filter");
        assert_eq!(alice_view.len(), 1);
        assert_eq!(alice_view[0].owner_user_id, alice);
    }

    #[test]
    fn mailbox_drain_advances_cursor() {
        let (_dir, store) = temp_store();
        let target = SessionId::new();
        store
            .enqueue(
                target,
                &MailboxEntry::AgentMessage {
                    request_id: "req-1".to_owned(),
                    from_session_id: SessionId::new(),
                    from_description: Some("a".to_owned()),
                    body: "hi".to_owned(),
                    in_reply_to: None,
                    at: OffsetDateTime::now_utc(),
                },
            )
            .expect("enqueue");
        let (unread, cursor) = store.drain_unread(target).expect("drain");
        assert_eq!(unread.len(), 1);
        assert!(cursor > 1);
        store.set_cursor(target, cursor).expect("persist cursor");
        let (unread2, cursor2) = store.drain_unread(target).expect("drain second");
        assert_eq!(unread2.len(), 0);
        assert_eq!(cursor2, cursor);
    }

    #[test]
    fn mailbox_offer_commit_requires_exact_event_and_cursor() {
        let (_dir, store) = temp_store();
        let target = MailboxDestination::new(SessionId::new(), Uuid::now_v7());
        store
            .enqueue_for(target, &message("request-1", "first"))
            .expect("enqueue first");
        store
            .enqueue_for(target, &message("request-2", "second"))
            .expect("enqueue second");

        let offer = store
            .peek_unread_offer(target)
            .expect("peek")
            .expect("first offer");
        assert_eq!(offer.event_id, "request-1");

        let mut wrong_event = offer.clone();
        wrong_event.event_id = "request-other".to_owned();
        assert!(store.commit_offer(target, &wrong_event).is_err());
        let mut wrong_cursor = offer.clone();
        wrong_cursor.cursor = wrong_cursor.cursor.saturating_add(1);
        assert!(store.commit_offer(target, &wrong_cursor).is_err());
        assert_eq!(
            store
                .peek_unread_offer(target)
                .expect("peek after mismatch")
                .expect("offer remains unread")
                .event_id,
            "request-1"
        );

        store.commit_offer(target, &offer).expect("exact commit");
        assert_eq!(
            store
                .peek_unread_offer(target)
                .expect("peek second")
                .expect("second offer")
                .event_id,
            "request-2"
        );
    }

    #[test]
    fn offer_reservation_revalidates_without_holding_the_mailbox_lock() {
        let (_dir, store) = temp_store();
        let target = MailboxDestination::new(SessionId::new(), Uuid::now_v7());
        store
            .enqueue_for(target, &message("request-1", "first"))
            .expect("enqueue");
        let offer = store
            .peek_unread_offer(target)
            .expect("peek")
            .expect("offer");
        let reservation = store.reserve_offer(target, &offer).expect("reserve");

        let concurrent_store = store.clone();
        let (acquired_tx, acquired_rx) = std::sync::mpsc::channel();
        let lock_thread = std::thread::spawn(move || {
            let _guard = concurrent_store
                .lock_mailbox_read(target)
                .expect("concurrent lock");
            acquired_tx.send(()).expect("report lock");
        });
        let acquired = acquired_rx.recv_timeout(std::time::Duration::from_secs(1));
        lock_thread.join().expect("lock thread");
        acquired.expect("reservation must not retain the mailbox lock");

        store
            .set_cursor(target, offer.cursor)
            .expect("another reader can advance while the action runs");

        let error = reservation
            .commit()
            .expect_err("commit must compare against durable state again");
        assert_eq!(error.kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn replacement_incarnation_cannot_read_prior_incarnation_mailbox() {
        let (_dir, store) = temp_store();
        let session_id = SessionId::new();
        let prior = MailboxDestination::new(session_id, Uuid::now_v7());
        let replacement = MailboxDestination::new(session_id, Uuid::now_v7());
        store
            .enqueue_for(prior, &message("request-prior", "prior work"))
            .expect("enqueue for prior incarnation");

        assert!(
            store
                .peek_unread_offer(replacement)
                .expect("replacement mailbox read")
                .is_none(),
            "a replacement incarnation must not inherit its predecessor's mailbox"
        );
        assert_eq!(
            store
                .peek_unread_offer(prior)
                .expect("prior mailbox read")
                .expect("prior offer remains")
                .event_id,
            "request-prior"
        );
    }

    #[test]
    fn legacy_cursor_never_advances_an_incarnation_mailbox() {
        let (_dir, store) = temp_store();
        let session_id = SessionId::new();
        let destination = MailboxDestination::new(session_id, Uuid::now_v7());
        store
            .enqueue(session_id, &message("request-legacy", "legacy"))
            .expect("enqueue legacy");
        let (_, legacy_cursor) = store.drain_unread(session_id).expect("read legacy");
        store
            .set_cursor(session_id, legacy_cursor)
            .expect("commit legacy cursor");
        store
            .enqueue_for(destination, &message("request-current", "current"))
            .expect("enqueue current");

        let offer = store
            .peek_unread_offer(destination)
            .expect("peek current")
            .expect("current offer");

        assert_eq!(offer.event_id, "request-current");
        assert_eq!(offer.previous_cursor, 0);
    }

    #[test]
    fn stale_incarnation_cursor_cannot_acknowledge_replacement_mail() {
        let (_dir, store) = temp_store();
        let session_id = SessionId::new();
        let stale = MailboxDestination::new(session_id, Uuid::now_v7());
        let replacement = MailboxDestination::new(session_id, Uuid::now_v7());
        store
            .enqueue_for(replacement, &message("request-current", "current"))
            .expect("enqueue current");
        let replacement_offer = store
            .peek_unread_offer(replacement)
            .expect("peek replacement")
            .expect("replacement offer");

        store
            .set_cursor(stale, replacement_offer.cursor)
            .expect("commit stale cursor");

        assert_eq!(
            store
                .peek_unread_offer(replacement)
                .expect("replacement remains readable")
                .expect("replacement offer remains")
                .event_id,
            "request-current"
        );
    }

    #[test]
    fn room_delivery_dedupes_after_store_reopen() {
        let (dir, store) = temp_store();
        let target = MailboxDestination::new(SessionId::new(), Uuid::now_v7());
        let entry = room_chat("room-1", 42);
        assert!(
            store
                .enqueue_deduplicated(target, &entry)
                .expect("first room delivery")
        );
        drop(store);

        let reopened =
            AgentRoomStore::open_at(dir.path().to_path_buf()).expect("reopen room store");
        assert!(
            !reopened
                .enqueue_deduplicated(target, &entry)
                .expect("duplicate room delivery")
        );
        let (entries, _) = reopened
            .drain_unread_page(target, MAX_UNREAD_ENTRIES_PER_DRAIN)
            .expect("read room mailbox");
        assert_eq!(entries.len(), 1);
        std::assert_matches!(
            &entries[0],
            MailboxEntry::RoomChat {
                room_id,
                seq: 42,
                ..
            } if room_id == "room-1"
        );
    }

    #[test]
    fn room_delivery_cursors_persist_and_prune_departed_rooms() {
        let (dir, store) = temp_store();
        let account = user(1);
        let task_session = SessionId::new();
        let mut state = RoomDeliveryState::default();
        state.chat_cursors.insert("room-1".to_owned(), 7);
        state.chat_cursors.insert("departed".to_owned(), 99);
        state.task_cursors.insert(
            "room-1/task-1".to_owned(),
            RoomTaskDeliveryCursor {
                fingerprint: "v1".to_owned(),
                assigned_session_id: Some(task_session),
                assigned_session_incarnation_id: None,
                mailbox_destination: None,
            },
        );
        state.next_chat_room_id = Some("room-1".to_owned());
        state.next_task_room_id = Some("room-1".to_owned());
        state.chat_rooms_remaining = Some(3);
        state.task_rooms_remaining = Some(4);
        state.task_cursors.insert(
            "departed/task-2".to_owned(),
            RoomTaskDeliveryCursor {
                fingerprint: "v2".to_owned(),
                assigned_session_id: None,
                assigned_session_incarnation_id: None,
                mailbox_destination: None,
            },
        );
        state.retain_rooms(&BTreeSet::from(["room-1".to_owned()]));
        store
            .save_room_delivery_state(account, &state)
            .expect("save room delivery state");
        drop(store);

        let reopened =
            AgentRoomStore::open_at(dir.path().to_path_buf()).expect("reopen room store");
        let loaded = reopened
            .load_room_delivery_state(account)
            .expect("load room delivery state");
        assert_eq!(
            loaded.chat_cursors,
            BTreeMap::from([("room-1".to_owned(), 7)])
        );
        assert_eq!(loaded.task_cursors.len(), 1);
        assert_eq!(
            loaded
                .task_cursors
                .get("room-1/task-1")
                .and_then(|cursor| cursor.assigned_session_id),
            Some(task_session)
        );
        assert_eq!(loaded.next_chat_room_id.as_deref(), Some("room-1"));
        assert_eq!(loaded.next_task_room_id.as_deref(), Some("room-1"));
        assert_eq!(loaded.chat_rooms_remaining, Some(3));
        assert_eq!(loaded.task_rooms_remaining, Some(4));
    }

    #[test]
    fn room_delivery_state_migrates_version_one_markers() {
        let (_dir, store) = temp_store();
        let account = user(1);
        fs::write(
            store.room_delivery_state_path(account),
            br#"{"version":1,"chatCursors":{"room-1":7},"taskCursors":{}}"#,
        )
        .unwrap();

        let state = store.load_room_delivery_state(account).unwrap();

        assert_eq!(state.chat_cursors.get("room-1"), Some(&7));
        assert!(state.next_chat_room_id.is_none());
        assert!(state.next_task_room_id.is_none());
        assert!(state.chat_rooms_remaining.is_none());
        assert!(state.task_rooms_remaining.is_none());
        store.save_room_delivery_state(account, &state).unwrap();
        let persisted: serde_json::Value =
            serde_json::from_slice(&fs::read(store.room_delivery_state_path(account)).unwrap())
                .unwrap();
        assert_eq!(
            persisted.get("version").and_then(serde_json::Value::as_u64),
            Some(2)
        );
    }

    #[test]
    fn historical_task_cursor_without_incarnations_remains_readable_but_unbound() {
        let (_dir, store) = temp_store();
        let account = user(1);
        let assigned_session_id = SessionId::new();
        fs::write(
            store.room_delivery_state_path(account),
            serde_json::to_vec(&serde_json::json!({
                "version": 2,
                "chatCursors": {},
                "taskCursors": {
                    "room-1/task-1": {
                        "fingerprint": "legacy",
                        "assignedSessionId": assigned_session_id,
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let state = store.load_room_delivery_state(account).unwrap();
        let cursor = state
            .task_cursors
            .get("room-1/task-1")
            .expect("historical cursor");

        assert_eq!(cursor.assigned_session_id, Some(assigned_session_id));
        assert!(cursor.assigned_session_incarnation_id.is_none());
        assert!(cursor.mailbox_destination.is_none());
    }

    #[test]
    fn explicit_cursor_ignores_persisted_state_and_does_not_commit() {
        let (_dir, store) = temp_store();
        let target = SessionId::new();
        store
            .enqueue(target, &message("req-1", "first"))
            .expect("first enqueue");

        let (first, first_cursor) = store
            .read_after_cursor(target, 0)
            .expect("explicit first page");
        assert_eq!(first.len(), 1);
        let (still_unread, persisted_cursor) = store
            .drain_unread(target)
            .expect("persisted cursor unchanged");
        assert_eq!(still_unread.len(), 1);
        assert_eq!(persisted_cursor, first_cursor);

        store
            .set_cursor(target, first_cursor)
            .expect("commit first");
        store
            .enqueue(target, &message("req-2", "second"))
            .expect("second enqueue");
        let (_, second_cursor) = store.drain_unread(target).expect("second page");
        store
            .set_cursor(target, second_cursor)
            .expect("commit second");

        let (after_first, next_cursor) = store
            .read_after_cursor(target, first_cursor)
            .expect("explicit cursor must ignore later persisted cursor");
        assert_eq!(after_first.len(), 1);
        std::assert_matches!(
            &after_first[0],
            MailboxEntry::AgentMessage {
                request_id,
                body,
                ..
            } if request_id == "req-2" && body == "second"
        );
        assert_eq!(next_cursor, second_cursor);
    }

    #[test]
    fn torn_tail_is_not_consumed_before_newline_arrives() {
        use std::io::Write;
        let (_dir, store) = temp_store();
        let target = SessionId::new();
        let entry = MailboxEntry::AgentMessage {
            request_id: "req-torn".to_owned(),
            from_session_id: SessionId::new(),
            from_description: None,
            body: "hello".to_owned(),
            in_reply_to: None,
            at: OffsetDateTime::now_utc(),
        };
        let encoded = serde_json::to_vec(&entry).unwrap();
        let split = encoded.len() / 2;
        let path = store.root.join("mailbox").join(mailbox_filename(target));
        fs::write(&path, &encoded[..split]).unwrap();
        let (first, cursor) = store.drain_unread(target).unwrap();
        assert!(first.is_empty());
        assert_eq!(cursor, 0);

        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(&encoded[split..]).unwrap();
        file.write_all(b"\n").unwrap();
        let (second, cursor) = store.drain_unread(target).unwrap();
        assert_eq!(second.len(), 1);
        std::assert_matches!(
            &second[0],
            MailboxEntry::AgentMessage { request_id, body, .. }
                if request_id == "req-torn" && body == "hello"
        );
        assert_eq!(cursor, u64::try_from(encoded.len() + 1).unwrap());
    }

    #[test]
    fn complete_legacy_oversized_line_aborts_at_record_bound() {
        let (_dir, store) = temp_store();
        let target = SessionId::new();
        let mut mailbox = vec![b'x'; MAX_MAILBOX_ENTRY_BYTES + 1024];
        mailbox.push(b'\n');
        let mut valid = serde_json::to_vec(&message("after-oversized", "delivered")).unwrap();
        valid.push(b'\n');
        mailbox.extend_from_slice(&valid);
        let path = store.root.join("mailbox").join(mailbox_filename(target));
        fs::write(path, &mailbox).unwrap();

        let error = store
            .drain_unread(target)
            .expect_err("oversized record must abort before scanning to its newline");

        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn torn_legacy_oversized_line_aborts_before_eof() {
        let (_dir, store) = temp_store();
        let target = SessionId::new();
        let mailbox = vec![b'x'; usize::try_from(MAX_MAILBOX_BYTES).unwrap()];
        let path = store.root.join("mailbox").join(mailbox_filename(target));
        fs::write(path, mailbox).unwrap();

        let error = store
            .drain_unread(target)
            .expect_err("unterminated oversized record must fail bounded");

        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert!(!store.cursor_path(target).exists());
    }

    #[test]
    fn scanner_consumes_no_more_than_its_remaining_byte_cap() {
        let source = vec![b'x'; MAX_MAILBOX_RECORD_BYTES * 4];
        let cursor = std::io::Cursor::new(source);
        let mut reader = BufReader::with_capacity(1024, cursor);

        let error = scan_mailbox_line(&mut reader, MAX_MAILBOX_RECORD_BYTES)
            .expect_err("unterminated record must stop at the supplied cap");

        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert_eq!(
            reader.stream_position().expect("logical reader position"),
            u64::try_from(MAX_MAILBOX_RECORD_BYTES).unwrap()
        );
    }

    #[test]
    fn plain_line_cursor_migrates_once_to_atomic_byte_cursor() {
        let (_dir, store) = temp_store();
        let target = SessionId::new();
        let path = store.root.join("mailbox").join(mailbox_filename(target));
        let mut first = serde_json::to_vec(&message("one", "consumed")).unwrap();
        first.push(b'\n');
        let mut second = serde_json::to_vec(&message("two", "queued")).unwrap();
        second.push(b'\n');
        let mut mailbox = first.clone();
        mailbox.extend_from_slice(&second);
        fs::write(path, mailbox).unwrap();
        fs::write(store.cursor_path(target), b"1").unwrap();

        let (unread, _) = store.drain_unread(target).expect("legacy cursor migrates");
        assert_eq!(unread.len(), 1);
        std::assert_matches!(
            &unread[0],
            MailboxEntry::AgentMessage { request_id, .. } if request_id == "two"
        );
        assert_eq!(
            fs::read_to_string(store.cursor_path(target)).unwrap(),
            format!("bytes:{}", first.len())
        );
    }

    #[test]
    fn oversized_legacy_cursor_is_bounded_and_not_rewritten() {
        let (_dir, store) = temp_store();
        let target = SessionId::new();
        store
            .enqueue(target, &message("one", "queued"))
            .expect("enqueue");
        let legacy = (MAX_LEGACY_CURSOR_MIGRATION_LINES + 1).to_string();
        fs::write(store.cursor_path(target), &legacy).unwrap();

        let error = store
            .drain_unread(target)
            .expect_err("oversized legacy cursor must fail bounded");
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert_eq!(
            fs::read_to_string(store.cursor_path(target)).unwrap(),
            legacy
        );
    }

    #[test]
    fn torn_legacy_cursor_is_not_rewritten_and_mail_remains_queued() {
        let (_dir, store) = temp_store();
        let target = SessionId::new();
        store
            .enqueue(target, &message("one", "queued"))
            .expect("enqueue");
        fs::write(store.cursor_path(target), b"1x").unwrap();

        let error = store
            .drain_unread(target)
            .expect_err("torn cursor must fail");
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert_eq!(fs::read(store.cursor_path(target)).unwrap(), b"1x");

        fs::write(store.cursor_path(target), b"0").unwrap();
        let (unread, _) = store.drain_unread(target).expect("retry migration");
        assert_eq!(
            unread.len(),
            1,
            "failed migration must not lose queued mail"
        );
        assert!(
            fs::read_to_string(store.cursor_path(target))
                .unwrap()
                .starts_with("bytes:")
        );
    }

    #[test]
    fn enqueue_repairs_abandoned_torn_tail_before_appending() {
        use std::io::Write;

        let (_dir, store) = temp_store();
        let target = SessionId::new();
        store
            .enqueue(target, &message("req-1", "complete"))
            .expect("complete enqueue");
        let (_, complete_cursor) = store.read_after_cursor(target, 0).expect("first page");

        let torn = serde_json::to_vec(&message("req-torn", "abandoned")).unwrap();
        let path = store.root.join("mailbox").join(mailbox_filename(target));
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(&torn[..torn.len() / 2]).unwrap();
        drop(file);

        store
            .enqueue(target, &message("req-2", "after repair"))
            .expect("enqueue should discard abandoned partial record");
        let (entries, _) = store
            .read_after_cursor(target, complete_cursor)
            .expect("page after repaired tail");
        assert_eq!(entries.len(), 1);
        std::assert_matches!(
            &entries[0],
            MailboxEntry::AgentMessage {
                request_id,
                body,
                ..
            } if request_id == "req-2" && body == "after repair"
        );
    }

    #[test]
    fn retention_compaction_keeps_cursors_monotonic() {
        let (_dir, store) = temp_store();
        let target = SessionId::new();
        let mut line = serde_json::to_vec(&message("old", "consumed")).unwrap();
        line.push(b'\n');
        let retention_bytes = usize::try_from(MAX_MAILBOX_BYTES).expect("retention fits usize");
        let mut consumed = Vec::with_capacity(retention_bytes.saturating_add(line.len()));
        while consumed.len() <= retention_bytes {
            consumed.extend_from_slice(&line);
        }
        let consumed_cursor = u64::try_from(consumed.len()).unwrap();
        let path = store.root.join("mailbox").join(mailbox_filename(target));
        fs::write(&path, consumed).unwrap();
        store.set_cursor(target, consumed_cursor).unwrap();

        store
            .enqueue(target, &message("new", "retained"))
            .expect("enqueue should compact consumed history");

        let (entries, next_cursor) = store
            .read_after_cursor(target, consumed_cursor)
            .expect("pre-compaction cursor remains valid");
        assert_eq!(entries.len(), 1);
        assert!(next_cursor > consumed_cursor);
        std::assert_matches!(
            &entries[0],
            MailboxEntry::AgentMessage {
                request_id,
                body,
                ..
            } if request_id == "new" && body == "retained"
        );
        let expired = store
            .read_after_cursor(target, 0)
            .expect_err("cursor for pruned history must fail rather than misread");
        assert_eq!(expired.kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn absolute_mailbox_cursor_survives_multiple_compaction_generations() {
        use std::io::Write;

        let (_dir, store) = temp_store();
        let target = SessionId::new();
        let mut filler_line = serde_json::to_vec(&message("filler", "consumed")).unwrap();
        filler_line.push(b'\n');
        let retention_bytes = usize::try_from(MAX_MAILBOX_BYTES).expect("retention fits usize");
        let path = store.root.join("mailbox").join(mailbox_filename(target));

        let mut first_generation = Vec::with_capacity(retention_bytes + filler_line.len());
        while first_generation.len() <= retention_bytes {
            first_generation.extend_from_slice(&filler_line);
        }
        let first_cursor = u64::try_from(first_generation.len()).unwrap();
        fs::write(&path, first_generation).unwrap();
        store.set_cursor(target, first_cursor).unwrap();
        store
            .enqueue(target, &message("generation-1", "retained"))
            .expect("first compaction");
        let (first_entries, first_end) = store
            .read_after_cursor(target, first_cursor)
            .expect("first generation cursor");
        assert_eq!(first_entries.len(), 1);
        store.set_cursor(target, first_end).unwrap();

        let current_len = usize::try_from(fs::metadata(&path).unwrap().len()).unwrap();
        let mut second_generation =
            Vec::with_capacity(retention_bytes.saturating_sub(current_len) + filler_line.len());
        while current_len + second_generation.len() <= retention_bytes {
            second_generation.extend_from_slice(&filler_line);
        }
        let appended = u64::try_from(second_generation.len()).unwrap();
        let second_cursor = first_end.saturating_add(appended);
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(&second_generation).unwrap();
        drop(file);
        store.set_cursor(target, second_cursor).unwrap();
        store
            .enqueue(target, &message("generation-2", "retained"))
            .expect("second compaction");

        let (second_entries, second_end) = store
            .read_after_cursor(target, second_cursor)
            .expect("second generation cursor");
        assert_eq!(second_entries.len(), 1);
        assert!(second_end > second_cursor);
        std::assert_matches!(
            &second_entries[0],
            MailboxEntry::AgentMessage { request_id, .. } if request_id == "generation-2"
        );
        assert_eq!(
            store
                .read_after_cursor(target, first_end)
                .expect_err("first-generation cursor must expire after second compaction")
                .kind(),
            ErrorKind::InvalidInput
        );
    }

    #[test]
    fn compaction_migrates_plain_line_cursor_before_rewrite() {
        let (_dir, store) = temp_store();
        let target = SessionId::new();
        let mut first = serde_json::to_vec(&message("old", "consumed")).unwrap();
        first.push(b'\n');
        let mut second = serde_json::to_vec(&message("new", "retained")).unwrap();
        second.push(b'\n');
        let path = store.root.join("mailbox").join(mailbox_filename(target));
        let mut mailbox = first;
        mailbox.extend_from_slice(&second);
        let mailbox_len = mailbox.len();
        fs::write(&path, mailbox).unwrap();
        fs::write(store.cursor_path(target), b"1").unwrap();

        let incoming = usize::try_from(MAX_MAILBOX_BYTES)
            .unwrap()
            .saturating_sub(mailbox_len)
            .saturating_add(1);
        store
            .compact_mailbox_if_needed(target.into(), incoming)
            .expect("plain-line cursor migrates before compaction");
        assert!(
            fs::read_to_string(store.cursor_path(target))
                .unwrap()
                .starts_with("bytes:")
        );
    }

    #[cfg(unix)]
    #[test]
    fn agent_room_state_is_owner_private() {
        use std::os::unix::fs::PermissionsExt;

        let (dir, store) = temp_store();
        let session = SessionId::new();
        let destination = MailboxDestination::new(session, Uuid::now_v7());
        store
            .put_profile(&AgentProfile {
                session_id: session,
                incarnation_id: Some(destination.incarnation_id),
                owner_user_id: user(1),
                agent_kind: "Claude".to_owned(),
                cwd: None,
                description: "private".to_owned(),
                last_described_at: OffsetDateTime::now_utc(),
            })
            .unwrap();
        store
            .enqueue_for(
                destination,
                &MailboxEntry::AgentMessage {
                    request_id: "private".to_owned(),
                    from_session_id: SessionId::new(),
                    from_description: None,
                    body: "secret".to_owned(),
                    in_reply_to: None,
                    at: OffsetDateTime::now_utc(),
                },
            )
            .unwrap();
        store.set_cursor(destination, 0).unwrap();
        for path in [
            dir.path().join("profiles").join(profile_filename(session)),
            dir.path()
                .join("mailbox")
                .join(mailbox_filename(destination)),
            store.cursor_path(destination),
        ] {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        for path in [
            dir.path().to_path_buf(),
            dir.path().join("profiles"),
            dir.path().join("mailbox"),
        ] {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
    }
}

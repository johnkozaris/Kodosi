use std::{collections::HashMap, fmt};

use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use kodosi_backend_client::control::{ControlTrustEntry, ControlTrustStore};
use kodosi_backend_client::crypto;
use kodosi_domain::{
    ids::SessionId,
    permissions::{AccessLevel, ShareScope},
};

pub(crate) const FRAME_NONCE_BLOCK_SIZE: u64 = 1 << 40;
pub(crate) const FRAME_REVISION_BLOCK_SIZE: u64 = 1 << 40;

pub(crate) const MAX_FRAME_REVISION_EXCLUSIVE: u64 = i64::MAX as u64 + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameNonceBlock {
    pub(crate) generation: u32,
    pub(crate) start: u64,
    pub(crate) end_exclusive: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameNonceReservationError {
    MissingSession,
    MissingKeyContext,
    Exhausted,
}

impl fmt::Display for FrameNonceReservationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSession => f.write_str("shared session is missing"),
            Self::MissingKeyContext => {
                f.write_str("shared session is missing encrypted key context")
            }
            Self::Exhausted => f.write_str("encrypted frame nonce space exhausted"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameRevisionBlock {
    pub(crate) start: u64,
    pub(crate) end_exclusive: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameRevisionReservationError {
    MissingSession,
    Exhausted,
}

impl fmt::Display for FrameRevisionReservationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSession => f.write_str("shared session is missing"),
            Self::Exhausted => f.write_str("terminal frame revision space exhausted"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SharedRoom {
    pub(crate) id: String,
    pub(crate) name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExplicitGrant {
    pub(crate) access: AccessLevel,
    pub(crate) expires_at: time::OffsetDateTime,
}

pub(crate) struct SharedSessionState {
    backend_session_id: String,
    backend_incarnation_id: Uuid,

    scope: ShareScope,
    room: Option<SharedRoom>,
    owner_secret: Zeroizing<String>,
    session_key: Option<crypto::SessionKey>,
    session_key_generation: Option<u32>,
    next_frame_nonce_counter: u64,
    next_frame_revision: u64,
    control_trust: ControlTrustStore,
    host_device_id: Option<String>,

    explicit_grantee_access: HashMap<String, ExplicitGrant>,
}

impl SharedSessionState {
    pub(crate) fn new(
        backend_session_id: String,
        backend_incarnation_id: Uuid,
        owner_secret: impl Into<Zeroizing<String>>,
        scope: ShareScope,
        room: Option<SharedRoom>,
        session_key: Option<crypto::SessionKey>,
        session_key_generation: Option<u32>,
    ) -> Self {
        Self {
            backend_session_id,
            backend_incarnation_id,
            scope,
            room,
            owner_secret: owner_secret.into(),
            session_key,
            session_key_generation,
            next_frame_nonce_counter: 0,
            next_frame_revision: 1,
            control_trust: ControlTrustStore::default(),
            host_device_id: None,
            explicit_grantee_access: HashMap::new(),
        }
    }

    pub(crate) fn backend_session_id(&self) -> &str {
        &self.backend_session_id
    }

    pub(crate) fn backend_incarnation_id(&self) -> &Uuid {
        &self.backend_incarnation_id
    }

    pub(crate) fn owner_secret(&self) -> &str {
        self.owner_secret.as_str()
    }

    pub(crate) fn room(&self) -> Option<&SharedRoom> {
        self.room.as_ref()
    }

    pub(crate) fn scope(&self) -> ShareScope {
        self.scope
    }

    pub(crate) fn set_audience(&mut self, scope: ShareScope, room: Option<SharedRoom>) {
        self.scope = scope;
        self.room = room;
    }

    pub(crate) fn session_key(&self) -> Option<&crypto::SessionKey> {
        self.session_key.as_ref()
    }

    pub(crate) fn session_key_generation(&self) -> Option<u32> {
        self.session_key_generation
    }

    pub(crate) fn control_trust(&self) -> ControlTrustStore {
        self.control_trust.clone()
    }

    pub(crate) fn host_device_id(&self) -> Option<&str> {
        self.host_device_id.as_deref()
    }

    pub(crate) fn set_session_key(
        &mut self,
        key: Option<crypto::SessionKey>,
        generation: Option<u32>,
    ) {
        if self.session_key == key && self.session_key_generation == generation {
            return;
        }

        if let Some(old_key) = self.session_key.as_mut() {
            old_key.zeroize();
        }
        self.session_key = key;
        self.session_key_generation = generation;
        self.next_frame_nonce_counter = 0;
        self.control_trust.reset_replay();
    }

    pub(crate) fn grant_user_at(
        &mut self,
        user_id: String,
        access: AccessLevel,
        expires_at: time::OffsetDateTime,
    ) {
        self.explicit_grantee_access
            .insert(user_id, ExplicitGrant { access, expires_at });
    }

    pub(crate) fn revoke_user(&mut self, user_id: &str) {
        self.explicit_grantee_access.remove(user_id);
    }

    pub(crate) fn explicit_grantee_access(&self) -> &HashMap<String, ExplicitGrant> {
        &self.explicit_grantee_access
    }

    fn reserve_frame_nonce_block(
        &mut self,
        block_size: u64,
    ) -> Result<FrameNonceBlock, FrameNonceReservationError> {
        if block_size == 0 {
            return Err(FrameNonceReservationError::Exhausted);
        }
        if self.session_key.is_none() {
            return Err(FrameNonceReservationError::MissingKeyContext);
        }
        let generation = self
            .session_key_generation
            .ok_or(FrameNonceReservationError::MissingKeyContext)?;
        let start = self.next_frame_nonce_counter;
        let end_exclusive = start
            .checked_add(block_size)
            .ok_or(FrameNonceReservationError::Exhausted)?;
        self.next_frame_nonce_counter = end_exclusive;
        Ok(FrameNonceBlock {
            generation,
            start,
            end_exclusive,
        })
    }

    fn reserve_frame_revision_block(
        &mut self,
        block_size: u64,
    ) -> Result<FrameRevisionBlock, FrameRevisionReservationError> {
        if block_size == 0 {
            return Err(FrameRevisionReservationError::Exhausted);
        }
        let start = self.next_frame_revision;
        let end_exclusive = start
            .checked_add(block_size)
            .ok_or(FrameRevisionReservationError::Exhausted)?;
        if end_exclusive > MAX_FRAME_REVISION_EXCLUSIVE {
            return Err(FrameRevisionReservationError::Exhausted);
        }
        self.next_frame_revision = end_exclusive;
        Ok(FrameRevisionBlock {
            start,
            end_exclusive,
        })
    }
}

impl fmt::Debug for SharedSessionState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SharedSessionState")
            .field("backend_session_id", &self.backend_session_id)
            .field("backend_incarnation_id", &self.backend_incarnation_id)
            .field("scope", &self.scope)
            .field("room", &self.room)
            .field("owner_secret", &"<redacted>")
            .field(
                "session_key",
                &self.session_key.as_ref().map(|_| "<redacted>"),
            )
            .field("session_key_generation", &self.session_key_generation)
            .field("next_frame_nonce_counter", &self.next_frame_nonce_counter)
            .field("next_frame_revision", &self.next_frame_revision)
            .field("control_trust", &self.control_trust)
            .field("host_device_id", &self.host_device_id)
            .field("explicit_grantee_access", &self.explicit_grantee_access)
            .finish()
    }
}

impl Drop for SharedSessionState {
    fn drop(&mut self) {
        if let Some(key) = self.session_key.as_mut() {
            key.zeroize();
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct SharedSessionRegistry {
    entries: HashMap<SessionId, SharedSessionState>,
}

impl SharedSessionRegistry {
    pub(crate) fn insert(&mut self, id: SessionId, state: SharedSessionState) {
        debug_assert!(
            !self.entries.contains_key(&id),
            "replacing existing shared session state for {id}"
        );
        self.entries.insert(id, state);
    }

    pub(crate) fn get(&self, id: SessionId) -> Option<&SharedSessionState> {
        self.entries.get(&id)
    }

    pub(crate) fn get_mut(&mut self, id: SessionId) -> Option<&mut SharedSessionState> {
        self.entries.get_mut(&id)
    }

    pub(crate) fn contains(&self, id: SessionId) -> bool {
        self.entries.contains_key(&id)
    }

    pub(crate) fn local_id_for_backend(&self, backend_session_id: &str) -> Option<SessionId> {
        self.entries
            .iter()
            .find(|(_, state)| state.backend_session_id() == backend_session_id)
            .map(|(&id, _)| id)
    }

    pub(crate) fn contains_exact_backend_incarnation(
        &self,
        backend_session_id: &str,
        incarnation_id: Uuid,
    ) -> bool {
        self.entries.values().any(|state| {
            state.backend_session_id() == backend_session_id
                && *state.backend_incarnation_id() == incarnation_id
        })
    }

    pub(crate) fn replace_control_trust_for_backend(
        &self,
        backend_session_id: &str,
        keys: HashMap<(String, String), ControlTrustEntry>,
    ) -> bool {
        let Some(state) = self
            .entries
            .values()
            .find(|state| state.backend_session_id() == backend_session_id)
        else {
            return false;
        };
        state.control_trust.replace(keys);
        true
    }

    pub(crate) fn set_host_device_for_backend(
        &mut self,
        backend_session_id: &str,
        device_id: String,
    ) -> bool {
        let Some(state) = self
            .entries
            .values_mut()
            .find(|state| state.backend_session_id() == backend_session_id)
        else {
            return false;
        };
        state.host_device_id = Some(device_id);
        true
    }

    pub(crate) fn ids(&self) -> impl Iterator<Item = SessionId> + '_ {
        self.entries.keys().copied()
    }

    pub(crate) fn clear(&mut self, id: SessionId) -> bool {
        self.entries.remove(&id).is_some()
    }

    pub(crate) fn set_session_key(
        &mut self,
        id: SessionId,
        key: Option<crypto::SessionKey>,
        generation: Option<u32>,
    ) -> bool {
        let Some(state) = self.entries.get_mut(&id) else {
            return false;
        };
        state.set_session_key(key, generation);
        true
    }

    pub(crate) fn set_audience(
        &mut self,
        id: SessionId,
        scope: ShareScope,
        room: Option<SharedRoom>,
    ) -> bool {
        let Some(state) = self.entries.get_mut(&id) else {
            return false;
        };
        state.set_audience(scope, room);
        true
    }

    pub(crate) fn reserve_frame_nonce_block(
        &mut self,
        id: SessionId,
        block_size: u64,
    ) -> Result<FrameNonceBlock, FrameNonceReservationError> {
        self.entries
            .get_mut(&id)
            .ok_or(FrameNonceReservationError::MissingSession)?
            .reserve_frame_nonce_block(block_size)
    }

    pub(crate) fn reserve_frame_revision_block(
        &mut self,
        id: SessionId,
        block_size: u64,
    ) -> Result<FrameRevisionBlock, FrameRevisionReservationError> {
        self.entries
            .get_mut(&id)
            .ok_or(FrameRevisionReservationError::MissingSession)?
            .reserve_frame_revision_block(block_size)
    }

    pub(crate) fn room_for_scope(&self, id: SessionId, scope: ShareScope) -> Option<SharedRoom> {
        (scope == ShareScope::Room)
            .then(|| self.entries.get(&id)?.room.clone())
            .flatten()
    }

    pub(crate) fn ids_scoped_to_room(&self, room_id: &str) -> Vec<SessionId> {
        self.entries
            .iter()
            .filter(|(_, state)| {
                state.scope == ShareScope::Room
                    && state.room.as_ref().is_some_and(|room| room.id == room_id)
            })
            .map(|(id, _)| *id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FrameNonceReservationError, FrameRevisionReservationError, SharedRoom,
        SharedSessionRegistry, SharedSessionState,
    };
    use kodosi_domain::ids::SessionId;

    fn assert_zeroizes_on_drop<T: zeroize::ZeroizeOnDrop>(_: &T) {}

    fn shared_state(key: Option<[u8; 32]>) -> SharedSessionState {
        SharedSessionState::new(
            "backend-1".to_owned(),
            uuid::Uuid::from_u128(1),
            "owner-secret".to_owned(),
            kodosi_domain::permissions::ShareScope::Room,
            Some(SharedRoom {
                id: "room-1".to_owned(),
                name: "Acme".to_owned(),
            }),
            key,
            key.map(|_| 1),
        )
    }

    fn shared_state_for(
        room_id: &str,
        scope: kodosi_domain::permissions::ShareScope,
    ) -> SharedSessionState {
        SharedSessionState::new(
            "backend-1".to_owned(),
            uuid::Uuid::from_u128(1),
            "owner-secret".to_owned(),
            scope,
            Some(SharedRoom {
                id: room_id.to_owned(),
                name: "Acme".to_owned(),
            }),
            Some([9; 32]),
            Some(1),
        )
    }

    #[test]
    fn ids_scoped_to_room_selects_only_that_rooms_room_scoped_sessions() {
        use kodosi_domain::permissions::ShareScope;
        let mut registry = SharedSessionRegistry::default();

        let in_room_a = SessionId::new();
        let also_in_room_a = SessionId::new();
        let in_room_b = SessionId::new();
        let just_me_in_room_a = SessionId::new();

        registry.insert(in_room_a, shared_state_for("room-a", ShareScope::Room));
        registry.insert(also_in_room_a, shared_state_for("room-a", ShareScope::Room));
        registry.insert(in_room_b, shared_state_for("room-b", ShareScope::Room));
        registry.insert(
            just_me_in_room_a,
            shared_state_for("room-a", ShareScope::JustMe),
        );

        let mut affected = registry.ids_scoped_to_room("room-a");
        affected.sort();
        let mut expected = vec![in_room_a, also_in_room_a];
        expected.sort();

        assert_eq!(affected, expected);
        assert!(registry.ids_scoped_to_room("room-unknown").is_empty());
    }

    #[test]
    fn insert_get_and_clear_shared_state() {
        let id = SessionId::new();
        let mut registry = SharedSessionRegistry::default();

        registry.insert(id, shared_state(Some([7; 32])));

        let state = registry
            .get(id)
            .unwrap_or_else(|| panic!("shared session should be present"));
        assert_eq!(state.backend_session_id(), "backend-1");
        assert_eq!(state.owner_secret(), "owner-secret");
        assert_zeroizes_on_drop(&state.owner_secret);
        let debug = format!("{state:?}");
        assert!(!debug.contains("owner-secret"));
        assert!(!debug.contains("[7, 7, 7"));
        assert!(debug.contains("<redacted>"));
        assert_eq!(state.room().map(|room| room.id.as_str()), Some("room-1"));
        assert!(registry.ids().next().is_some());

        assert!(registry.clear(id));
        assert!(registry.ids().next().is_none());
        assert!(!registry.clear(id));
    }

    #[test]
    fn key_replacement_updates_key_and_generation_together() {
        let id = SessionId::new();
        let mut registry = SharedSessionRegistry::default();
        registry.insert(id, shared_state(Some([1; 32])));

        assert!(registry.set_session_key(id, Some([2; 32]), Some(2)));

        let state = registry
            .get(id)
            .unwrap_or_else(|| panic!("shared session should be present"));
        assert_eq!(state.session_key(), Some(&[2; 32]));
        assert_eq!(state.session_key_generation(), Some(2));

        assert!(registry.set_session_key(id, None, None));
        let state = registry
            .get(id)
            .unwrap_or_else(|| panic!("shared session should remain present"));
        assert_eq!(state.session_key(), None);
        assert_eq!(state.session_key_generation(), None);
    }

    #[test]
    fn reserve_frame_nonce_blocks_are_contiguous_and_non_overlapping() {
        let id = SessionId::new();
        let mut registry = SharedSessionRegistry::default();
        registry.insert(id, shared_state(Some([1; 32])));

        let first = registry.reserve_frame_nonce_block(id, 4).unwrap();
        let second = registry.reserve_frame_nonce_block(id, 4).unwrap();
        let third = registry.reserve_frame_nonce_block(id, 4).unwrap();

        assert_eq!(first.generation, 1);
        assert_eq!((first.start, first.end_exclusive), (0, 4));
        assert_eq!((second.start, second.end_exclusive), (4, 8));
        assert_eq!((third.start, third.end_exclusive), (8, 12));
    }

    #[test]
    fn setting_same_key_context_preserves_nonce_allocator() {
        let id = SessionId::new();
        let mut registry = SharedSessionRegistry::default();
        registry.insert(id, shared_state(Some([1; 32])));

        assert_eq!(
            registry.reserve_frame_nonce_block(id, 4).unwrap(),
            super::FrameNonceBlock {
                generation: 1,
                start: 0,
                end_exclusive: 4,
            }
        );
        assert!(registry.set_session_key(id, Some([1; 32]), Some(1)));

        assert_eq!(
            registry.reserve_frame_nonce_block(id, 4).unwrap(),
            super::FrameNonceBlock {
                generation: 1,
                start: 4,
                end_exclusive: 8,
            }
        );
    }

    #[test]
    fn changing_key_context_resets_nonce_allocator() {
        let id = SessionId::new();
        let mut registry = SharedSessionRegistry::default();
        registry.insert(id, shared_state(Some([1; 32])));

        assert_eq!(registry.reserve_frame_nonce_block(id, 4).unwrap().start, 0);
        assert!(registry.set_session_key(id, Some([2; 32]), Some(2)));

        let block = registry.reserve_frame_nonce_block(id, 4).unwrap();
        assert_eq!(block.generation, 2);
        assert_eq!((block.start, block.end_exclusive), (0, 4));
    }

    #[test]
    fn sessions_without_key_do_not_reserve_frame_nonce_blocks() {
        let id = SessionId::new();
        let mut registry = SharedSessionRegistry::default();
        registry.insert(id, shared_state(None));

        assert_eq!(
            registry.reserve_frame_nonce_block(id, 4),
            Err(FrameNonceReservationError::MissingKeyContext)
        );
        assert!(registry.set_session_key(id, Some([3; 32]), Some(3)));

        let block = registry.reserve_frame_nonce_block(id, 4).unwrap();
        assert_eq!((block.start, block.end_exclusive), (0, 4));
    }

    #[test]
    fn reserve_frame_nonce_block_fails_closed_on_overflow() {
        let id = SessionId::new();
        let mut registry = SharedSessionRegistry::default();
        registry.insert(id, shared_state(Some([1; 32])));

        assert_eq!(
            registry.reserve_frame_nonce_block(id, 0),
            Err(FrameNonceReservationError::Exhausted)
        );
        assert!(registry.reserve_frame_nonce_block(id, u64::MAX - 1).is_ok());
        assert_eq!(
            registry.reserve_frame_nonce_block(id, 4),
            Err(FrameNonceReservationError::Exhausted)
        );
    }

    #[test]
    fn reserve_frame_revision_blocks_start_at_one_and_do_not_overlap() {
        let id = SessionId::new();
        let mut registry = SharedSessionRegistry::default();
        registry.insert(id, shared_state(None));

        let first = registry.reserve_frame_revision_block(id, 4).unwrap();
        let second = registry.reserve_frame_revision_block(id, 4).unwrap();

        assert_eq!((first.start, first.end_exclusive), (1, 5));
        assert_eq!((second.start, second.end_exclusive), (5, 9));
    }

    #[test]
    fn sessions_without_key_reserve_frame_revision_blocks() {
        let id = SessionId::new();
        let mut registry = SharedSessionRegistry::default();
        registry.insert(id, shared_state(None));

        let block = registry.reserve_frame_revision_block(id, 2).unwrap();

        assert_eq!((block.start, block.end_exclusive), (1, 3));
    }

    #[test]
    fn key_rotation_does_not_reset_frame_revision_allocator() {
        let id = SessionId::new();
        let mut registry = SharedSessionRegistry::default();
        registry.insert(id, shared_state(Some([1; 32])));

        assert_eq!(
            registry.reserve_frame_revision_block(id, 4).unwrap(),
            super::FrameRevisionBlock {
                start: 1,
                end_exclusive: 5,
            }
        );
        assert!(registry.set_session_key(id, Some([2; 32]), Some(2)));

        assert_eq!(
            registry.reserve_frame_revision_block(id, 4).unwrap(),
            super::FrameRevisionBlock {
                start: 5,
                end_exclusive: 9,
            }
        );
    }

    #[test]
    fn reserve_frame_revision_block_fails_closed_on_overflow() {
        let id = SessionId::new();
        let mut registry = SharedSessionRegistry::default();
        registry.insert(id, shared_state(None));

        assert_eq!(
            registry.reserve_frame_revision_block(id, 0),
            Err(FrameRevisionReservationError::Exhausted)
        );
        assert!(
            registry
                .reserve_frame_revision_block(id, super::MAX_FRAME_REVISION_EXCLUSIVE - 1)
                .is_ok()
        );
        assert_eq!(
            registry.reserve_frame_revision_block(id, 1),
            Err(FrameRevisionReservationError::Exhausted)
        );
    }

    #[test]
    fn room_for_scope_only_projects_room_sessions() {
        let id = SessionId::new();
        let mut registry = SharedSessionRegistry::default();
        registry.insert(id, shared_state(None));

        assert!(
            registry
                .room_for_scope(id, kodosi_domain::permissions::ShareScope::JustMe)
                .is_none()
        );
        assert_eq!(
            registry
                .room_for_scope(id, kodosi_domain::permissions::ShareScope::Room)
                .map(|room| room.name),
            Some("Acme".to_owned())
        );
    }
}

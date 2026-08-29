#![allow(clippy::trivially_copy_pass_by_ref, clippy::ref_option)]

use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

use kodosi_domain::{
    permissions::{AccessLevel, DefaultAudienceAccess, ShareScope},
    session::SessionState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendToolKind {
    Generic,
    Terminal,
    ClaudeCode,
}

pub const fn share_scope_label(scope: ShareScope) -> &'static str {
    match scope {
        ShareScope::JustMe => "JustMe",
        ShareScope::MyDevices => "MyDevices",
        ShareScope::Friends => "Friends",
        ShareScope::Room => "Room",
    }
}

pub const fn access_level_label(access: AccessLevel) -> &'static str {
    match access {
        AccessLevel::View => "View",
        AccessLevel::Suggest => "Suggest",
        AccessLevel::Inject => "Inject",
        AccessLevel::Approve => "Approve",
    }
}

pub const fn default_audience_access_label(access: DefaultAudienceAccess) -> &'static str {
    access_level_label(access.access_level())
}

pub(crate) fn parse_share_scope(value: &str) -> Option<ShareScope> {
    if value.eq_ignore_ascii_case("justme") {
        Some(ShareScope::JustMe)
    } else if value.eq_ignore_ascii_case("mydevices") {
        Some(ShareScope::MyDevices)
    } else if value.eq_ignore_ascii_case("friends") {
        Some(ShareScope::Friends)
    } else if value.eq_ignore_ascii_case("room") {
        Some(ShareScope::Room)
    } else {
        None
    }
}

pub(crate) fn parse_access_level(value: &str) -> Option<AccessLevel> {
    if value.eq_ignore_ascii_case("view") {
        Some(AccessLevel::View)
    } else if value.eq_ignore_ascii_case("suggest") {
        Some(AccessLevel::Suggest)
    } else if value.eq_ignore_ascii_case("inject") {
        Some(AccessLevel::Inject)
    } else if value.eq_ignore_ascii_case("approve") {
        Some(AccessLevel::Approve)
    } else {
        None
    }
}

pub const fn session_state_label(state: SessionState) -> &'static str {
    match state {
        SessionState::Starting => "Pending",
        SessionState::Running | SessionState::Published => "Live",
        SessionState::Reconnecting => "Reconnecting",
        SessionState::Stopping | SessionState::Stopped | SessionState::Failed => "Ended",
    }
}

pub(crate) fn parse_session_state(value: &str) -> Option<SessionState> {
    if value.eq_ignore_ascii_case("pending") {
        Some(SessionState::Starting)
    } else if value.eq_ignore_ascii_case("live") {
        Some(SessionState::Running)
    } else if value.eq_ignore_ascii_case("reconnecting") {
        Some(SessionState::Reconnecting)
    } else if value.eq_ignore_ascii_case("ended") {
        Some(SessionState::Stopped)
    } else {
        None
    }
}

pub(crate) const fn tool_kind_label(tool_kind: BackendToolKind) -> &'static str {
    match tool_kind {
        BackendToolKind::Generic => "Generic",
        BackendToolKind::Terminal => "Terminal",
        BackendToolKind::ClaudeCode => "ClaudeCode",
    }
}

pub(crate) fn parse_tool_kind(value: &str) -> Option<BackendToolKind> {
    if value.eq_ignore_ascii_case("generic") {
        Some(BackendToolKind::Generic)
    } else if value.eq_ignore_ascii_case("terminal") {
        Some(BackendToolKind::Terminal)
    } else if value.eq_ignore_ascii_case("claudecode") || value.eq_ignore_ascii_case("claude_code")
    {
        Some(BackendToolKind::ClaudeCode)
    } else {
        None
    }
}

#[cfg(test)]
pub(crate) fn merge_session_state(local: SessionState, value: &str) -> SessionState {
    let Some(incoming) = parse_session_state(value) else {
        return local;
    };
    SessionState::merge_local_with_incoming(local, incoming)
}

pub(crate) fn serialize_share_scope<S>(scope: &ShareScope, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(share_scope_label(*scope))
}

pub(crate) fn deserialize_share_scope<'de, D>(deserializer: D) -> Result<ShareScope, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    parse_share_scope(&value).ok_or_else(|| D::Error::custom(format!("unknown scope {value}")))
}

pub(crate) fn serialize_optional_share_scope<S>(
    scope: &Option<ShareScope>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match scope {
        Some(scope) => serializer.serialize_some(share_scope_label(*scope)),
        None => serializer.serialize_none(),
    }
}

pub(crate) fn serialize_access_level<S>(
    access: &AccessLevel,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(access_level_label(*access))
}

pub(crate) fn deserialize_access_level<'de, D>(deserializer: D) -> Result<AccessLevel, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    parse_access_level(&value)
        .ok_or_else(|| D::Error::custom(format!("unknown access level {value}")))
}

pub(crate) fn serialize_optional_access_level<S>(
    access: &Option<AccessLevel>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match access {
        Some(access) => serializer.serialize_some(access_level_label(*access)),
        None => serializer.serialize_none(),
    }
}

pub(crate) fn serialize_default_audience_access<S>(
    access: &DefaultAudienceAccess,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(default_audience_access_label(*access))
}

pub(crate) fn serialize_optional_default_audience_access<S>(
    access: &Option<DefaultAudienceAccess>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match access {
        Some(access) => serializer.serialize_some(default_audience_access_label(*access)),
        None => serializer.serialize_none(),
    }
}

pub(crate) fn deserialize_optional_access_level<'de, D>(
    deserializer: D,
) -> Result<Option<AccessLevel>, D::Error>
where
    D: Deserializer<'de>,
{
    let Some(value) = Option::<String>::deserialize(deserializer)? else {
        return Ok(None);
    };
    parse_access_level(&value)
        .map(Some)
        .ok_or_else(|| D::Error::custom(format!("unknown access level {value}")))
}

pub(crate) fn serialize_session_state<S>(
    state: &SessionState,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(session_state_label(*state))
}

pub(crate) fn deserialize_session_state<'de, D>(deserializer: D) -> Result<SessionState, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    parse_session_state(&value)
        .ok_or_else(|| D::Error::custom(format!("unknown session status {value}")))
}

pub(crate) fn serialize_tool_kind<S>(
    tool_kind: &BackendToolKind,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(tool_kind_label(*tool_kind))
}

pub(crate) fn deserialize_tool_kind<'de, D>(deserializer: D) -> Result<BackendToolKind, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    parse_tool_kind(&value).ok_or_else(|| D::Error::custom(format!("unknown tool kind {value}")))
}

#[cfg(test)]
mod tests {
    use kodosi_domain::session::SessionState;

    use super::{merge_session_state, parse_session_state};

    #[test]
    fn parses_backend_state_values() {
        assert_eq!(parse_session_state("Pending"), Some(SessionState::Starting));
        assert_eq!(parse_session_state("Live"), Some(SessionState::Running));
        assert_eq!(
            parse_session_state("Reconnecting"),
            Some(SessionState::Reconnecting)
        );
        assert_eq!(parse_session_state("Ended"), Some(SessionState::Stopped));
        assert_eq!(parse_session_state("Unknown"), None);
    }

    #[test]
    fn merge_preserves_published_across_live_tick() {
        assert_eq!(
            merge_session_state(SessionState::Published, "Live"),
            SessionState::Published
        );
    }

    #[test]
    fn merge_preserves_stopping_across_non_terminal_ticks() {
        for incoming in ["Pending", "Live", "Reconnecting"] {
            assert_eq!(
                merge_session_state(SessionState::Stopping, incoming),
                SessionState::Stopping,
                "{incoming} must not clobber Stopping"
            );
        }
    }

    #[test]
    fn merge_accepts_ended_over_every_local_state() {
        for local in [
            SessionState::Starting,
            SessionState::Running,
            SessionState::Published,
            SessionState::Reconnecting,
            SessionState::Stopping,
            SessionState::Stopped,
            SessionState::Failed,
        ] {
            assert_eq!(
                merge_session_state(local, "Ended"),
                SessionState::Stopped,
                "Ended must stop {local:?}"
            );
        }
    }

    #[test]
    fn merge_ignores_unknown_incoming() {
        assert_eq!(
            merge_session_state(SessionState::Published, "Garbage"),
            SessionState::Published
        );
    }

    #[test]
    fn merge_preserves_stopped_across_non_terminal_ticks() {
        for incoming in ["Pending", "Live", "Reconnecting"] {
            assert_eq!(
                merge_session_state(SessionState::Stopped, incoming),
                SessionState::Stopped,
                "{incoming} must not revive Stopped"
            );
        }
    }
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ShareScope {
    JustMe,

    MyDevices,
    Friends,
    Room,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum AccessLevel {
    View,
    Suggest,
    Inject,
    Approve,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DefaultAudienceAccess {
    View,
    Suggest,
}

impl DefaultAudienceAccess {
    #[must_use]
    pub const fn clamp(access: AccessLevel) -> Self {
        match access {
            AccessLevel::View => Self::View,
            AccessLevel::Suggest | AccessLevel::Inject | AccessLevel::Approve => Self::Suggest,
        }
    }

    #[must_use]
    pub const fn access_level(self) -> AccessLevel {
        match self {
            Self::View => AccessLevel::View,
            Self::Suggest => AccessLevel::Suggest,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(transparent)]
pub struct SessionCapabilities(pub u16);

impl SessionCapabilities {
    pub const VIEW: u16 = 1 << 0;
    pub const SUGGEST: u16 = 1 << 1;
    pub const SEND_INPUT: u16 = 1 << 2;
    pub const APPROVE_DENY: u16 = 1 << 3;
    pub const RESIZE: u16 = 1 << 4;
    pub const FOCUS: u16 = 1 << 5;
    pub const STOP: u16 = 1 << 6;
    pub const OWNER: u16 = Self::VIEW
        | Self::SUGGEST
        | Self::SEND_INPUT
        | Self::APPROVE_DENY
        | Self::RESIZE
        | Self::FOCUS
        | Self::STOP;

    #[must_use]
    pub const fn from_access(access: AccessLevel, owner: bool) -> Self {
        if owner {
            return Self(Self::OWNER);
        }
        Self(match access {
            AccessLevel::View => Self::VIEW,
            AccessLevel::Suggest => Self::VIEW | Self::SUGGEST,
            AccessLevel::Inject => Self::VIEW | Self::SUGGEST | Self::SEND_INPUT | Self::FOCUS,
            AccessLevel::Approve => Self::VIEW | Self::APPROVE_DENY,
        })
    }

    #[must_use]
    pub const fn allows(self, capability: u16) -> bool {
        capability != 0 && (self.0 & capability) == capability
    }
}

#[cfg(test)]
mod tests {
    use super::{AccessLevel, DefaultAudienceAccess, SessionCapabilities};

    #[test]
    fn non_owner_inject_does_not_advertise_owner_only_resize() {
        let inject = SessionCapabilities::from_access(AccessLevel::Inject, false);
        assert!(inject.allows(SessionCapabilities::SEND_INPUT));
        assert!(inject.allows(SessionCapabilities::FOCUS));
        assert!(!inject.allows(SessionCapabilities::RESIZE));
    }

    #[test]
    fn clamp_maps_owner_only_rungs_down_to_suggest() {
        assert_eq!(
            DefaultAudienceAccess::clamp(AccessLevel::Inject),
            DefaultAudienceAccess::Suggest
        );
        assert_eq!(
            DefaultAudienceAccess::clamp(AccessLevel::Approve),
            DefaultAudienceAccess::Suggest
        );
    }

    #[test]
    fn clamp_preserves_audience_rungs() {
        assert_eq!(
            DefaultAudienceAccess::clamp(AccessLevel::View),
            DefaultAudienceAccess::View
        );
        assert_eq!(
            DefaultAudienceAccess::clamp(AccessLevel::Suggest),
            DefaultAudienceAccess::Suggest
        );
    }

    #[test]
    fn every_audience_default_is_a_level_the_backend_accepts() {
        for default in [DefaultAudienceAccess::View, DefaultAudienceAccess::Suggest] {
            let access = default.access_level();
            assert!(
                matches!(access, AccessLevel::View | AccessLevel::Suggest),
                "{access:?} is rejected as a stored session default"
            );
            assert_eq!(DefaultAudienceAccess::clamp(access), default);
        }
    }
}

use std::collections::BTreeMap;

use kodosi_domain::permissions::{AccessLevel, SessionCapabilities};

use crate::host_protocol::PermissionFlags;

pub(super) fn desktop_bit_positions() -> BTreeMap<String, u32> {
    bit_positions([
        ("view", PermissionFlags::VIEW.bits()),
        ("sendInput", PermissionFlags::SEND_INPUT.bits()),
        ("resize", PermissionFlags::RESIZE.bits()),
        ("focusBlur", PermissionFlags::FOCUS_BLUR.bits()),
        ("stop", PermissionFlags::STOP.bits()),
        ("rename", PermissionFlags::RENAME.bits()),
        ("delete", PermissionFlags::DELETE.bits()),
        ("setMode", PermissionFlags::SET_MODE.bits()),
        ("approve", PermissionFlags::APPROVE.bits()),
    ])
}

pub(super) fn desktop_role_masks() -> BTreeMap<String, u32> {
    BTreeMap::from([
        ("approve".to_owned(), desktop_role(AccessLevel::Approve)),
        ("inject".to_owned(), desktop_role(AccessLevel::Inject)),
        ("owner".to_owned(), PermissionFlags::OWNER.bits()),
        ("suggest".to_owned(), desktop_role(AccessLevel::Suggest)),
        ("view".to_owned(), desktop_role(AccessLevel::View)),
    ])
}

pub(super) fn relay_bit_positions() -> BTreeMap<String, u32> {
    bit_positions([
        ("view", u32::from(SessionCapabilities::VIEW)),
        ("suggest", u32::from(SessionCapabilities::SUGGEST)),
        ("sendInput", u32::from(SessionCapabilities::SEND_INPUT)),
        ("approveDeny", u32::from(SessionCapabilities::APPROVE_DENY)),
        ("resize", u32::from(SessionCapabilities::RESIZE)),
        ("focus", u32::from(SessionCapabilities::FOCUS)),
        ("stop", u32::from(SessionCapabilities::STOP)),
    ])
}

pub(super) fn relay_role_masks() -> BTreeMap<String, u32> {
    BTreeMap::from([
        ("approve".to_owned(), relay_role(AccessLevel::Approve)),
        ("inject".to_owned(), relay_role(AccessLevel::Inject)),
        ("owner".to_owned(), u32::from(SessionCapabilities::OWNER)),
        ("suggest".to_owned(), relay_role(AccessLevel::Suggest)),
        ("view".to_owned(), relay_role(AccessLevel::View)),
    ])
}

fn desktop_role(access: AccessLevel) -> u32 {
    PermissionFlags::from_access(access).bits()
}

fn relay_role(access: AccessLevel) -> u32 {
    u32::from(SessionCapabilities::from_access(access, false).0)
}

fn bit_positions<const N: usize>(bits: [(&str, u32); N]) -> BTreeMap<String, u32> {
    bits.into_iter()
        .map(|(name, mask)| {
            debug_assert!(mask.is_power_of_two());
            (name.to_owned(), mask.trailing_zeros())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn permission_contract_positions_and_roles_are_stable() {
        assert_eq!(
            serde_json::to_value(desktop_bit_positions()).expect("desktop bits"),
            json!({
                "approve": 8,
                "delete": 6,
                "focusBlur": 3,
                "rename": 5,
                "resize": 2,
                "sendInput": 1,
                "setMode": 7,
                "stop": 4,
                "view": 0
            })
        );
        assert_eq!(
            serde_json::to_value(desktop_role_masks()).expect("desktop roles"),
            json!({"approve": 257, "inject": 15, "owner": 511, "suggest": 1, "view": 1})
        );
        assert_eq!(
            serde_json::to_value(relay_bit_positions()).expect("relay bits"),
            json!({
                "approveDeny": 3,
                "focus": 5,
                "resize": 4,
                "sendInput": 2,
                "stop": 6,
                "suggest": 1,
                "view": 0
            })
        );
        assert_eq!(
            serde_json::to_value(relay_role_masks()).expect("relay roles"),
            json!({"approve": 9, "inject": 39, "owner": 127, "suggest": 3, "view": 1})
        );
    }
}

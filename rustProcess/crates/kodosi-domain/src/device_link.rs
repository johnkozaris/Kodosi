use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum DeviceLinkOutcome {
    Approved,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum SelfDeviceLinkOutcome {
    Approved,
    Cancelled,
    Expired,
    Failed,
}

const USER_CODE_GROUP_LEN: usize = 4;

const USER_CODE_GROUPS: usize = 2;

const USER_CODE_LEN: usize = USER_CODE_GROUP_LEN * USER_CODE_GROUPS;

#[must_use]
pub fn normalize_user_code(raw: &str) -> String {
    let stripped: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if stripped.len() == USER_CODE_LEN && stripped.chars().all(|c| c.is_ascii_alphanumeric()) {
        format!(
            "{}-{}",
            &stripped[..USER_CODE_GROUP_LEN],
            &stripped[USER_CODE_GROUP_LEN..]
        )
    } else {
        stripped
    }
}

const USER_CODE_ALPHABET: &[u8] = b"BCDFGHJKMNPQRSTVWXZ23456789";

#[must_use]
pub fn is_canonical_user_code(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != USER_CODE_LEN + (USER_CODE_GROUPS - 1) {
        return false;
    }
    bytes.iter().enumerate().all(|(index, byte)| {
        if index == USER_CODE_GROUP_LEN {
            *byte == b'-'
        } else {
            USER_CODE_ALPHABET.contains(byte)
        }
    })
}

#[cfg(test)]
mod user_code_tests {
    use super::{is_canonical_user_code, normalize_user_code};

    #[test]
    fn normalize_accepts_every_shape_a_user_can_type() {
        for raw in ["bcdf2345", "BCDF-2345", " bcdf - 2345 ", "b c d f 2 3 4 5"] {
            assert_eq!(normalize_user_code(raw), "BCDF-2345", "raw = {raw}");
            assert!(is_canonical_user_code(&normalize_user_code(raw)));
        }
    }

    #[test]
    fn normalize_leaves_non_conforming_input_undashed_and_non_canonical() {
        for raw in ["abc", "abcdefghij", "αβγδεζηθ", "abcédefg", "abcd!efg"] {
            assert!(
                !is_canonical_user_code(&normalize_user_code(raw)),
                "raw = {raw} must not pass as canonical"
            );
        }
    }

    #[test]
    fn canonical_check_rejects_wrong_dash_position_and_lowercase() {
        assert!(!is_canonical_user_code("ABCDE-FGH"));
        assert!(!is_canonical_user_code("abcd-efgh"));
        assert!(!is_canonical_user_code("ABCDEFGH"));
        assert!(!is_canonical_user_code("AAAA-0000"));
        assert!(!is_canonical_user_code("ABCD-EFGH"));
        assert!(is_canonical_user_code("BCDF-2345"));
    }
}

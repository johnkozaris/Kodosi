use serde::{Deserialize, Serialize};

use super::DegradationNotice;

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentIdentity {
    pub agent_type: String,
    pub version: Option<String>,
    pub model: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub parser_compat: Option<ParserCompatNotice>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ParserCompatNotice {
    pub unknown_record_types: Vec<String>,

    #[serde(default, skip_serializing_if = "is_zero")]
    pub unrecognized_envelopes: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub degradation_notices: Vec<DegradationNotice>,
}

impl ParserCompatNotice {
    #[must_use]
    pub fn from_parts(
        unknown_record_types: Vec<String>,
        unrecognized_envelopes: u32,
        degradation_notices: Vec<DegradationNotice>,
    ) -> Option<Self> {
        if unknown_record_types.is_empty()
            && unrecognized_envelopes == 0
            && degradation_notices.is_empty()
        {
            return None;
        }
        Some(Self {
            unknown_record_types,
            unrecognized_envelopes,
            degradation_notices,
        })
    }
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if requires a by-reference predicate"
)]
fn is_zero(value: &u32) -> bool {
    *value == 0
}

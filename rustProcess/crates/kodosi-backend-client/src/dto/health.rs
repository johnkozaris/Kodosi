use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendCompatibilityDto {
    pub api_contract_version: u32,
    pub auth_contract_version: u32,
}

#[cfg(test)]
mod tests {
    use super::BackendCompatibilityDto;

    #[test]
    fn rejects_legacy_health_payload_without_contract_versions() {
        let error = serde_json::from_str::<BackendCompatibilityDto>(r#"{"status":"ready"}"#)
            .expect_err("legacy backend must not pass compatibility preflight");
        assert!(error.to_string().contains("apiContractVersion"));
    }
}

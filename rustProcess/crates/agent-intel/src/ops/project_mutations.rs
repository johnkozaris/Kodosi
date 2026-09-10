use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use specta::Type;

const MAX_RECEIPTS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ProjectMemoryCopyOutcome {
    Copied,
    AlreadyExists,
    Indeterminate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMemoryCopyReceipt {
    pub mutation_id: String,
    pub outcome: ProjectMemoryCopyOutcome,
    pub filename: String,
    pub target_label: String,
    pub detail: Option<String>,
}

impl ProjectMemoryCopyReceipt {
    pub fn validate(&self) -> Result<(), String> {
        match self.outcome {
            ProjectMemoryCopyOutcome::Copied | ProjectMemoryCopyOutcome::AlreadyExists => {
                if self.detail.is_some() {
                    return Err(
                        "confirmed project memory copy outcome must not include detail".to_owned(),
                    );
                }
            }
            ProjectMemoryCopyOutcome::Indeterminate => {
                if self.detail.as_deref().is_none_or(str::is_empty) {
                    return Err(
                        "indeterminate project memory copy outcome requires detail".to_owned()
                    );
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
pub struct ProjectMutationLedger {
    inner: Arc<Mutex<ProjectMutationLedgerInner>>,
}

#[derive(Debug, Default)]
struct ProjectMutationLedgerInner {
    receipts: HashMap<uuid::Uuid, ProjectMemoryCopyReceipt>,
    order: VecDeque<uuid::Uuid>,
}

impl ProjectMutationLedger {
    pub fn existing(&self, mutation_id: &str) -> Result<Option<ProjectMemoryCopyReceipt>, String> {
        let id = parse_mutation_id(mutation_id)?;
        Ok(self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .receipts
            .get(&id)
            .cloned())
    }

    pub fn record(
        &self,
        mutation_id: &str,
        outcome: ProjectMemoryCopyOutcome,
        filename: String,
        target_label: String,
        detail: Option<String>,
    ) -> Result<ProjectMemoryCopyReceipt, String> {
        let id = parse_mutation_id(mutation_id)?;
        let receipt = ProjectMemoryCopyReceipt {
            mutation_id: id.to_string(),
            outcome,
            filename,
            target_label,
            detail,
        };
        receipt.validate()?;
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(existing) = inner.receipts.get(&id) {
            return Ok(existing.clone());
        }
        while inner.receipts.len() >= MAX_RECEIPTS {
            let Some(oldest) = inner.order.pop_front() else {
                break;
            };
            inner.receipts.remove(&oldest);
        }
        inner.order.push_back(id);
        inner.receipts.insert(id, receipt.clone());
        drop(inner);
        Ok(receipt)
    }

    pub fn mark_indeterminate(
        &self,
        mutation_id: &str,
        detail: String,
    ) -> Result<ProjectMemoryCopyReceipt, String> {
        if detail.is_empty() {
            return Err("indeterminate project memory copy outcome requires detail".to_owned());
        }
        let id = parse_mutation_id(mutation_id)?;
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let receipt = inner
            .receipts
            .get_mut(&id)
            .ok_or_else(|| "project memory copy outcome is not retained".to_owned())?;
        receipt.outcome = ProjectMemoryCopyOutcome::Indeterminate;
        receipt.detail = Some(detail);
        let receipt = receipt.clone();
        drop(inner);
        Ok(receipt)
    }

    pub fn reconcile(&self, mutation_id: &str) -> Result<ProjectMemoryCopyReceipt, String> {
        self.existing(mutation_id)?
            .ok_or_else(|| "project memory copy outcome is not retained".to_owned())
    }

    pub fn clear(&mut self) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.receipts.clear();
        inner.order.clear();
    }
}

fn parse_mutation_id(value: &str) -> Result<uuid::Uuid, String> {
    let id = uuid::Uuid::parse_str(value)
        .map_err(|_| "invalid project memory mutation id".to_owned())?;
    if id.get_version_num() != 7 || id.hyphenated().to_string() != value {
        return Err("invalid project memory mutation id".to_owned());
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_is_exact_and_reconcilable() {
        let ledger = ProjectMutationLedger::default();
        let id = uuid::Uuid::now_v7().to_string();
        let receipt = ledger
            .record(
                &id,
                ProjectMemoryCopyOutcome::Copied,
                "MEMORY.md".to_owned(),
                "target".to_owned(),
                None,
            )
            .unwrap();
        assert_eq!(ledger.reconcile(&id).unwrap(), receipt);
        assert_eq!(
            ledger
                .record(
                    &id,
                    ProjectMemoryCopyOutcome::AlreadyExists,
                    "other.md".to_owned(),
                    "other".to_owned(),
                    None,
                )
                .unwrap(),
            receipt
        );
    }

    #[test]
    fn malformed_outcome_detail_coupling_is_rejected_atomically() {
        let ledger = ProjectMutationLedger::default();
        let confirmed = uuid::Uuid::now_v7().to_string();
        assert!(
            ledger
                .record(
                    &confirmed,
                    ProjectMemoryCopyOutcome::Copied,
                    "MEMORY.md".to_owned(),
                    "target".to_owned(),
                    Some("unexpected".to_owned()),
                )
                .is_err()
        );
        assert!(ledger.existing(&confirmed).unwrap().is_none());

        let indeterminate = uuid::Uuid::now_v7().to_string();
        assert!(
            ledger
                .record(
                    &indeterminate,
                    ProjectMemoryCopyOutcome::Indeterminate,
                    "MEMORY.md".to_owned(),
                    "target".to_owned(),
                    Some(String::new()),
                )
                .is_err()
        );
        assert!(ledger.existing(&indeterminate).unwrap().is_none());
    }
}

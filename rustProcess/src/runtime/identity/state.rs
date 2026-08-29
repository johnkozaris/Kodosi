use crate::session_runtime::events::{AccountEpoch, AccountEventOrigin};
use kodosi_domain::auth::AuthState;

use super::{AccountRuntimes, DeviceFlowRuntime};

#[derive(Debug)]
pub(crate) struct IdentityState {
    pub(crate) auth: AuthState,
    pub(crate) account_runtimes: AccountRuntimes,
    pub(crate) device_flow: DeviceFlowRuntime,
    pub(crate) current_user_name: Option<String>,
    account_epoch: AccountEpoch,
}

impl IdentityState {
    pub(crate) fn new(device_flow: DeviceFlowRuntime) -> Self {
        Self {
            auth: AuthState::SignedOut,
            account_runtimes: AccountRuntimes::default(),
            device_flow,
            current_user_name: None,
            account_epoch: AccountEpoch::INITIAL,
        }
    }

    pub(crate) const fn account_epoch(&self) -> AccountEpoch {
        self.account_epoch
    }

    pub(crate) fn advance_account_epoch(&mut self) -> crate::Result<AccountEpoch> {
        let next = self.account_epoch.next()?;
        self.account_epoch = next;
        Ok(next)
    }

    #[cfg(test)]
    pub(crate) fn set_account_epoch_for_test(&mut self, epoch: AccountEpoch) {
        self.account_epoch = epoch;
    }

    pub(crate) fn event_context(&self) -> (Option<String>, u64) {
        (self.auth.subject_string(), self.account_epoch.value())
    }

    pub(crate) fn current_account_event_origin(&self) -> Option<AccountEventOrigin> {
        let AuthState::Authenticated {
            subject: Some(subject),
            ..
        } = &self.auth
        else {
            return None;
        };
        Some(AccountEventOrigin {
            account_user_id: subject.to_string(),
            epoch: self.account_epoch,
        })
    }

    pub(crate) fn accepts_account_event(&self, origin: &AccountEventOrigin) -> bool {
        self.auth.is_authenticated()
            && self.account_epoch == origin.epoch
            && self.identity_subject_matches(origin.account_user_id.as_str())
    }

    fn identity_subject_matches(&self, account_user_id: &str) -> bool {
        self.auth.subject_string().as_deref() == Some(account_user_id)
    }
}

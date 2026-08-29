use std::collections::VecDeque;

use crate::{
    Result, identity_core::device_list_pin_store::DeviceListPinStoreHandle,
    runtime::state::push_log,
};

pub(crate) async fn bind_pin_store_for_subject(
    user_id: &str,
    pin_store: &DeviceListPinStoreHandle,
    logs: &mut VecDeque<String>,
) -> Result<()> {
    match pin_store.bind_to_user(user_id).await {
        Ok(true) => push_log(logs, "switched to account-scoped pin store".to_owned()),
        Ok(false) => {}
        Err(error) => {
            tracing::warn!(%error, "failed to bind pin store to user");
            push_log(logs, format!("failed to bind pin store: {error}"));
            return Err(error);
        }
    }
    Ok(())
}

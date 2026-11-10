use std::collections::{BTreeSet, HashMap};

use serde_json::Value;
use tokio::time::{Duration, Instant};
use uuid::Uuid;

use super::{Completion, MAX_JOBS, MAX_SESSIONS, Runtime};
use crate::{Command, Error, EventBody, Result, protocol::SessionFolder};

const RETRY: Duration = Duration::from_secs(30);

#[derive(Default)]
pub(super) struct FolderAsk {
    pub(super) asking: bool,
    pub(super) asked: BTreeSet<Uuid>,
    pub(super) retry: Option<Instant>,
}

impl Runtime {
    #[cfg(feature = "host")]
    pub(super) fn session_folders(&self) -> Vec<SessionFolder> {
        self.local
            .iter()
            .filter_map(|(id, local)| {
                Some(SessionFolder {
                    session_id: id.to_string(),
                    working_dir: local.entry.working_dir.clone()?,
                })
            })
            .filter(SessionFolder::is_valid)
            .take(MAX_SESSIONS)
            .collect()
    }

    #[cfg(not(feature = "host"))]
    #[expect(clippy::unused_self, reason = "a viewer hosts no terminals")]
    pub(super) const fn session_folders(&self) -> Vec<SessionFolder> {
        Vec::new()
    }

    /// Asks each other computer of the person, one call at a time, for the folders of its
    /// terminals that this device does not know yet.
    pub(super) fn ask_folders(&mut self) {
        let Some(identity) = self.network.identity().filter(|identity| identity.enrolled) else {
            return;
        };
        let mut unknown = HashMap::<String, BTreeSet<Uuid>>::new();
        for (id, entry) in &self.remotes {
            let Some(host) = entry
                .host_device_id
                .clone()
                .filter(|host| entry.is_owner && *host != identity.device_id)
            else {
                continue;
            };
            let ids = unknown.entry(host).or_default();
            if entry.working_dir.is_none() && self.host_online(*id) && !self.streams(*id) {
                ids.insert(*id);
            }
        }
        self.folder_asks
            .retain(|device, ask| ask.asking || unknown.contains_key(device));
        let now = Instant::now();
        let mut room = MAX_JOBS.saturating_sub(self.jobs.len());
        for (device, ids) in unknown {
            let ask = self.folder_asks.entry(device.clone()).or_default();
            ask.asked.retain(|id| ids.contains(id));
            if room == 0
                || ask.asking
                || ask.retry.is_some_and(|retry| retry > now)
                || ids.is_subset(&ask.asked)
            {
                continue;
            }
            let Ok(request) = serde_json::to_value(Command::ListSessionFolders {
                request_id: Uuid::now_v7().to_string(),
            }) else {
                continue;
            };
            ask.asking = true;
            room -= 1;
            let network = self.network.clone();
            self.spawn(async move {
                let result = network
                    .ask_computer(&device, request)
                    .await
                    .map_err(Error::from);
                Completion::TerminalFolders {
                    device,
                    asked: ids,
                    result,
                }
            });
        }
    }

    pub(super) fn folders_answered(
        &mut self,
        device: &str,
        asked: BTreeSet<Uuid>,
        result: Result<Value>,
    ) {
        let Some(ask) = self.folder_asks.get_mut(device) else {
            return;
        };
        ask.asking = false;
        match result.and_then(folders_of) {
            Ok(folders) => {
                ask.asked.extend(asked);
                ask.retry = None;
                self.learn_folders(device, folders);
            }
            Err(error) => {
                ask.retry = Some(Instant::now() + RETRY);
                tracing::debug!(%error, "another computer did not give the folders of its terminals");
            }
        }
        self.ask_folders();
    }

    fn learn_folders(&mut self, device: &str, folders: Vec<SessionFolder>) {
        let mut changed = false;
        for folder in folders {
            let Ok(id) = Uuid::parse_str(&folder.session_id) else {
                continue;
            };
            if self.streams(id) {
                continue;
            }
            if let Some(entry) = self
                .remotes
                .get_mut(&id)
                .filter(|entry| entry.is_owner && entry.host_device_id.as_deref() == Some(device))
                && entry.working_dir.as_ref() != Some(&folder.working_dir)
            {
                entry.working_dir = Some(folder.working_dir);
                changed = true;
            }
        }
        if changed {
            self.publish_catalog();
        }
    }
}

fn folders_of(answer: Value) -> Result<Vec<SessionFolder>> {
    match EventBody::new(answer) {
        Ok(EventBody::SessionResult {
            folders: Some(folders),
            ..
        }) if folders.len() <= MAX_SESSIONS && folders.iter().all(SessionFolder::is_valid) => {
            Ok(folders)
        }
        Ok(EventBody::SessionError { message, .. }) => Err(Error::Text(message)),
        _ => Err(Error::Text(crate::protocol::other_version())),
    }
}

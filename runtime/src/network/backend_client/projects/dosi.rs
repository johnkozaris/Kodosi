use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::{
    BackendClient, Credentials, Error, Item, Lookups, Method, Payload, ProjectCache, Result, Uuid,
    Value, invalid, json,
};
use crate::dosi::{DosiEntry, DosiLog, DosiRecord, SharedInstructions};

const PAGE: usize = 200;
const PINS_LIMIT: usize = 64 * 1024;

impl ProjectCache {
    fn dosi_alone(&self, credentials: &Credentials, project: Uuid) -> Result<()> {
        self.current(credentials)?;
        let state = &self
            .projects
            .get(&project)
            .and_then(|cached| cached.states.last_key_value())
            .ok_or_else(|| Error::Text(text!("Project keys are missing.")))?
            .1
            .state;
        if state.owner_user_id == credentials.user_id
            && state.members.keys().eq([&credentials.user_id])
        {
            Ok(())
        } else {
            Err(invalid(
                "The server offered Dosi a project that is not yours alone.",
            ))
        }
    }

    fn dosi_devices(&self, credentials: &Credentials, project: Uuid) -> BTreeSet<String> {
        self.projects
            .get(&project)
            .and_then(|cached| cached.states.last_key_value())
            .and_then(|(_, state)| state.identities.get(&credentials.user_id))
            .map(|identity| {
                identity
                    .devices
                    .keys()
                    .filter(|device| !crate::identity::recovery::is_recovery_device(device))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn dosi_records(
        &self,
        credentials: &Credentials,
        project: Uuid,
        items: &[Item],
        log: &mut DosiLog,
    ) -> Result<Option<String>> {
        self.dosi_alone(credentials, project)?;
        let cached = self
            .projects
            .get(&project)
            .ok_or_else(|| Error::Text(text!("Project keys are missing.")))?;
        let mut instructed = None;
        for item in items {
            if item.user_id != credentials.user_id {
                continue;
            }
            match BackendClient::project_payload(cached, project, item) {
                Ok(Payload::Dosi { record }) if item.kind == "message" => {
                    log.entries.push(DosiEntry {
                        sequence: item.sequence,
                        mine: item.device_id == credentials.keys.device_id,
                        device: item.device_id.clone(),
                        record,
                    });
                }
                Ok(Payload::DosiInstructions { text })
                    if item.kind == "task" && item.id == project =>
                {
                    log.instructions = Some(SharedInstructions { text });
                    instructed = Some(item.device_id.clone());
                }
                Ok(_) => {}
                Err(error) => tracing::debug!(%error, "Dosi passed over a record it cannot read"),
            }
        }
        Ok(instructed)
    }
}

impl BackendClient {
    fn dosi_credentials(&self) -> Result<Credentials> {
        let credentials = self.credentials()?;
        if credentials.enrolled {
            Ok(credentials)
        } else {
            Err(Error::EnrollmentRequired)
        }
    }

    async fn listed_dosi(&self, credentials: &Credentials) -> Result<Option<Uuid>> {
        let list: Value = self
            .inner
            .http
            .device(Method::GET, "api/projects", credentials, None)
            .await?;
        list.get("dosi")
            .and_then(Value::as_str)
            .map(|project| {
                Uuid::parse_str(project).map_err(|_| invalid("Invalid project identity."))
            })
            .transpose()
    }

    async fn dosi_project(&self, credentials: &Credentials) -> Result<Uuid> {
        {
            let cache = self.inner.projects.lock().await;
            cache.current(credentials)?;
            if let Some(project) = cache.dosi {
                return Ok(project);
            }
        }
        let project = if let Some(project) = self.pinned_dosi(credentials)? {
            project
        } else if let Some(project) = self.listed_dosi(credentials).await? {
            project
        } else {
            self.make_dosi(credentials).await?
        };
        let mut cache = self.inner.projects.lock().await;
        cache.current(credentials)?;
        cache.dosi = Some(project);
        drop(cache);
        Ok(project)
    }

    async fn own_dosi(&self, credentials: &Credentials) -> Result<Uuid> {
        let project = self.dosi_project(credentials).await?;
        let owned = async {
            self.refresh_project_keys(credentials, project, false, &mut Lookups::default())
                .await?;
            self.inner
                .projects
                .lock()
                .await
                .dosi_alone(credentials, project)?;
            self.pin_dosi(credentials, Some(project))
        }
        .await;
        self.dosi_kept(credentials, project, owned)
            .await
            .map(|()| project)
    }

    fn dosi_pins(&self) -> Result<(PathBuf, BTreeMap<String, Uuid>)> {
        let path = self.inner.config.data_root.join("network/dosi-rooms.json");
        let pins = match std::fs::read(&path) {
            Ok(bytes) if bytes.len() <= PINS_LIMIT => serde_json::from_slice(&bytes)?,
            Ok(_) => return Err(invalid("Saved Dosi projects exceed their limit.")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => return Err(error.into()),
        };
        Ok((path, pins))
    }

    pub(super) fn pinned_dosi(&self, credentials: &Credentials) -> Result<Option<Uuid>> {
        Ok(self.dosi_pins()?.1.get(&credentials.user_id).copied())
    }

    fn pin_dosi(&self, credentials: &Credentials, project: Option<Uuid>) -> Result<()> {
        let (path, mut pins) = self.dosi_pins()?;
        let user = credentials.user_id.clone();
        let changed = match project {
            Some(project) => pins.insert(user, project) != Some(project),
            None => pins.remove(&user).is_some(),
        };
        if changed {
            kodosi_os::fs::write_private(&path, &serde_json::to_vec(&pins)?)?;
        }
        Ok(())
    }

    async fn make_dosi(&self, credentials: &Credentials) -> Result<Uuid> {
        let id = Uuid::now_v7();
        let created = self
            .inner
            .http
            .device::<Value>(
                Method::POST,
                "api/projects",
                credentials,
                Some(json!({"id":id,"name":"Dosi","dosi":true})),
            )
            .await;
        match created {
            Ok(_) => Ok(id),
            Err(Error::Backend { status: 409, .. }) => self
                .listed_dosi(credentials)
                .await?
                .ok_or_else(|| Error::Text(text!("Dosi's conversation is missing."))),
            Err(error) => Err(error),
        }
    }

    async fn dosi_kept<T>(
        &self,
        credentials: &Credentials,
        project: Uuid,
        result: Result<T>,
    ) -> Result<T> {
        if matches!(
            result,
            Err(Error::Backend {
                status: 403 | 404,
                ..
            })
        ) {
            self.forget_project(project).await;
            let current = self
                .inner
                .projects
                .lock()
                .await
                .current(credentials)
                .is_ok();
            if current
                && self.pinned_dosi(credentials).ok().flatten() == Some(project)
                && let Err(error) = self.pin_dosi(credentials, None)
            {
                tracing::warn!(%error, "Dosi could not forget a project that is gone");
            }
        }
        result
    }

    pub(crate) async fn dosi_write(&self, record: &DosiRecord) -> Result<u64> {
        let credentials = self.dosi_credentials()?;
        let project = self.own_dosi(&credentials).await?;
        let payload = Payload::Dosi {
            record: record.clone(),
        };
        let written = self
            .project_write(
                &credentials,
                project,
                Uuid::now_v7(),
                "message",
                0,
                &payload,
            )
            .await;
        self.dosi_kept(&credentials, project, written).await
    }

    #[cfg(feature = "host")]
    pub(crate) async fn dosi_save(&self, instructions: SharedInstructions) -> Result<()> {
        let credentials = self.dosi_credentials()?;
        let project = self.own_dosi(&credentials).await?;
        let payload = Payload::DosiInstructions {
            text: instructions.text,
        };
        let saved = async {
            for _ in 0..3 {
                let page = self
                    .read_page(
                        &credentials,
                        project,
                        &format!("kind=task&limit={PAGE}&after=0"),
                    )
                    .await?;
                let version = page
                    .items
                    .iter()
                    .find(|item| item.id == project)
                    .map_or(0, |item| item.version);
                match self
                    .project_write(&credentials, project, project, "task", version, &payload)
                    .await
                {
                    Err(Error::Backend { status: 409, .. }) => {}
                    written => return written.map(|_| ()),
                }
            }
            Err(Error::Busy)
        }
        .await;
        self.dosi_kept(&credentials, project, saved).await
    }

    pub(crate) async fn dosi_read(&self, after: u64) -> Result<DosiLog> {
        let credentials = self.dosi_credentials()?;
        let project = self.own_dosi(&credentials).await?;
        let read = self.read_dosi(&credentials, project, after).await;
        self.dosi_kept(&credentials, project, read).await
    }

    async fn read_dosi(
        &self,
        credentials: &Credentials,
        project: Uuid,
        after: u64,
    ) -> Result<DosiLog> {
        self.inner
            .projects
            .lock()
            .await
            .writing_key(credentials, project)?;
        let mut log = DosiLog::default();
        let mut instructed = None;
        let mut after = after;
        loop {
            let page = self
                .read_page(credentials, project, &format!("limit={PAGE}&after={after}"))
                .await?;
            if page.sequence < after {
                log.reset = true;
                return Ok(log);
            }
            let cache = self.inner.projects.lock().await;
            if let Some(device) = cache.dosi_records(credentials, project, &page.items, &mut log)? {
                instructed = Some(device);
            }
            let last = page.items.last().map(|item| item.sequence);
            if !page.has_more {
                log.devices = cache.dosi_devices(credentials, project);
                if instructed.is_some_and(|device| !log.devices.contains(&device)) {
                    log.instructions = None;
                }
                log.sequence = last.unwrap_or(after).max(page.sequence);
                return Ok(log);
            }
            drop(cache);
            after = last
                .filter(|last| *last > after)
                .ok_or_else(|| invalid("Project history did not advance."))?;
        }
    }
}

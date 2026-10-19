use super::{
    BackendClient, Credentials, Error, Item, Lookups, Method, Payload, Result, RoomCache, Uuid,
    Value, invalid, json,
};
use crate::dosi::{DosiEntry, DosiLog, DosiRecord, SharedInstructions};

const PAGE: usize = 200;

impl RoomCache {
    fn dosi_records(
        &self,
        credentials: &Credentials,
        room: Uuid,
        items: &[Item],
        log: &mut DosiLog,
    ) -> Result<()> {
        self.current(credentials)?;
        let cached = self
            .rooms
            .get(&room)
            .ok_or_else(|| invalid("Room keys are missing."))?;
        for item in items {
            match BackendClient::room_payload(cached, room, item) {
                Ok(Payload::Dosi { record }) if item.kind == "message" => {
                    log.entries.push(DosiEntry {
                        sequence: item.sequence,
                        mine: item.device_id == credentials.keys.device_id,
                        record,
                    });
                }
                Ok(Payload::DosiInstructions { text })
                    if item.kind == "task" && item.id == room =>
                {
                    log.instructions = Some(SharedInstructions { text });
                }
                Ok(_) => {}
                Err(error) => tracing::debug!(%error, "Dosi passed over a record it cannot read"),
            }
        }
        Ok(())
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
            .device(Method::GET, "api/missions", credentials, None)
            .await?;
        list.get("dosi")
            .and_then(Value::as_str)
            .map(|room| Uuid::parse_str(room).map_err(|_| invalid("Invalid room identity.")))
            .transpose()
    }

    async fn dosi_room(&self, credentials: &Credentials) -> Result<Uuid> {
        {
            let cache = self.inner.rooms.lock().await;
            cache.current(credentials)?;
            if let Some(room) = cache.dosi {
                return Ok(room);
            }
        }
        let room = if let Some(room) = self.listed_dosi(credentials).await? {
            room
        } else {
            self.make_dosi(credentials).await?
        };
        let mut cache = self.inner.rooms.lock().await;
        cache.current(credentials)?;
        cache.dosi = Some(room);
        drop(cache);
        Ok(room)
    }

    async fn make_dosi(&self, credentials: &Credentials) -> Result<Uuid> {
        let id = Uuid::now_v7();
        let created = self
            .inner
            .http
            .device::<Value>(
                Method::POST,
                "api/missions",
                credentials,
                Some(json!({"id":id,"name":"Dosi","dosi":true})),
            )
            .await;
        match created {
            Ok(_) => Ok(id),
            Err(Error::Backend { status: 409, .. }) => self
                .listed_dosi(credentials)
                .await?
                .ok_or_else(|| invalid("Dosi's conversation is missing.")),
            Err(error) => Err(error),
        }
    }

    async fn dosi_kept<T>(&self, room: Uuid, result: Result<T>) -> Result<T> {
        if matches!(
            result,
            Err(Error::Backend {
                status: 403 | 404,
                ..
            })
        ) {
            self.forget_room(room).await;
        }
        result
    }

    pub(crate) async fn dosi_write(&self, record: &DosiRecord) -> Result<u64> {
        let credentials = self.dosi_credentials()?;
        let room = self.dosi_room(&credentials).await?;
        let payload = Payload::Dosi {
            record: record.clone(),
        };
        let written = self
            .room_write(&credentials, room, Uuid::now_v7(), "message", 0, &payload)
            .await;
        self.dosi_kept(room, written).await
    }

    pub(crate) async fn dosi_save(&self, instructions: SharedInstructions) -> Result<()> {
        let credentials = self.dosi_credentials()?;
        let room = self.dosi_room(&credentials).await?;
        let payload = Payload::DosiInstructions {
            text: instructions.text,
        };
        let saved = async {
            for _ in 0..3 {
                let page = self
                    .read_page(
                        &credentials,
                        room,
                        &format!("kind=task&limit={PAGE}&after=0"),
                    )
                    .await?;
                let version = page
                    .items
                    .iter()
                    .find(|item| item.id == room)
                    .map_or(0, |item| item.version);
                match self
                    .room_write(&credentials, room, room, "task", version, &payload)
                    .await
                {
                    Err(Error::Backend { status: 409, .. }) => {}
                    written => return written.map(|_| ()),
                }
            }
            Err(Error::Busy)
        }
        .await;
        self.dosi_kept(room, saved).await
    }

    pub(crate) async fn dosi_read(&self, after: u64) -> Result<DosiLog> {
        let credentials = self.dosi_credentials()?;
        let room = self.dosi_room(&credentials).await?;
        let read = self.read_dosi(&credentials, room, after).await;
        self.dosi_kept(room, read).await
    }

    async fn read_dosi(
        &self,
        credentials: &Credentials,
        room: Uuid,
        after: u64,
    ) -> Result<DosiLog> {
        self.refresh_room_keys(credentials, room, false, &mut Lookups::default())
            .await?;
        self.inner
            .rooms
            .lock()
            .await
            .writing_key(credentials, room)?;
        let mut log = DosiLog::default();
        let mut after = after;
        loop {
            let page = self
                .read_page(credentials, room, &format!("limit={PAGE}&after={after}"))
                .await?;
            if page.sequence < after {
                log.reset = true;
                return Ok(log);
            }
            self.inner
                .rooms
                .lock()
                .await
                .dosi_records(credentials, room, &page.items, &mut log)?;
            let last = page.items.last().map(|item| item.sequence);
            if !page.has_more {
                log.sequence = last.unwrap_or(after).max(page.sequence);
                return Ok(log);
            }
            after = last
                .filter(|last| *last > after)
                .ok_or_else(|| invalid("Room history did not advance."))?;
        }
    }
}

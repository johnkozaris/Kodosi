use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use uuid::Uuid;

use super::{
    BackendClient, CancellationToken, Credentials, Duration, Error, Result, invalid,
    rooms::Lookups,
    terminal_connections,
    wire::{Change, Notice, TerminalNotice},
};

const RETRY_LIMIT: Duration = Duration::from_mins(1);

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Scope {
    All,
    Rooms(BTreeSet<Uuid>),
}

#[derive(Debug, Default)]
pub(super) struct Waiting {
    surfaces: BTreeSet<&'static str>,
    missions: Option<Scope>,
    rooms: BTreeMap<Uuid, Option<u64>>,
    terminals: BTreeMap<Uuid, TerminalNotice>,
    hosts: Option<Vec<String>>,
}

impl Waiting {
    fn take(&mut self, notice: Notice) -> Result<()> {
        match notice {
            Notice::Changed(change) => self.add(&change),
            Notice::Terminal(terminal) => {
                if !self.surfaces.contains("sessions") {
                    self.terminals.insert(terminal.session, terminal);
                }
                Ok(())
            }
            Notice::Hosts(hosts) => {
                self.hosts = Some(hosts);
                Ok(())
            }
        }
    }

    pub(super) fn add(&mut self, change: &Change) -> Result<()> {
        match change.surface.as_str() {
            "devices" => self.surfaces.insert("devices"),
            "friends" => self.surfaces.insert("friends"),
            "sessions" => {
                self.terminals.clear();
                self.surfaces.insert("sessions")
            }
            "missions" => {
                match (change.room, &mut self.missions) {
                    (Some(room), Some(Scope::Rooms(rooms))) => {
                        rooms.insert(room);
                    }
                    (Some(room), missions @ None) => {
                        *missions = Some(Scope::Rooms(BTreeSet::from([room])));
                    }
                    (Some(_), Some(Scope::All)) => {}
                    (None, missions) => *missions = Some(Scope::All),
                }
                true
            }
            "rooms" => {
                let room = change
                    .room
                    .ok_or_else(|| invalid("A room change names its room."))?;
                let wanted = self.rooms.entry(room).or_insert(change.sequence);
                *wanted = wanted
                    .zip(change.sequence)
                    .map(|(known, new)| known.max(new));
                true
            }
            _ => return Err(invalid("Unsupported notification surface.")),
        };
        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.surfaces.is_empty()
            && self.missions.is_none()
            && self.rooms.is_empty()
            && self.terminals.is_empty()
            && self.hosts.is_none()
    }
}

fn retried(error: &Error) -> bool {
    error.unanswered() || matches!(error, Error::Backend { status: 409, .. })
}

impl BackendClient {
    pub(super) async fn start_notifications(&self) -> Result<()> {
        let credentials = self.credentials()?;
        if !credentials.enrolled {
            return Ok(());
        }
        self.register_room_key(&credentials).await?;
        let mut current = self.inner.notifications.lock().await;
        if current
            .as_ref()
            .is_some_and(|cancel| !cancel.is_cancelled())
        {
            return Ok(());
        }
        let cancel = credentials.cancel.child_token();
        *current = Some(cancel.clone());
        drop(current);
        let network = self.clone();
        tokio::spawn(async move {
            let mut delay = 1;
            let mut failure = crate::diagnostics::Repeats::new();
            loop {
                if cancel.is_cancelled()
                    || network.check_credentials(&credentials).is_err()
                    || !network.credentials().is_ok_and(|current| current.enrolled)
                {
                    break;
                }
                let started = tokio::time::Instant::now();
                let result = tokio::select! {
                    biased;
                    ()=cancel.cancelled()=>break,
                    ()=network.inner.shutdown.cancelled()=>break,
                    result=network.notification_connection(&credentials,&cancel)=>result,
                };
                if started.elapsed() >= terminal_connections::STABLE_CONNECTION {
                    delay = 1;
                    failure.worked();
                }
                if let Err(error) = result
                    && failure.new_failure(&error)
                {
                    tracing::warn!(%error, "notification stream interrupted");
                }
                tokio::select! {
                    ()=cancel.cancelled()=>break,
                    ()=network.inner.shutdown.cancelled()=>break,
                    ()=tokio::time::sleep(Duration::from_secs(delay))=>{},
                }
                delay = (delay * 2).min(15);
            }
            cancel.cancel();
        });
        Ok(())
    }

    async fn notification_connection(
        &self,
        admitted: &Credentials,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let credentials = self.credentials()?;
        self.check_credentials(admitted)?;
        let (sender, mut notices) = tokio::sync::mpsc::unbounded_channel::<Notice>();
        let refresh = async {
            let mut waiting = Waiting::default();
            let mut retry = None;
            let mut delay = Duration::from_secs(1);
            loop {
                if waiting.is_empty() {
                    let Some(notice) = notices.recv().await else {
                        return Ok(());
                    };
                    waiting.take(notice)?;
                }
                if let Some(at) = retry {
                    loop {
                        tokio::select! {
                            notice = notices.recv() => match notice {
                                Some(notice) => waiting.take(notice)?,
                                None => return Ok(()),
                            },
                            () = tokio::time::sleep_until(at) => break,
                        }
                        if let Some(hosts) = waiting.hosts.take() {
                            self.emit_hosts(admitted, hosts);
                        }
                    }
                }
                while let Ok(notice) = notices.try_recv() {
                    waiting.take(notice)?;
                }
                match self.refresh_waiting(admitted, &mut waiting).await {
                    Ok(()) => {
                        retry = None;
                        delay = Duration::from_secs(1);
                    }
                    Err(error) if retried(&error) => {
                        tracing::debug!(%error, "the refresh will be tried again");
                        retry = Some(tokio::time::Instant::now() + delay);
                        delay = (delay * 2).min(RETRY_LIMIT);
                    }
                    Err(error) => return Err(error),
                }
            }
        };
        let result = tokio::select! {
            biased;
            () = cancel.cancelled() => Ok(()),
            () = self.inner.shutdown.cancelled() => Ok(()),
            result = terminal_connections::run_link(self, &credentials, &sender) => result,
            result = refresh => result,
        };
        self.emit_hosts(admitted, Vec::new());
        if matches!(result, Err(Error::Backend { status: 403, .. })) {
            let mut waiting = Waiting::default();
            waiting.add(&Change::of("devices"))?;
            self.refresh_waiting(admitted, &mut waiting).await?;
        }
        if matches!(result, Err(Error::UpdateRequired)) {
            self.update_required();
        }
        result
    }

    pub(super) async fn refresh_waiting(
        &self,
        admitted: &Credentials,
        waiting: &mut Waiting,
    ) -> Result<()> {
        if let Some(hosts) = waiting.hosts.take() {
            self.emit_hosts(admitted, hosts);
        }
        let _operation = self.inner.operations.lock().await;
        self.check_credentials(admitted)?;
        let credentials = self.credentials()?;
        let emit =
            |event| self.emit_for(admitted.generation, Some(admitted.user_id.clone()), event);
        for surface in ["devices", "friends", "sessions"] {
            if !waiting.surfaces.contains(surface) {
                continue;
            }
            let events = if surface == "sessions" {
                vec![self.session_event().await?]
            } else if surface == "friends" {
                vec![self.friend_event().await?]
            } else if self.settle_enrollment().await? {
                self.device_events().await?
            } else {
                self.enrollment_events().await?
            };
            events.into_iter().for_each(emit);
            waiting.surfaces.remove(surface);
        }
        for notice in std::mem::take(&mut waiting.terminals).into_values() {
            emit(self.terminal_event(&notice));
        }
        if let Some(scope) = waiting.missions.take() {
            let catalog = match self.mission_list().await {
                Ok(catalog) => catalog,
                Err(error) => {
                    waiting.missions = Some(scope);
                    return Err(error);
                }
            };
            let (listed, complete) = Self::listed_rooms(&catalog)?;
            emit(catalog);
            let whole = scope == Scope::All;
            let scope = match scope {
                Scope::All => None,
                Scope::Rooms(rooms) => Some(rooms),
            };
            let mut pending = self.rooms_to_refresh(listed, scope.as_ref()).await;
            let mut lookups = Lookups::default();
            for room in pending.clone() {
                match self
                    .refresh_room(&credentials, room, None, true, &mut lookups)
                    .await
                {
                    Err(error) if retried(&error) => {
                        waiting.missions = Some(Scope::Rooms(pending));
                        return Err(error);
                    }
                    Err(error) => tracing::debug!(%error, "the room waits for its next change"),
                    Ok(event) => event.into_iter().for_each(emit),
                }
                pending.remove(&room);
                waiting.rooms.remove(&room);
            }
            if whole {
                if complete {
                    self.drop_recovery_room_key(&credentials).await;
                }
                waiting.rooms.clear();
            }
        }
        let rooms = std::mem::take(&mut waiting.rooms);
        let mut pending = rooms.clone();
        for (room, wanted) in rooms {
            match self
                .refresh_room(&credentials, room, wanted, false, &mut Lookups::default())
                .await
            {
                Err(error) if retried(&error) => {
                    waiting.rooms = pending;
                    return Err(error);
                }
                Err(error) => tracing::debug!(%error, "the room waits for its next change"),
                Ok(event) => event.into_iter().for_each(emit),
            }
            pending.remove(&room);
        }
        if self.credentials()?.enrolled {
            Ok(())
        } else {
            Err(Error::EnrollmentRequired)
        }
    }

    fn emit_hosts(&self, admitted: &Credentials, mut hosts: Vec<String>) {
        hosts.retain(|device| *device != admitted.keys.device_id);
        self.emit_for(
            admitted.generation,
            Some(admitted.user_id.clone()),
            json!({"type":"devices.hosts","deviceIds":hosts}),
        );
    }

    fn terminal_event(&self, notice: &TerminalNotice) -> Value {
        if let Ok(mut kept) = self.inner.sessions.lock()
            && let Some(dto) = kept
                .get_mut(&notice.session)
                .filter(|dto| dto.incarnation_id == notice.incarnation)
        {
            dto.host_online = notice.online;
            dto.status.clone_from(&notice.status);
        }
        json!({"type":"network.terminal","sessionId":notice.session,
            "incarnationId":notice.incarnation,"online":notice.online,"status":notice.status})
    }
}

#[cfg(test)]
mod tests;

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

/// When the device link connects again after it ended. The wait doubles up to 15 seconds. A
/// terminal that waits for the link ends a wait, but when that early try fails too, the next
/// wait runs in full, so that a link that keeps ending cannot connect again and again.
#[derive(Debug, PartialEq, Eq)]
struct Retry {
    delay: u64,
    hurried: bool,
}

impl Default for Retry {
    fn default() -> Self {
        Self {
            delay: 1,
            hurried: false,
        }
    }
}

impl Retry {
    const fn hurries(&self) -> bool {
        !self.hurried
    }

    fn waited(&mut self, ended: terminal_connections::Wait) {
        if matches!(
            ended,
            terminal_connections::Wait::Slept | terminal_connections::Wait::Asked
        ) {
            *self = Self::default();
        } else {
            self.hurried = ended == terminal_connections::Wait::Wanted;
            self.delay = (self.delay * 2).min(15);
        }
    }
}

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
    reported: BTreeMap<&'static str, String>,
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
                    .ok_or_else(|| invalid("A project change does not name its project."))?;
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

fn ends_connection(error: &Error) -> bool {
    matches!(
        error,
        Error::Stale
            | Error::SignedOut
            | Error::AccountDeleted
            | Error::AccountBanned
            | Error::EnrollmentRequired
            | Error::UpdateRequired
            | Error::ServerOutdated
            | Error::Backend {
                status: 401 | 403,
                ..
            }
    )
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
            let mut retry = Retry::default();
            let mut failure = crate::diagnostics::Repeats::new();
            loop {
                if cancel.is_cancelled()
                    || network.check_credentials(&credentials).is_err()
                    || !network.credentials().is_ok_and(|current| current.enrolled)
                {
                    break;
                }
                network.inner.link.start_wanting();
                let started = crate::network::Moment::now();
                let result = tokio::select! {
                    biased;
                    ()=cancel.cancelled()=>break,
                    ()=network.inner.shutdown.cancelled()=>break,
                    result=network.notification_connection(&credentials,&cancel)=>result,
                };
                if started.monotonic.elapsed() >= terminal_connections::STABLE_CONNECTION
                    || started.slept_until(crate::network::Moment::now())
                {
                    retry = Retry::default();
                    failure.worked();
                }
                if matches!(result, Err(Error::AccountBanned)) {
                    let _operation = network.inner.operations.lock().await;
                    if let Err(error) = network.sign_in_kept(result).await {
                        tracing::warn!(%error, "the banned account was not signed out");
                    }
                    break;
                }
                if let Err(error) = result
                    && failure.new_failure(&error)
                {
                    tracing::warn!(%error, "notification stream interrupted");
                }
                let ended = tokio::select! {
                    ()=cancel.cancelled()=>break,
                    ()=network.inner.shutdown.cancelled()=>break,
                    ended=network.inner.link.wait_to_reconnect(crate::network::stagger(Duration::from_secs(retry.delay)), retry.hurries())=>ended,
                };
                retry.waited(ended);
            }
            network.inner.link.stop_wanting();
            cancel.cancel();
        });
        Ok(())
    }

    pub(super) async fn notification_connection(
        &self,
        admitted: &Credentials,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let credentials = {
            let _operation = self.inner.operations.lock().await;
            self.renew_token().await?;
            self.credentials()?
        };
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
                        retry = Some(tokio::time::Instant::now() + crate::network::stagger(delay));
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
        let mut again = None;
        for surface in ["devices", "friends", "sessions"] {
            if !waiting.surfaces.contains(surface) {
                continue;
            }
            let refreshed = match surface {
                "sessions" => self.session_event().await.map(|event| vec![event]),
                "friends" => self.friend_event().await.map(|event| vec![event]),
                _ => match self.settle_enrollment().await {
                    Ok(true) => self.device_events().await,
                    Ok(false) => self.enrollment_events().await,
                    Err(error) => Err(error),
                },
            };
            match refreshed {
                Ok(events) => {
                    events.into_iter().for_each(emit);
                    waiting.reported.remove(surface);
                }
                Err(error) if retried(&error) => {
                    again.get_or_insert(error);
                    continue;
                }
                Err(error) => Self::surface_failed(waiting, surface, error)?,
            }
            waiting.surfaces.remove(surface);
        }
        for notice in std::mem::take(&mut waiting.terminals).into_values() {
            emit(self.terminal_event(&notice));
        }
        if let Some(scope) = waiting.missions.take() {
            match self
                .refresh_missions(admitted, &credentials, waiting, scope)
                .await
            {
                Ok(()) => {
                    waiting.reported.remove("missions");
                }
                Err(error) if retried(&error) => {
                    again.get_or_insert(error);
                }
                Err(error) => Self::surface_failed(waiting, "missions", error)?,
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
        if !self.credentials()?.enrolled {
            return Err(Error::EnrollmentRequired);
        }
        again.map_or(Ok(()), Err)
    }

    /// Takes the failure of one part of a refresh that the server answered. An error that ends
    /// the account's connection ends the refresh; any other failure waits for the next change of
    /// its part.
    fn surface_failed(waiting: &mut Waiting, surface: &'static str, error: Error) -> Result<()> {
        if ends_connection(&error) {
            return Err(error);
        }
        let reason = error.to_string();
        if waiting.reported.get(surface) != Some(&reason) {
            tracing::warn!(%error, surface, "the refresh waits for the next change");
            waiting.reported.insert(surface, reason);
        }
        Ok(())
    }

    async fn refresh_missions(
        &self,
        admitted: &Credentials,
        credentials: &Credentials,
        waiting: &mut Waiting,
        scope: Scope,
    ) -> Result<()> {
        let listed = self.mission_list().await.and_then(|catalog| {
            let listed = Self::listed_rooms(&catalog)?;
            Ok((catalog, listed))
        });
        let (catalog, (listed, complete)) = match listed {
            Ok(listed) => listed,
            Err(error) => {
                if retried(&error) {
                    waiting.missions = Some(scope);
                }
                return Err(error);
            }
        };
        let emit =
            |event| self.emit_for(admitted.generation, Some(admitted.user_id.clone()), event);
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
                .refresh_room(credentials, room, None, true, &mut lookups)
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
                self.drop_recovery_room_key(credentials).await;
            }
            waiting.rooms.clear();
        }
        Ok(())
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

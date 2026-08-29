use std::{
    collections::BTreeMap, env, io, net::IpAddr, path::PathBuf, process::Stdio, time::Duration,
};

use aws_lc_rs::digest::{SHA256, digest};
use futures_util::StreamExt;
use kodosi_domain::ids::SessionId;
use serde::Deserialize;
use serde_json::{Value, json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufWriter},
    process::Command,
};
use tokio_util::codec::{FramedRead, LinesCodec};
use uuid::Uuid;

use super::mailbox_store::{
    AgentRoomStore, MailboxDestination, MailboxEntry, MailboxOffer, MailboxOfferReservation,
    RoomTaskDeliveryReason,
};
use crate::{AppError, Result};

const MAX_INPUT_FRAME_BYTES: usize = 64 * 1024;
const MAX_OUTPUT_FRAME_BYTES: usize = 64 * 1024;
const MAX_TOOL_ARGUMENT_BYTES: usize = 32 * 1024;
const MAX_CHANNEL_CONTENT_BYTES: usize = 16 * 1024;
const MAX_TOOL_RESULT_BYTES: usize = 16 * 1024;
const MAX_CHILD_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_ID_BYTES: usize = 512;
const MAX_RESOURCE_ID_BYTES: usize = 256;
const MAX_ENDPOINT_BYTES: usize = 2_048;
const MAX_STATE_EVENT_BYTES: usize = 8 * 1024;
const MAX_FAILURE_DETAIL_BYTES: usize = 240;
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const CHILD_TIMEOUT: Duration =
    Duration::from_secs(crate::headless_host::ROOM_ACTION_WAIT_BOUND_SECS + 90);
const STATE_EVENT_TIMEOUT: Duration = Duration::from_secs(1);
const LATEST_PROTOCOL_VERSION: &str = "2025-11-25";
const SUPPORTED_PROTOCOL_VERSIONS: [&str; 4] =
    ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];

pub(crate) async fn run_server() -> Result<()> {
    let destination = current_mailbox_destination()?;
    let enabled = env::var_os("KODOSI_ROOM_CHANNEL_ENABLED").is_some_and(|value| value == "1");
    let store = if enabled {
        Some(AgentRoomStore::open().map_err(mailbox_error)?)
    } else {
        None
    };
    let reporter =
        StateReporter::from_environment().map_err(|reason| AppError::Unsupported { reason })?;
    let executable = env::current_exe()?;
    ChannelServer::new(destination, store, enabled, reporter, executable)
        .run_stdio()
        .await
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "independent protocol, readiness, and retry flags are not one state machine"
)]
struct ChannelServer {
    destination: MailboxDestination,
    store: Option<AgentRoomStore>,
    enabled: bool,
    offered: Option<MailboxOffer>,
    initialize_seen: bool,
    initialized: bool,
    ready_reported: bool,
    poll_failure_reported: bool,
    reporter: Option<StateReporter>,
    executable: PathBuf,
}

impl ChannelServer {
    fn new(
        destination: MailboxDestination,
        store: Option<AgentRoomStore>,
        enabled: bool,
        reporter: Option<StateReporter>,
        executable: PathBuf,
    ) -> Self {
        Self {
            destination,
            store,
            enabled,
            offered: None,
            initialize_seen: false,
            initialized: false,
            ready_reported: false,
            poll_failure_reported: false,
            reporter,
            executable,
        }
    }

    async fn run_stdio(mut self) -> Result<()> {
        let mut input = FramedRead::new(
            tokio::io::stdin(),
            LinesCodec::new_with_max_length(MAX_INPUT_FRAME_BYTES),
        );
        let mut output = BufWriter::new(tokio::io::stdout());
        let mut poll = tokio::time::interval(POLL_INTERVAL);
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                frame = input.next() => {
                    let Some(frame) = frame else {
                        output.flush().await?;
                        return Ok(());
                    };
                    let response = match frame {
                        Ok(line) => match serde_json::from_str::<Value>(&line) {
                            Ok(message) => self.handle_message(message).await,
                            Err(error) => {
                                tracing::warn!(%error, "rejected malformed room channel JSON");
                                Some(rpc_error(Value::Null, -32700, "Parse error"))
                            }
                        },
                        Err(error) => {
                            tracing::warn!(%error, "rejected oversized room channel frame");
                            Some(rpc_error(
                                Value::Null,
                                -32600,
                                "JSON-RPC frame exceeded the 64 KiB limit",
                            ))
                        }
                    };
                    if let Some(response) = response {
                        write_frame(&mut output, &response).await?;
                    }
                }
                _ = poll.tick(), if self.initialized && self.enabled && self.offered.is_none() => {
                    match self.poll_next_offer() {
                        Ok(Some(notification)) => {
                            if let Err(error) = write_frame(&mut output, &notification).await {
                                self.report_delivery_failure(
                                    self.offered.as_ref().map(|offer| offer.event_id.as_str()),
                                    &format!("could not write channel notification: {error}"),
                                ).await;
                                return Err(AppError::Io(error));
                            }
                            self.poll_failure_reported = false;
                            if let Some(offer) = self.offered.as_ref() {
                                self.report_state(
                                    "kodosi.room.offered",
                                    [
                                        ("eventId", offer.event_id.clone()),
                                        ("cursor", offer.cursor.to_string()),
                                    ],
                                ).await;
                            }
                        }
                        Ok(None) => {
                            self.poll_failure_reported = false;
                        }
                        Err(error) => {
                            tracing::warn!(%error, "room channel mailbox poll failed");
                            if !self.poll_failure_reported {
                                self.report_delivery_failure(None, &error).await;
                                self.poll_failure_reported = true;
                            }
                        }
                    }
                }
            }
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one exhaustive JSON-RPC dispatcher prevents silent method fallthrough"
    )]
    async fn handle_message(&mut self, message: Value) -> Option<Value> {
        let Some(object) = message.as_object() else {
            return Some(rpc_error(Value::Null, -32600, "Invalid Request"));
        };
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Some(rpc_error(Value::Null, -32600, "Invalid Request"));
        }
        let has_id = object.contains_key("id");
        let id = object.get("id").cloned().unwrap_or(Value::Null);
        if has_id && !valid_request_id(&id) {
            return Some(rpc_error(Value::Null, -32600, "Invalid Request"));
        }
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            return Some(rpc_error(id, -32600, "Invalid Request"));
        };
        if method.len() > 128 {
            return Some(rpc_error(id, -32600, "Invalid Request"));
        }
        let params = object.get("params").cloned().unwrap_or_else(|| json!({}));

        match method {
            "initialize" => {
                if !has_id {
                    return Some(rpc_error(Value::Null, -32600, "initialize requires an id"));
                }
                if self.initialize_seen {
                    return Some(rpc_error(id, -32600, "initialize was already completed"));
                }
                match initialize_result(&params) {
                    Ok(result) => {
                        self.initialize_seen = true;
                        Some(rpc_result(id, result))
                    }
                    Err(message) => Some(rpc_error(id, -32602, &message)),
                }
            }
            "notifications/initialized" => {
                if has_id {
                    return Some(rpc_error(
                        id,
                        -32600,
                        "notifications/initialized must not contain an id",
                    ));
                }
                if !self.initialize_seen {
                    tracing::warn!(
                        "ignored room channel initialized notification before initialize"
                    );
                    return None;
                }
                self.initialized = true;
                if !self.ready_reported {
                    self.report_state(
                        "kodosi.channel.ready",
                        [
                            ("sessionId", self.destination.session_id.to_string()),
                            (
                                "sessionIncarnationId",
                                self.destination.incarnation_id.to_string(),
                            ),
                            ("enabled", self.enabled.to_string()),
                        ],
                    )
                    .await;
                    self.ready_reported = true;
                }
                None
            }
            "ping" => {
                if has_id {
                    Some(rpc_result(id, json!({})))
                } else {
                    None
                }
            }
            "tools/list" => {
                if !has_id {
                    return None;
                }
                if !self.initialized {
                    return Some(rpc_error(id, -32002, "Server is not initialized"));
                }
                Some(rpc_result(id, tools_list_result()))
            }
            "tools/call" => {
                if !has_id {
                    return None;
                }
                if !self.initialized {
                    return Some(rpc_error(id, -32002, "Server is not initialized"));
                }
                match self.call_tool(params).await {
                    Ok(result) => Some(rpc_result(id, result)),
                    Err(ToolCallError::Invalid(message)) => Some(rpc_error(
                        id,
                        -32602,
                        &bounded(&message, MAX_FAILURE_DETAIL_BYTES),
                    )),
                    Err(ToolCallError::Execution(message)) => {
                        self.report_delivery_failure(
                            self.offered.as_ref().map(|offer| offer.event_id.as_str()),
                            &message,
                        )
                        .await;
                        Some(rpc_result(id, tool_error_result(&message)))
                    }
                }
            }
            _ if has_id => Some(rpc_error(id, -32601, "Method not found")),
            _ => {
                tracing::debug!(method, "ignored unknown room channel notification");
                None
            }
        }
    }

    fn poll_next_offer(&mut self) -> std::result::Result<Option<Value>, String> {
        if !self.enabled || self.offered.is_some() {
            return Ok(None);
        }
        let store = self
            .store
            .as_ref()
            .ok_or_else(|| "room channel mailbox store is unavailable".to_owned())?;
        let Some(offer) = store
            .peek_unread_offer(self.destination)
            .map_err(|error| format!("could not read the room mailbox: {error}"))?
        else {
            return Ok(None);
        };
        validate_event_id(&offer.event_id)?;
        if offer.cursor <= offer.previous_cursor {
            return Err("room mailbox offered a non-advancing cursor".to_owned());
        }
        let notification = offer_notification(&offer)?;
        self.offered = Some(offer);
        Ok(Some(notification))
    }

    #[expect(
        clippy::too_many_lines,
        reason = "tool validation and cursor commit remain visibly adjacent"
    )]
    async fn call_tool(&mut self, params: Value) -> std::result::Result<Value, ToolCallError> {
        if serde_json::to_vec(&params)
            .map_err(|error| ToolCallError::Invalid(error.to_string()))?
            .len()
            > MAX_TOOL_ARGUMENT_BYTES
        {
            return Err(ToolCallError::Invalid(
                "tool arguments exceeded the 32 KiB limit".to_owned(),
            ));
        }
        let call: ToolCall = serde_json::from_value(params)
            .map_err(|error| ToolCallError::Invalid(error.to_string()))?;
        if call.name.len() > 128 {
            return Err(ToolCallError::Invalid("tool name is too long".to_owned()));
        }

        match call.name.as_str() {
            "kodosi_room_acknowledge" => {
                let args: OfferedEventArgs = decode_arguments(call.arguments)?;
                let offer = self.bound_offer(&args.event_id, &args.cursor)?;
                let reservation = self.reserve_offer(&offer)?;
                self.commit_reservation(reservation)?;
                self.offered = None;
                self.report_action(&offer, "acknowledge").await;
                Ok(tool_success_result(
                    "acknowledge",
                    &offer,
                    "Acknowledged the offered Kodosi room event.",
                ))
            }
            "kodosi_room_reply" => {
                let args: ReplyArgs = decode_arguments(call.arguments)?;
                let offer = self.bound_offer(&args.event_id, &args.cursor)?;
                validate_resource_id("room_id", &args.room_id)?;
                validate_body("body", &args.body, crate::host_protocol::CHAT_BODY_MAX_LEN)?;
                if let Some(target) = args.to_session_id.as_deref() {
                    SessionId::parse_field(target, "to_session_id")
                        .map_err(|error| ToolCallError::Invalid(error.to_string()))?;
                }
                if let Some(offered_room_id) = offered_room_id(&offer.entry)
                    && offered_room_id != args.room_id
                {
                    return Err(ToolCallError::Invalid(
                        "room_id does not match the offered event".to_owned(),
                    ));
                }
                let mut command = vec![
                    "--json".to_owned(),
                    "room".to_owned(),
                    "chat".to_owned(),
                    "post".to_owned(),
                    args.room_id,
                    args.body,
                    "--request-id".to_owned(),
                    offer_action_request_id(&offer).to_string(),
                ];
                if let Some(target) = args.to_session_id {
                    command.push("--to-session".to_owned());
                    command.push(target);
                }
                let reservation = self.reserve_offer(&offer)?;
                let output = self.invoke_kodosi(&command).await?;
                self.commit_reservation(reservation)?;
                self.offered = None;
                self.report_action(&offer, "reply").await;
                Ok(tool_success_result("reply", &offer, &output))
            }
            "kodosi_room_task_transition" => {
                let args: TaskTransitionArgs = decode_arguments(call.arguments)?;
                let offer = self.bound_offer(&args.event_id, &args.cursor)?;
                validate_resource_id("room_id", &args.room_id)?;
                validate_resource_id("task_id", &args.task_id)?;
                let transition = TaskTransition::parse(&args.transition)?;
                validate_task_transition_offer(&offer.entry, &args)?;
                let result = match (transition, args.result) {
                    (TaskTransition::Done, Some(result)) => {
                        validate_body(
                            "result",
                            &result,
                            crate::host_protocol::TASK_RESULT_MAX_LEN,
                        )?;
                        Some(result)
                    }
                    (TaskTransition::Done, None) => {
                        return Err(ToolCallError::Invalid(
                            "result is required for the done transition".to_owned(),
                        ));
                    }
                    (_, Some(_)) => {
                        return Err(ToolCallError::Invalid(
                            "result is accepted only for the done transition".to_owned(),
                        ));
                    }
                    (_, None) => None,
                };
                let request_id = offer_action_request_id(&offer).to_string();
                let command = task_transition_command(
                    transition,
                    &args.room_id,
                    &args.task_id,
                    args.expected_revision,
                    result,
                    &request_id,
                );
                let reservation = self.reserve_offer(&offer)?;
                let output = self.invoke_kodosi(&command).await?;
                self.commit_reservation(reservation)?;
                self.offered = None;
                self.report_action(&offer, transition.as_cli_arg()).await;
                Ok(tool_success_result(
                    transition.as_cli_arg(),
                    &offer,
                    &output,
                ))
            }
            _ => Err(ToolCallError::Invalid(format!(
                "unknown room channel tool `{}`",
                call.name
            ))),
        }
    }

    fn bound_offer(
        &self,
        event_id: &str,
        cursor: &str,
    ) -> std::result::Result<MailboxOffer, ToolCallError> {
        validate_event_id(event_id).map_err(ToolCallError::Invalid)?;
        let parsed_cursor = parse_cursor(cursor).map_err(ToolCallError::Invalid)?;
        let offer = self.offered.as_ref().ok_or_else(|| {
            ToolCallError::Invalid("there is no currently offered room event".to_owned())
        })?;
        if event_id != offer.event_id || cursor != offer.cursor.to_string() {
            return Err(ToolCallError::Invalid(
                "event_id and cursor must exactly match the current offer".to_owned(),
            ));
        }
        if parsed_cursor != offer.cursor {
            return Err(ToolCallError::Invalid(
                "cursor does not match the current offer".to_owned(),
            ));
        }
        Ok(offer.clone())
    }

    fn reserve_offer(
        &mut self,
        offer: &MailboxOffer,
    ) -> std::result::Result<MailboxOfferReservation, ToolCallError> {
        let result = self
            .store
            .as_ref()
            .ok_or_else(|| {
                ToolCallError::Execution("room channel mailbox store is unavailable".to_owned())
            })?
            .reserve_offer(self.destination, offer);
        match result {
            Ok(reservation) => Ok(reservation),
            Err(error) if error.kind() == io::ErrorKind::InvalidInput => {
                self.offered = None;
                Err(ToolCallError::Invalid(format!(
                    "the offered room event is no longer current: {error}"
                )))
            }
            Err(error) => Err(ToolCallError::Execution(format!(
                "could not reserve the exact offered mailbox cursor: {error}"
            ))),
        }
    }

    fn commit_reservation(
        &mut self,
        reservation: MailboxOfferReservation,
    ) -> std::result::Result<(), ToolCallError> {
        match reservation.commit() {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::InvalidInput => {
                self.offered = None;
                Err(ToolCallError::Invalid(format!(
                    "the offered room event was superseded before its cursor could be committed: {error}"
                )))
            }
            Err(error) => Err(ToolCallError::Execution(format!(
                "could not commit the exact offered mailbox cursor: {error}"
            ))),
        }
    }

    #[expect(
        clippy::single_match_else,
        reason = "the timeout branch performs required child termination before returning"
    )]
    async fn invoke_kodosi(
        &self,
        arguments: &[String],
    ) -> std::result::Result<String, ToolCallError> {
        if arguments.len() > 12
            || arguments
                .iter()
                .any(|argument| argument.len() > MAX_CHANNEL_CONTENT_BYTES)
        {
            return Err(ToolCallError::Invalid(
                "Kodosi command arguments exceeded their bounds".to_owned(),
            ));
        }
        let mut child = Command::new(&self.executable)
            .args(arguments)
            .env("KODOSI_SESSION_ID", self.destination.session_id.to_string())
            .env(
                "KODOSI_SESSION_INCARNATION_ID",
                self.destination.incarnation_id.to_string(),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| {
                ToolCallError::Execution(format!(
                    "could not start the Kodosi room command: {error}"
                ))
            })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            ToolCallError::Execution("Kodosi room command stdout was unavailable".to_owned())
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            ToolCallError::Execution("Kodosi room command stderr was unavailable".to_owned())
        })?;
        let execution = async {
            let (status, stdout, stderr) = tokio::join!(
                child.wait(),
                read_bounded(stdout, MAX_CHILD_OUTPUT_BYTES),
                read_bounded(stderr, MAX_CHILD_OUTPUT_BYTES),
            );
            let status =
                status.map_err(|error| format!("could not wait for room command: {error}"))?;
            let stdout =
                stdout.map_err(|error| format!("could not read room command output: {error}"))?;
            let stderr = stderr
                .map_err(|error| format!("could not read room command diagnostics: {error}"))?;
            Ok::<_, String>((status, stdout, stderr))
        };
        let (status, stdout, stderr) = match tokio::time::timeout(CHILD_TIMEOUT, execution).await {
            Ok(result) => result.map_err(ToolCallError::Execution)?,
            Err(_) => {
                if let Err(error) = child.kill().await
                    && error.kind() != io::ErrorKind::InvalidInput
                {
                    tracing::warn!(%error, "could not kill timed-out Kodosi room command");
                }
                drop(child.wait().await);
                return Err(ToolCallError::Execution(
                    "Kodosi room command timed out".to_owned(),
                ));
            }
        };
        let stdout_text = stdout.text();
        let stderr_text = stderr.text();
        if !status.success() {
            let detail = if stderr_text.trim().is_empty() {
                stdout_text
            } else {
                stderr_text
            };
            return Err(ToolCallError::Execution(format!(
                "Kodosi room command failed with status {}: {}",
                status,
                bounded(detail.trim(), MAX_FAILURE_DETAIL_BYTES)
            )));
        }
        let mut result = if stdout_text.trim().is_empty() {
            "Kodosi room command succeeded.".to_owned()
        } else {
            bounded(stdout_text.trim(), MAX_TOOL_RESULT_BYTES)
        };
        if stdout.truncated {
            result.push_str("\n[command output truncated]");
        }
        Ok(bounded(&result, MAX_TOOL_RESULT_BYTES))
    }

    async fn report_action(&self, offer: &MailboxOffer, action: &str) {
        self.report_state(
            "kodosi.room.acted",
            [
                ("eventId", offer.event_id.clone()),
                ("cursor", offer.cursor.to_string()),
                ("action", action.to_owned()),
            ],
        )
        .await;
    }

    async fn report_delivery_failure(&self, event_id: Option<&str>, message: &str) {
        let mut data = BTreeMap::from([(
            "message".to_owned(),
            bounded(message, MAX_FAILURE_DETAIL_BYTES),
        )]);
        if let Some(event_id) = event_id {
            data.insert("eventId".to_owned(), bounded(event_id, MAX_ID_BYTES));
        }
        self.report_state_map("kodosi.room.delivery_failed", data)
            .await;
    }

    async fn report_state<const N: usize>(&self, event_type: &str, fields: [(&str, String); N]) {
        self.report_state_map(
            event_type,
            fields
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
        .await;
    }

    async fn report_state_map(&self, event_type: &str, data: BTreeMap<String, String>) {
        let Some(reporter) = &self.reporter else {
            return;
        };
        if let Err(error) = reporter.post(event_type, data).await {
            tracing::debug!(%error, event_type, "could not post room channel state event");
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolCall {
    name: String,
    arguments: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OfferedEventArgs {
    event_id: String,
    cursor: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplyArgs {
    event_id: String,
    cursor: String,
    room_id: String,
    body: String,
    #[serde(default)]
    to_session_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskTransitionArgs {
    event_id: String,
    cursor: String,
    room_id: String,
    task_id: String,
    expected_revision: i64,
    transition: String,
    #[serde(default)]
    result: Option<String>,
}

#[derive(Clone, Copy)]
enum TaskTransition {
    Claim,
    Submit,
    Done,
    Archive,
    Reopen,
}

impl TaskTransition {
    fn parse(value: &str) -> std::result::Result<Self, ToolCallError> {
        match value {
            "claim" => Ok(Self::Claim),
            "submit" => Ok(Self::Submit),
            "done" => Ok(Self::Done),
            "archive" => Ok(Self::Archive),
            "reopen" => Ok(Self::Reopen),
            _ => Err(ToolCallError::Invalid(
                "transition must be claim, submit, done, archive, or reopen".to_owned(),
            )),
        }
    }

    const fn as_cli_arg(self) -> &'static str {
        match self {
            Self::Claim => "claim",
            Self::Submit => "submit",
            Self::Done => "done",
            Self::Archive => "archive",
            Self::Reopen => "reopen",
        }
    }
}

fn validate_task_transition_offer(
    entry: &MailboxEntry,
    args: &TaskTransitionArgs,
) -> std::result::Result<(), ToolCallError> {
    if args.expected_revision < 0 {
        return Err(ToolCallError::Invalid(
            "expected_revision must be nonnegative".to_owned(),
        ));
    }
    match entry {
        MailboxEntry::RoomTask {
            room_id,
            task_id,
            revision,
            assigned_session_id: Some(_),
            assigned_session_incarnation_id: Some(_),
            delivery_reason: RoomTaskDeliveryReason::CurrentAssignmentOrUpdate,
            ..
        } if room_id == &args.room_id
            && task_id == &args.task_id
            && revision == &args.expected_revision =>
        {
            Ok(())
        }
        MailboxEntry::RoomTask {
            room_id,
            task_id,
            revision,
            ..
        } if room_id != &args.room_id
            || task_id != &args.task_id
            || revision != &args.expected_revision =>
        {
            Err(ToolCallError::Invalid(
                "room_id, task_id, and expected_revision must exactly match the offered task"
                    .to_owned(),
            ))
        }
        MailboxEntry::RoomTask {
            delivery_reason, ..
        } if *delivery_reason != RoomTaskDeliveryReason::CurrentAssignmentOrUpdate => {
            Err(ToolCallError::Invalid(
                "the offered task was reassigned or unassigned and is not actionable by this session"
                    .to_owned(),
            ))
        }
        MailboxEntry::RoomTask { .. } => Err(ToolCallError::Invalid(
            "the offered task has no complete current assignment identity".to_owned(),
        )),
        _ => Err(ToolCallError::Invalid(
            "the offered event is not a room task".to_owned(),
        )),
    }
}

fn task_transition_command(
    transition: TaskTransition,
    room_id: &str,
    task_id: &str,
    expected_revision: i64,
    result: Option<String>,
    request_id: &str,
) -> Vec<String> {
    let mut command = vec![
        "--json".to_owned(),
        "room".to_owned(),
        "tasks".to_owned(),
        transition.as_cli_arg().to_owned(),
        room_id.to_owned(),
        task_id.to_owned(),
        "--expected-revision".to_owned(),
        expected_revision.to_string(),
        "--request-id".to_owned(),
        request_id.to_owned(),
    ];
    if let Some(result) = result {
        command.push(result);
    }
    command
}

fn offer_action_request_id(offer: &MailboxOffer) -> Uuid {
    let at = match &offer.entry {
        MailboxEntry::AgentMessage { at, .. }
        | MailboxEntry::RoomChat { at, .. }
        | MailboxEntry::RoomTask { at, .. } => *at,
    };
    let timestamp_ms = u64::try_from(at.unix_timestamp_nanos().div_euclid(1_000_000))
        .unwrap_or(0)
        .min(0xffff_ffff_ffff);
    let preimage = format!(
        "kodosi-room-action-v1\0{}\0{}\0{}",
        offer.destination.session_id, offer.destination.incarnation_id, offer.event_id
    );
    let hash = digest(&SHA256, preimage.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes[..6].copy_from_slice(&timestamp_ms.to_be_bytes()[2..]);
    bytes[6..].copy_from_slice(&hash.as_ref()[..10]);
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

#[derive(Debug)]
enum ToolCallError {
    Invalid(String),
    Execution(String),
}

struct CapturedOutput {
    bytes: Vec<u8>,
    truncated: bool,
}

impl CapturedOutput {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

async fn read_bounded<R>(mut reader: R, limit: usize) -> io::Result<CapturedOutput>
where
    R: AsyncRead + Unpin,
{
    let mut bytes = Vec::with_capacity(limit.min(4096));
    let mut truncated = false;
    let mut chunk = [0_u8; 4096];
    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        bytes.extend_from_slice(&chunk[..read.min(remaining)]);
        truncated |= read > remaining;
    }
    Ok(CapturedOutput { bytes, truncated })
}

#[derive(Clone)]
struct StateReporter {
    client: reqwest::Client,
    endpoint: reqwest::Url,
}

impl StateReporter {
    fn from_environment() -> std::result::Result<Option<Self>, String> {
        let raw = match env::var("KODOSI_ROOM_EVENTS_ENDPOINT") {
            Ok(raw) => raw,
            Err(env::VarError::NotPresent) => return Ok(None),
            Err(env::VarError::NotUnicode(_)) => {
                return Err("KODOSI_ROOM_EVENTS_ENDPOINT is not valid Unicode".to_owned());
            }
        };
        if raw.is_empty() || raw.len() > MAX_ENDPOINT_BYTES {
            return Err("KODOSI_ROOM_EVENTS_ENDPOINT has an invalid length".to_owned());
        }
        let endpoint = reqwest::Url::parse(&raw)
            .map_err(|error| format!("KODOSI_ROOM_EVENTS_ENDPOINT is invalid: {error}"))?;
        if endpoint.scheme() != "http"
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(
                "KODOSI_ROOM_EVENTS_ENDPOINT must be a credential-free loopback HTTP URL"
                    .to_owned(),
            );
        }
        let host = endpoint
            .host_str()
            .ok_or_else(|| "KODOSI_ROOM_EVENTS_ENDPOINT has no host".to_owned())?;
        let address = host.parse::<IpAddr>().map_err(|_| {
            "KODOSI_ROOM_EVENTS_ENDPOINT must use a literal loopback IP address".to_owned()
        })?;
        if !address.is_loopback() {
            return Err("KODOSI_ROOM_EVENTS_ENDPOINT must use a loopback IP address".to_owned());
        }
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(250))
            .timeout(STATE_EVENT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|error| format!("could not build the room state HTTP client: {error}"))?;
        Ok(Some(Self { client, endpoint }))
    }

    async fn post(
        &self,
        event_type: &str,
        data: BTreeMap<String, String>,
    ) -> std::result::Result<(), String> {
        let timestamp = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|error| format!("could not format room state timestamp: {error}"))?;
        let body = serde_json::to_vec(&json!({
            "type": event_type,
            "timestamp": timestamp,
            "data": data,
        }))
        .map_err(|error| format!("could not encode room state event: {error}"))?;
        if body.len() > MAX_STATE_EVENT_BYTES {
            return Err("room state event exceeded its byte limit".to_owned());
        }
        let response = self
            .client
            .post(self.endpoint.clone())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(|_| "room state event POST failed".to_owned())?;
        if !response.status().is_success() {
            return Err(format!(
                "room state event endpoint returned HTTP {}",
                response.status()
            ));
        }
        Ok(())
    }
}

fn current_mailbox_destination() -> Result<MailboxDestination> {
    let raw = env::var("KODOSI_SESSION_ID").map_err(|error| AppError::Unsupported {
        reason: format!(
            "KODOSI_SESSION_ID is required; the room channel server must run inside a Kodosi session: {error}"
        ),
    })?;
    let session_id = SessionId::parse_field(&raw, "KODOSI_SESSION_ID").map_err(|error| {
        AppError::Unsupported {
            reason: format!("KODOSI_SESSION_ID is malformed: {error}"),
        }
    })?;
    let raw = env::var("KODOSI_SESSION_INCARNATION_ID").map_err(|error| {
        AppError::Unsupported {
            reason: format!(
                "KODOSI_SESSION_INCARNATION_ID is required; the room channel server must run inside a Kodosi session incarnation: {error}"
            ),
        }
    })?;
    let incarnation_id = Uuid::parse_str(&raw).map_err(|error| AppError::Unsupported {
        reason: format!("KODOSI_SESSION_INCARNATION_ID is malformed: {error}"),
    })?;
    Ok(MailboxDestination::new(session_id, incarnation_id))
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "map_err supplies ownership and AppError owns the formatted diagnostic"
)]
fn mailbox_error(error: io::Error) -> AppError {
    AppError::Unsupported {
        reason: format!("room channel mailbox I/O error: {error}"),
    }
}

fn valid_request_id(id: &Value) -> bool {
    match id {
        Value::String(value) => !value.is_empty() && value.len() <= MAX_ID_BYTES,
        Value::Number(_) | Value::Null => true,
        _ => false,
    }
}

fn initialize_result(params: &Value) -> std::result::Result<Value, String> {
    let requested = params
        .as_object()
        .and_then(|object| object.get("protocolVersion"))
        .and_then(Value::as_str)
        .ok_or_else(|| "initialize.params.protocolVersion must be a string".to_owned())?;
    if requested.is_empty() || requested.len() > 32 {
        return Err("initialize protocolVersion is invalid".to_owned());
    }
    let protocol_version = if SUPPORTED_PROTOCOL_VERSIONS.contains(&requested) {
        requested
    } else {
        LATEST_PROTOCOL_VERSION
    };
    Ok(json!({
        "protocolVersion": protocol_version,
        "serverInfo": {
            "name": "kodosi-room-channel",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "instructions": "One durable Kodosi room event is offered at a time. Use the acknowledgement, reply, or task-transition tool with the exact event_id and cursor. A notification write is not an acknowledgement.",
        "capabilities": {
            "experimental": {
                "claude/channel": {}
            },
            "tools": {}
        }
    }))
}

fn tools_list_result() -> Value {
    json!({
        "tools": [
            {
                "name": "kodosi_room_acknowledge",
                "description": "Acknowledge exactly the currently offered Kodosi room event without posting a reply.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "event_id": {"type": "string", "minLength": 1, "maxLength": MAX_ID_BYTES},
                        "cursor": {"type": "string", "pattern": "^[0-9]{1,20}$"}
                    },
                    "required": ["event_id", "cursor"]
                }
            },
            {
                "name": "kodosi_room_reply",
                "description": "Post a room reply through Kodosi's authorized room API, then acknowledge the exact offered event.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "event_id": {"type": "string", "minLength": 1, "maxLength": MAX_ID_BYTES},
                        "cursor": {"type": "string", "pattern": "^[0-9]{1,20}$"},
                        "room_id": {"type": "string", "minLength": 1, "maxLength": MAX_RESOURCE_ID_BYTES},
                        "body": {"type": "string", "minLength": 1, "maxLength": crate::host_protocol::CHAT_BODY_MAX_LEN},
                        "to_session_id": {"type": "string", "minLength": 1, "maxLength": MAX_RESOURCE_ID_BYTES}
                    },
                    "required": ["event_id", "cursor", "room_id", "body"]
                }
            },
            {
                "name": "kodosi_room_task_transition",
                "description": "Transition the currently offered room task through Kodosi's authorized room API, then acknowledge it.",
                "inputSchema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "event_id": {"type": "string", "minLength": 1, "maxLength": MAX_ID_BYTES},
                        "cursor": {"type": "string", "pattern": "^[0-9]{1,20}$"},
                        "room_id": {"type": "string", "minLength": 1, "maxLength": MAX_RESOURCE_ID_BYTES},
                        "task_id": {"type": "string", "minLength": 1, "maxLength": MAX_RESOURCE_ID_BYTES},
                        "expected_revision": {"type": "integer", "minimum": 0},
                        "transition": {"type": "string", "enum": ["claim", "submit", "done", "archive", "reopen"]},
                        "result": {"type": "string", "minLength": 1, "maxLength": crate::host_protocol::TASK_RESULT_MAX_LEN}
                    },
                    "required": ["event_id", "cursor", "room_id", "task_id", "expected_revision", "transition"]
                }
            }
        ]
    })
}

#[expect(
    clippy::too_many_lines,
    reason = "all mailbox variants share one bounded Channel notification renderer"
)]
fn offer_notification(offer: &MailboxOffer) -> std::result::Result<Value, String> {
    let (kind, content) = match &offer.entry {
        MailboxEntry::AgentMessage {
            from_session_id,
            from_description,
            body,
            in_reply_to,
            ..
        } => (
            "agent_message",
            [
                format!("[Kodosi agent message {}]", offer.event_id),
                format!("From session: {from_session_id}"),
                from_description
                    .as_deref()
                    .map(|description| format!("Sender: {description}"))
                    .unwrap_or_default(),
                in_reply_to
                    .as_deref()
                    .map(|reply| format!("In reply to: {reply}"))
                    .unwrap_or_default(),
                body.clone(),
                "Use kodosi_room_acknowledge with the exact event_id and cursor after acting."
                    .to_owned(),
            ]
            .into_iter()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        ),
        MailboxEntry::RoomChat {
            room_id,
            room_name,
            author_user_id,
            author_session_id,
            body,
            seq,
            ..
        } => (
            "room_chat",
            [
                format!("[Kodosi room message {}]", offer.event_id),
                format!("Room: {}", room_name.as_deref().unwrap_or(room_id)),
                format!("Room ID: {room_id}"),
                format!("Sequence: {seq}"),
                format!("Author user: {author_user_id}"),
                author_session_id
                    .map(|session| format!("Author session: {session}"))
                    .unwrap_or_default(),
                body.clone(),
                "Use kodosi_room_reply or kodosi_room_acknowledge with the exact event_id and cursor."
                    .to_owned(),
            ]
            .into_iter()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        ),
        MailboxEntry::RoomTask {
            room_id,
            room_name,
            task_id,
            title,
            status,
            revision,
            assigned_session_id,
            assigned_session_incarnation_id,
            delivery_reason,
            ..
        } => (
            "room_task",
            [
                format!("[Kodosi room task {}]", offer.event_id),
                format!("Room: {}", room_name.as_deref().unwrap_or(room_id)),
                format!("Room ID: {room_id}"),
                format!("Task ID: {task_id}"),
                format!("Status: {status}"),
                format!("Expected revision: {revision}"),
                format!("Delivery reason: {delivery_reason:?}"),
                assigned_session_id
                    .map_or_else(
                        || "Assigned session: none".to_owned(),
                        |session| format!("Assigned session: {session}"),
                    ),
                assigned_session_incarnation_id
                    .map_or_else(
                        || "Assigned incarnation: none".to_owned(),
                        |incarnation| format!("Assigned incarnation: {incarnation}"),
                    ),
                format!("Title: {title}"),
                if *delivery_reason == RoomTaskDeliveryReason::CurrentAssignmentOrUpdate
                    && assigned_session_id.is_some()
                    && assigned_session_incarnation_id.is_some()
                {
                    "Use kodosi_room_task_transition with this exact expected_revision, or kodosi_room_acknowledge, using the exact event_id and cursor.".to_owned()
                } else {
                    "This task is no longer assigned to this session. Acknowledge it; do not transition it from this delivery.".to_owned()
                },
            ]
            .join("\n"),
        ),
    };
    let content = bounded(&content, MAX_CHANNEL_CONTENT_BYTES);
    let mut meta = serde_json::Map::from_iter([
        ("event_id".to_owned(), Value::String(offer.event_id.clone())),
        ("cursor".to_owned(), Value::String(offer.cursor.to_string())),
        (
            "session_id".to_owned(),
            Value::String(offer.destination.session_id.to_string()),
        ),
        (
            "session_incarnation_id".to_owned(),
            Value::String(offer.destination.incarnation_id.to_string()),
        ),
        ("kind".to_owned(), Value::String(kind.to_owned())),
    ]);
    match &offer.entry {
        MailboxEntry::RoomChat { room_id, .. } => {
            meta.insert("room_id".to_owned(), Value::String(room_id.clone()));
        }
        MailboxEntry::RoomTask {
            room_id,
            task_id,
            revision,
            delivery_reason,
            ..
        } => {
            meta.insert("room_id".to_owned(), Value::String(room_id.clone()));
            meta.insert("task_id".to_owned(), Value::String(task_id.clone()));
            meta.insert(
                "expected_revision".to_owned(),
                Value::String(revision.to_string()),
            );
            meta.insert(
                "delivery_reason".to_owned(),
                Value::String(format!("{delivery_reason:?}")),
            );
        }
        MailboxEntry::AgentMessage { .. } => {}
    }
    if meta.values().any(|value| !value.is_string()) {
        return Err("room channel metadata must contain only strings".to_owned());
    }
    Ok(json!({
        "jsonrpc": "2.0",
        "method": "notifications/claude/channel",
        "params": {
            "content": content,
            "meta": meta,
        }
    }))
}

fn offered_room_id(entry: &MailboxEntry) -> Option<&str> {
    match entry {
        MailboxEntry::RoomChat { room_id, .. } | MailboxEntry::RoomTask { room_id, .. } => {
            Some(room_id)
        }
        MailboxEntry::AgentMessage { .. } => None,
    }
}

fn decode_arguments<T: for<'de> Deserialize<'de>>(
    arguments: Value,
) -> std::result::Result<T, ToolCallError> {
    serde_json::from_value(arguments).map_err(|error| ToolCallError::Invalid(error.to_string()))
}

fn validate_event_id(value: &str) -> std::result::Result<(), String> {
    if value.is_empty()
        || value.len() > MAX_ID_BYTES
        || value.chars().any(char::is_control)
        || value.chars().any(char::is_whitespace)
    {
        return Err("event_id is malformed".to_owned());
    }
    Ok(())
}

fn parse_cursor(value: &str) -> std::result::Result<u64, String> {
    if value.is_empty() || value.len() > 20 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("cursor is malformed".to_owned());
    }
    value
        .parse::<u64>()
        .map_err(|_| "cursor is malformed".to_owned())
}

fn validate_resource_id(field: &str, value: &str) -> std::result::Result<(), ToolCallError> {
    if value.is_empty()
        || value.len() > MAX_RESOURCE_ID_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(ToolCallError::Invalid(format!("{field} is malformed")));
    }
    Ok(())
}

fn validate_body(
    field: &str,
    value: &str,
    max_utf16_units: usize,
) -> std::result::Result<(), ToolCallError> {
    if value.is_empty()
        || value.len() > MAX_CHANNEL_CONTENT_BYTES
        || value.encode_utf16().count() > max_utf16_units
        || value.contains('\0')
    {
        return Err(ToolCallError::Invalid(format!(
            "{field} is empty or exceeds its bound"
        )));
    }
    Ok(())
}

fn tool_success_result(action: &str, offer: &MailboxOffer, text: &str) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": bounded(text, MAX_TOOL_RESULT_BYTES),
        }],
        "structuredContent": {
            "action": action,
            "event_id": offer.event_id,
            "cursor": offer.cursor.to_string(),
            "committed": true,
        },
        "isError": false,
    })
}

fn tool_error_result(message: &str) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": bounded(message, MAX_FAILURE_DETAIL_BYTES),
        }],
        "isError": true,
    })
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "JSON-RPC response construction consumes caller-owned values conceptually"
)]
fn rpc_result(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "JSON-RPC response construction consumes the request identifier conceptually"
)]
fn rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": bounded(message, MAX_FAILURE_DETAIL_BYTES),
        }
    })
}

async fn write_frame<W>(writer: &mut W, value: &Value) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let frame = serde_json::to_vec(value).map_err(io::Error::other)?;
    if frame.len() > MAX_OUTPUT_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "room channel output frame exceeded the 64 KiB limit",
        ));
    }
    writer.write_all(&frame).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await
}

fn bounded(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    value[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodosi_domain::ids::UserId;
    use time::OffsetDateTime;

    fn room_chat(room_id: &str, seq: i64) -> MailboxEntry {
        MailboxEntry::RoomChat {
            room_id: room_id.to_owned(),
            room_name: Some("Engineering".to_owned()),
            author_user_id: UserId::try_from("00000000000000000000000000000001").expect("user id"),
            author_session_id: None,
            recipient_session_ids: Vec::new(),
            recipient_user_ids: Vec::new(),
            body: format!("message-{seq}"),
            seq,
            at: OffsetDateTime::now_utc(),
        }
    }

    #[test]
    fn initialize_advertises_claude_channel_capability() {
        let result = initialize_result(&json!({"protocolVersion": "2025-06-18"}))
            .expect("initialize result");
        assert_eq!(result["protocolVersion"], "2025-06-18");
        assert_eq!(
            result["capabilities"]["experimental"]["claude/channel"],
            json!({})
        );
        assert_eq!(result["capabilities"]["tools"], json!({}));
    }

    fn actionable_task_entry(revision: i64) -> MailboxEntry {
        MailboxEntry::RoomTask {
            room_id: "room-a".to_owned(),
            room_name: Some("Engineering".to_owned()),
            task_id: "task-1".to_owned(),
            title: "Ship it".to_owned(),
            status: "Open".to_owned(),
            revision,
            assigned_session_id: Some(SessionId::new()),
            assigned_session_incarnation_id: Some(Uuid::now_v7()),
            delivery_reason: RoomTaskDeliveryReason::CurrentAssignmentOrUpdate,
            at: OffsetDateTime::now_utc(),
        }
    }

    fn transition_args(expected_revision: i64) -> TaskTransitionArgs {
        TaskTransitionArgs {
            event_id: "event-1".to_owned(),
            cursor: "1".to_owned(),
            room_id: "room-a".to_owned(),
            task_id: "task-1".to_owned(),
            expected_revision,
            transition: "claim".to_owned(),
            result: None,
        }
    }

    #[test]
    fn task_action_requires_the_exact_offered_revision_and_current_assignment() {
        let entry = actionable_task_entry(17);
        validate_task_transition_offer(&entry, &transition_args(17))
            .expect("exact offered revision should be actionable");
        assert!(matches!(
            validate_task_transition_offer(&entry, &transition_args(18)),
            Err(ToolCallError::Invalid(message)) if message.contains("expected_revision")
        ));

        let mut reassigned = actionable_task_entry(17);
        let MailboxEntry::RoomTask {
            delivery_reason, ..
        } = &mut reassigned
        else {
            unreachable!();
        };
        *delivery_reason = RoomTaskDeliveryReason::ReassignedPreviousAssignee;
        assert!(matches!(
            validate_task_transition_offer(&reassigned, &transition_args(17)),
            Err(ToolCallError::Invalid(message)) if message.contains("reassigned")
        ));
    }

    #[test]
    fn task_transition_command_preserves_revision_and_stable_request_identity() {
        assert_eq!(
            task_transition_command(
                TaskTransition::Done,
                "room-a",
                "task-1",
                17,
                Some("finished".to_owned()),
                "01900000-0000-7000-8000-000000000001",
            ),
            [
                "--json",
                "room",
                "tasks",
                "done",
                "room-a",
                "task-1",
                "--expected-revision",
                "17",
                "--request-id",
                "01900000-0000-7000-8000-000000000001",
                "finished",
            ]
        );
    }

    #[test]
    fn offered_event_has_a_deterministic_uuid_v7_action_identity() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = AgentRoomStore::open_at(directory.path().to_path_buf()).expect("mailbox store");
        let destination = MailboxDestination::new(SessionId::new(), Uuid::now_v7());
        store
            .enqueue_for(destination, &room_chat("room-a", 1))
            .expect("enqueue");
        let offer = store
            .peek_unread_offer(destination)
            .expect("peek")
            .expect("offer");

        let first = offer_action_request_id(&offer);
        assert_eq!(first, offer_action_request_id(&offer));
        assert_eq!(first.get_version_num(), 7);
    }

    #[test]
    fn disabled_channel_does_not_read_or_advance_mailbox() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = AgentRoomStore::open_at(directory.path().to_path_buf()).expect("mailbox store");
        let destination = MailboxDestination::new(SessionId::new(), Uuid::now_v7());
        store
            .enqueue_for(
                destination,
                &MailboxEntry::AgentMessage {
                    request_id: "request-offline".to_owned(),
                    from_session_id: SessionId::new(),
                    from_description: None,
                    body: "still durable".to_owned(),
                    in_reply_to: None,
                    at: OffsetDateTime::now_utc(),
                },
            )
            .expect("enqueue");
        let mut server = ChannelServer::new(
            destination,
            Some(store.clone()),
            false,
            None,
            PathBuf::from("kodosi"),
        );

        assert!(server.poll_next_offer().expect("disabled poll").is_none());
        assert_eq!(
            store
                .peek_unread_offer(destination)
                .expect("peek")
                .expect("durable event")
                .event_id,
            "request-offline"
        );
    }

    #[tokio::test]
    async fn acknowledge_requires_and_commits_the_exact_offer() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = AgentRoomStore::open_at(directory.path().to_path_buf()).expect("mailbox store");
        let destination = MailboxDestination::new(SessionId::new(), Uuid::now_v7());
        store
            .enqueue_for(
                destination,
                &MailboxEntry::AgentMessage {
                    request_id: "request-exact".to_owned(),
                    from_session_id: SessionId::new(),
                    from_description: None,
                    body: "act once".to_owned(),
                    in_reply_to: None,
                    at: OffsetDateTime::now_utc(),
                },
            )
            .expect("enqueue");
        let mut server = ChannelServer::new(
            destination,
            Some(store.clone()),
            true,
            None,
            PathBuf::from("kodosi"),
        );
        let notification = server
            .poll_next_offer()
            .expect("poll")
            .expect("notification");
        let event_id = notification["params"]["meta"]["event_id"]
            .as_str()
            .expect("event id");
        let cursor = notification["params"]["meta"]["cursor"]
            .as_str()
            .expect("cursor");
        assert_eq!(
            notification["params"]["meta"]["session_incarnation_id"],
            destination.incarnation_id.to_string()
        );
        assert!(
            notification["params"]["meta"]
                .as_object()
                .expect("meta")
                .values()
                .all(Value::is_string)
        );

        let mismatch = server
            .call_tool(json!({
                "name": "kodosi_room_acknowledge",
                "arguments": {
                    "event_id": event_id,
                    "cursor": format!("0{cursor}"),
                }
            }))
            .await;
        assert!(matches!(mismatch, Err(ToolCallError::Invalid(_))));
        assert!(
            store
                .peek_unread_offer(destination)
                .expect("peek after mismatch")
                .is_some()
        );

        let result = server
            .call_tool(json!({
                "name": "kodosi_room_acknowledge",
                "arguments": {
                    "event_id": event_id,
                    "cursor": cursor,
                }
            }))
            .await
            .expect("exact acknowledgement");
        assert_eq!(result["isError"], false);
        assert!(
            store
                .peek_unread_offer(destination)
                .expect("peek after acknowledgement")
                .is_none()
        );
    }

    #[tokio::test]
    async fn externally_consumed_offer_is_rejected_before_action_and_next_event_is_offered() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = AgentRoomStore::open_at(directory.path().to_path_buf()).expect("mailbox store");
        let destination = MailboxDestination::new(SessionId::new(), Uuid::now_v7());
        store
            .enqueue_for(destination, &room_chat("room-a", 1))
            .expect("enqueue first event");
        store
            .enqueue_for(destination, &room_chat("room-a", 2))
            .expect("enqueue second event");
        let mut server = ChannelServer::new(
            destination,
            Some(store.clone()),
            true,
            None,
            PathBuf::from("/definitely/not/a/kodosi/executable"),
        );
        let first = server
            .poll_next_offer()
            .expect("poll")
            .expect("first notification");
        let event_id = first["params"]["meta"]["event_id"]
            .as_str()
            .expect("event id");
        let cursor = first["params"]["meta"]["cursor"].as_str().expect("cursor");

        store
            .set_cursor(destination, cursor.parse().expect("cursor number"))
            .expect("manual consumer advances cursor");

        let result = server
            .call_tool(json!({
                "name": "kodosi_room_reply",
                "arguments": {
                    "event_id": event_id,
                    "cursor": cursor,
                    "room_id": "room-a",
                    "body": "handled"
                }
            }))
            .await;
        assert!(matches!(result, Err(ToolCallError::Invalid(_))));
        assert!(server.offered.is_none());

        let second = server
            .poll_next_offer()
            .expect("poll after stale offer")
            .expect("second notification");
        assert_eq!(second["params"]["meta"]["event_id"], "chat:room-a:2");
    }

    #[tokio::test]
    async fn stale_channel_cannot_read_or_commit_replacement_incarnation_offer() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = AgentRoomStore::open_at(directory.path().to_path_buf()).expect("mailbox store");
        let session_id = SessionId::new();
        let stale = MailboxDestination::new(session_id, Uuid::now_v7());
        let replacement = MailboxDestination::new(session_id, Uuid::now_v7());
        store
            .enqueue_for(
                replacement,
                &MailboxEntry::AgentMessage {
                    request_id: "request-replacement".to_owned(),
                    from_session_id: SessionId::new(),
                    from_description: None,
                    body: "replacement work".to_owned(),
                    in_reply_to: None,
                    at: OffsetDateTime::now_utc(),
                },
            )
            .expect("enqueue replacement offer");
        let replacement_offer = store
            .peek_unread_offer(replacement)
            .expect("peek replacement")
            .expect("replacement offer");
        let mut stale_server = ChannelServer::new(
            stale,
            Some(store.clone()),
            true,
            None,
            PathBuf::from("kodosi"),
        );

        assert!(
            stale_server
                .poll_next_offer()
                .expect("stale channel poll")
                .is_none()
        );
        stale_server.offered = Some(replacement_offer.clone());
        let result = stale_server
            .call_tool(json!({
                "name": "kodosi_room_acknowledge",
                "arguments": {
                    "event_id": replacement_offer.event_id,
                    "cursor": replacement_offer.cursor.to_string(),
                }
            }))
            .await;
        assert!(matches!(result, Err(ToolCallError::Invalid(_))));
        assert!(stale_server.offered.is_none());
        assert!(
            store
                .peek_unread_offer(replacement)
                .expect("replacement remains unread")
                .is_some()
        );
    }
}

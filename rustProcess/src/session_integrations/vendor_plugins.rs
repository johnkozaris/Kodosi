use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};

use kodosi_domain::ids::SessionId;

const PLUGIN_MANIFEST: &str = r#"{
  "name": "kodosi-session",
  "description": "Session-scoped Kodosi integration",
  "version": "1.0.0",
  "extensions": {
    "paths": ["./extensions"],
    "exclusive": true
  },
  "skills": ["skills/kodosi-room"]
}
"#;

const CLAUDE_PLUGIN_MANIFEST: &str = r#"{
  "name": "kodosi-room",
  "description": "Session-scoped Kodosi room tools and guidance",
  "version": "1.0.0"
}
"#;

const EXTENSION_TEMPLATE: &str = r#"import { execFile } from "node:child_process";
import { joinSession } from "@github/copilot-sdk/extension";

const endpoint = __KODOSI_ENDPOINT__;
const permissionEndpoint = __KODOSI_PERMISSION_ENDPOINT__;
const kodosi = __KODOSI_CLI__;
const MAX_TEXT = 240;
const MAX_PERMISSION_BYTES = 512 * 1024;
const MAX_ROOM_BODY = 12 * 1024;

function short(value) {
  return typeof value === "string" ? value.slice(0, MAX_TEXT) : undefined;
}

function identifier(value) {
  return typeof value === "string" && /^[A-Za-z0-9._:-]{1,240}$/.test(value)
    ? value
    : undefined;
}

function category(value, allowed) {
  if (typeof value !== "string") return "other";
  const normalized = value.toLowerCase();
  return allowed.includes(normalized) ? normalized : "other";
}

function toolCategory(value) {
  const normalized = typeof value === "string" ? value.toLowerCase() : "";
  if (["shell", "bash", "powershell"].includes(normalized)) return "shell";
  if (["read", "view"].includes(normalized)) return "read";
  if (["write", "edit", "create"].includes(normalized)) return "write";
  if (["search", "grep", "glob"].includes(normalized)) return "search";
  if (normalized === "task") return "task";
  if (["web", "webfetch", "websearch"].includes(normalized)) return "web";
  if (["mcp", "kodosi_room_reply", "kodosi_room_task_transition"].includes(normalized)) {
    return "integration";
  }
  return "other";
}

function errorCategory(value) {
  const normalized = typeof value === "string" ? value.toLowerCase() : "";
  if (["authentication", "auth", "unauthorized"].includes(normalized)) return "authentication";
  if (["rate-limit", "rate_limit", "ratelimit"].includes(normalized)) return "rate-limit";
  if (normalized === "quota") return "quota";
  if (["context-limit", "context_limit", "contextoverflow"].includes(normalized)) {
    return "context-limit";
  }
  if (["network", "timeout", "connection"].includes(normalized)) return "network";
  return "other";
}

function safeEvent(event) {
  const data = event?.data ?? {};
  const common = { type: event?.type ?? "unknown", timestamp: new Date().toISOString(), data: {} };
  switch (common.type) {
    case "assistant.turn_start":
    case "assistant.message":
      break;
    case "tool.execution_start":
      common.data = {
        toolCategory: toolCategory(data.toolName),
        toolCallId: identifier(data.toolCallId),
      };
      break;
    case "tool.execution_complete":
      common.data = {
        toolCategory: toolCategory(data.toolName),
        toolCallId: identifier(data.toolCallId),
        success: data.success === true,
        errorCategory: data.success === true ? undefined : "tool-execution-failed",
      };
      break;
    case "permission.requested":
      common.data = {
        kind: category(data.permissionRequest?.kind, ["shell", "write", "read", "mcp", "url"]),
      };
      break;
    case "session.idle":
      common.data = { aborted: data.aborted === true };
      break;
    case "session.error":
      common.data = {
        errorCategory: errorCategory(data.errorType),
      };
      break;
    case "session.shutdown":
      common.data = {
        shutdownCategory: category(data.shutdownType, ["completed", "cancelled", "error"]),
      };
      break;
    case "user.message":
      common.data = {
        sourceCategory: category(data.source, ["user", "system", "extension"]),
      };
      break;
    default:
      return null;
  }
  return common;
}

async function publish(event) {
  try {
    await fetch(endpoint, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(event),
    });
  } catch {}
}

async function decidePermission(request) {
  try {
    const requestId =
      short(request?.toolCallId) ??
      `copilot-permission-${Date.now()}-${Math.random().toString(16).slice(2)}`;
    const body = JSON.stringify({ requestId, permissionRequest: request });
    if (body.length > MAX_PERMISSION_BYTES) return { kind: "no-result" };
    const response = await fetch(permissionEndpoint, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body,
    });
    if (!response.ok) return { kind: "no-result" };
    const decision = await response.json();
    if (
      decision?.kind === "approve-once" ||
      decision?.kind === "reject" ||
      decision?.kind === "no-result"
    ) {
      return decision;
    }
  } catch {}
  return { kind: "no-result" };
}

function runKodosi(args) {
  return new Promise((resolve, reject) => {
    execFile(kodosi, args, { maxBuffer: 1024 * 1024 }, (error, stdout, stderr) => {
      if (error) {
        reject(new Error(short(stderr) ?? short(error.message) ?? "Kodosi command failed"));
      } else {
        resolve(stdout);
      }
    });
  });
}

async function runKodosiJson(args) {
  return JSON.parse(await runKodosi(["--json", ...args]));
}

function roomEventId(entry) {
  switch (entry?.kind) {
    case "agentMessage":
      return identifier(entry.request_id) ? `message:${entry.request_id}` : null;
    case "roomChat":
      return identifier(entry.room_id) && Number.isSafeInteger(entry.seq) && entry.seq >= 0
        ? `chat:${entry.room_id}:${entry.seq}`
        : null;
    case "roomTask":
      return identifier(entry.room_id) &&
        identifier(entry.task_id) &&
        Number.isSafeInteger(entry.revision) &&
        entry.revision >= 0
        ? `task:${entry.room_id}:${entry.task_id}:${entry.revision}`
        : null;
    default:
      return null;
  }
}

function boundedBody(value) {
  const body = typeof value === "string" ? value : "";
  return body.length > MAX_ROOM_BODY
    ? `${body.slice(0, MAX_ROOM_BODY)}\n[message truncated by the session adapter]`
    : body;
}

function roomPrompt(entry, eventId) {
  switch (entry.kind) {
    case "agentMessage":
      if (
        !identifier(entry.request_id) ||
        !identifier(entry.from_session_id) ||
        typeof entry.body !== "string"
      ) return null;
      return [
        `[Kodosi agent message ${eventId}]`,
        `From session: ${entry.from_session_id}`,
        entry.from_description ? `Sender: ${entry.from_description}` : null,
        boundedBody(entry.body),
        "Reply explicitly with the kodosi_room_reply tool when a response is needed.",
      ].filter(Boolean).join("\n");
    case "roomChat":
      if (
        !identifier(entry.room_id) ||
        !Number.isSafeInteger(entry.seq) ||
        entry.seq < 0 ||
        typeof entry.body !== "string"
      ) return null;
      return [
        `[Kodosi room message ${eventId}]`,
        `Room: ${entry.room_name ?? entry.room_id}`,
        `Room ID: ${entry.room_id}`,
        `Sequence: ${entry.seq}`,
        boundedBody(entry.body),
        "Use kodosi_room_reply for an explicit room reply.",
      ].join("\n");
    case "roomTask":
      if (
        !identifier(entry.room_id) ||
        !identifier(entry.task_id) ||
        typeof entry.status !== "string" ||
        typeof entry.title !== "string" ||
        !Number.isSafeInteger(entry.revision) ||
        entry.revision < 0 ||
        ![
          "currentAssignmentOrUpdate",
          "reassignedPreviousAssignee",
          "unassignedPreviousAssignee",
        ].includes(entry.delivery_reason)
      ) return null;
      const actionable =
        entry.delivery_reason === "currentAssignmentOrUpdate" &&
        identifier(entry.assigned_session_id) &&
        identifier(entry.assigned_session_incarnation_id);
      return [
        `[Kodosi room task ${eventId}]`,
        `Room: ${entry.room_name ?? entry.room_id}`,
        `Room ID: ${entry.room_id}`,
        `Task ID: ${entry.task_id}`,
        `Status: ${entry.status}`,
        `Expected revision: ${entry.revision}`,
        `Delivery reason: ${entry.delivery_reason}`,
        `Title: ${boundedBody(entry.title)}`,
        actionable
          ? "Use kodosi_room_task_transition with this exact expectedRevision to claim, submit, complete, archive, or reopen it."
          : "This task is no longer assigned to this session. Do not transition it from this delivery.",
      ].join("\n");
    default:
      return null;
  }
}

const taskOffers = new Map();

function taskOfferKey(roomId, taskId, expectedRevision) {
  return `${roomId}\n${taskId}\n${expectedRevision}`;
}

function rememberTaskDelivery(entry) {
  for (const [key, offer] of taskOffers) {
    if (offer.roomId === entry.room_id && offer.taskId === entry.task_id) {
      taskOffers.delete(key);
    }
  }
  if (
    entry.delivery_reason !== "currentAssignmentOrUpdate" ||
    !identifier(entry.assigned_session_id) ||
    !identifier(entry.assigned_session_incarnation_id)
  ) return;
  const offer = {
    roomId: entry.room_id,
    taskId: entry.task_id,
    expectedRevision: entry.revision,
  };
  taskOffers.set(
    taskOfferKey(offer.roomId, offer.taskId, offer.expectedRevision),
    offer,
  );
  while (taskOffers.size > 128) {
    taskOffers.delete(taskOffers.keys().next().value);
  }
}

const roomTools = [
  {
    name: "kodosi_room_reply",
    description: "Post an explicit reply to a Kodosi room.",
    parameters: {
      type: "object",
      properties: {
        roomId: { type: "string", description: "Room ID from the delivered event" },
        body: { type: "string", description: "Reply body" },
        toSessionId: { type: "string", description: "Optional target session ID" },
      },
      required: ["roomId", "body"],
    },
    handler: async (args) => {
      const command = ["room", "chat", "post", args.roomId, args.body];
      if (args.toSessionId) command.push("--to-session", args.toSessionId);
      const result = await runKodosi(command);
      void publish({
        type: "kodosi.room.acted",
        timestamp: new Date().toISOString(),
        data: { roomId: args.roomId, action: "reply" },
      });
      return result;
    },
  },
  {
    name: "kodosi_room_task_transition",
    description: "Transition a Kodosi room task through the normal authorized room API.",
    parameters: {
      type: "object",
      properties: {
        roomId: { type: "string" },
        taskId: { type: "string" },
        transition: {
          type: "string",
          enum: ["claim", "submit", "done", "archive", "reopen"],
        },
        expectedRevision: {
          type: "integer",
          minimum: 0,
          description: "Exact revision from the delivered task",
        },
        result: { type: "string", description: "Required when transition is done" },
      },
      required: ["roomId", "taskId", "expectedRevision", "transition"],
    },
    handler: async (args) => {
      if (
        !identifier(args.roomId) ||
        !identifier(args.taskId) ||
        !Number.isSafeInteger(args.expectedRevision) ||
        args.expectedRevision < 0
      ) return "The room, task, or expected revision is malformed.";
      const offerKey = taskOfferKey(args.roomId, args.taskId, args.expectedRevision);
      if (!taskOffers.has(offerKey)) {
        return "The room, task, and expected revision do not match a current task offered to this session.";
      }
      const command = [
        "room",
        "tasks",
        args.transition,
        args.roomId,
        args.taskId,
        "--expected-revision",
        String(args.expectedRevision),
      ];
      if (args.transition === "done") {
        if (!args.result) return "A result is required when completing a task.";
        command.push(args.result);
      }
      const result = await runKodosi(command);
      taskOffers.delete(offerKey);
      void publish({
        type: "kodosi.room.acted",
        timestamp: new Date().toISOString(),
        data: {
          roomId: args.roomId,
          taskId: args.taskId,
          action: args.transition,
        },
      });
      return result;
    },
  },
];

const session = await joinSession({
  onPermissionRequest: decidePermission,
  tools: roomTools,
  onEvent: (event) => {
    const safe = safeEvent(event);
    if (safe) void publish(safe);
  },
});
void publish({
  type: "kodosi.extension.ready",
  timestamp: new Date().toISOString(),
  data: {},
});

let deliveryActive = false;
let deliveryFailure = false;
async function deliverRoomEvent() {
  if (deliveryActive) return;
  deliveryActive = true;
  try {
    const page = await runKodosiJson(["msg", "inbox", "--peek", "--limit", "1"]);
    const entry = Array.isArray(page.entries) ? page.entries[0] : null;
    if (!entry) {
      deliveryFailure = false;
      return;
    }
    const cursor =
      Number.isSafeInteger(page.cursor) && page.cursor >= 0 ? String(page.cursor) : null;
    const eventId = roomEventId(entry);
    const prompt = eventId ? roomPrompt(entry, eventId) : null;
    if (!eventId || !prompt || !cursor) return;
    void publish({
      type: "kodosi.room.offered",
      timestamp: new Date().toISOString(),
      data: { eventId, cursor },
    });
    await session.send({
      prompt,
      mode: "enqueue",
      displayPrompt: `[Kodosi room event ${eventId}]`,
    });
    if (entry.kind === "roomTask") rememberTaskDelivery(entry);
    await runKodosiJson(["msg", "ack", cursor]);
    void publish({
      type: "kodosi.room.accepted",
      timestamp: new Date().toISOString(),
      data: { eventId, cursor },
    });
    deliveryFailure = false;
  } catch {
    if (!deliveryFailure) {
      deliveryFailure = true;
      void publish({
        type: "kodosi.room.delivery_failed",
        timestamp: new Date().toISOString(),
        data: { errorCategory: "room-delivery-failed" },
      });
    }
  } finally {
    deliveryActive = false;
  }
}

const deliveryTimer = setInterval(() => void deliverRoomEvent(), 1500);
deliveryTimer.unref();
void deliverRoomEvent();
"#;

pub(crate) struct SessionIntegration {
    root: PathBuf,
    environment: Vec<(String, String)>,
}

impl SessionIntegration {
    pub(crate) fn environment(&self) -> &[(String, String)] {
        &self.environment
    }

    pub(crate) fn provider_executable(
        &self,
        provider: kodosi_domain::provider_conversation::ProviderConversationProvider,
    ) -> Option<PathBuf> {
        let executable = self.root.join("bin").join(provider.executable());
        executable.is_file().then_some(executable)
    }
}

impl Drop for SessionIntegration {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.root)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::debug!(
                path = %self.root.display(),
                %error,
                "could not remove session integration directory"
            );
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "one materialization pass keeps per-session ownership and cleanup paths aligned"
)]
pub(crate) fn prepare(
    session_id: SessionId,
    registration: &crate::agent_intel::telemetry::SessionRegistration,
) -> Result<Option<SessionIntegration>, String> {
    let copilot = find_executable("copilot");
    let claude = find_executable("claude");
    if copilot.is_none() && claude.is_none() {
        return Ok(None);
    }
    let endpoint = registration.extension_endpoint();
    let permission_endpoint = registration.extension_permission_endpoint();
    let root = crate::support::storage::paths::data_root()
        .map_err(|error| error.to_string())?
        .join("session-integrations")
        .join(session_id.to_string())
        .join(registration.local_incarnation_id().to_string());
    if let Ok(metadata) = std::fs::symlink_metadata(&root) {
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "refusing symlinked session integration root: {}",
                root.display()
            ));
        }
        std::fs::remove_dir_all(&root)
            .map_err(|error| format!("could not reset session integration: {error}"))?;
    }
    let bin = root.join("bin");
    create_private_dir(&bin)?;
    let kodosi = std::env::current_exe()
        .map_err(|error| format!("could not resolve the Kodosi executable: {error}"))?;

    if let Some(copilot) = copilot {
        let plugin = root.join("copilot-plugin");
        let extension = plugin.join("extensions").join("kodosi");
        let skill = plugin.join("skills").join("kodosi-room");
        create_private_dir(&extension)?;
        create_private_dir(&skill)?;
        write_private(
            &plugin.join("plugin.json"),
            PLUGIN_MANIFEST.as_bytes(),
            false,
        )?;
        let endpoint_literal = serde_json::to_string(&endpoint)
            .map_err(|error| format!("could not encode extension endpoint: {error}"))?;
        let permission_endpoint_literal = serde_json::to_string(&permission_endpoint)
            .map_err(|error| format!("could not encode permission endpoint: {error}"))?;
        let kodosi_literal = serde_json::to_string(&kodosi.to_string_lossy())
            .map_err(|error| format!("could not encode the Kodosi executable path: {error}"))?;
        let source = EXTENSION_TEMPLATE
            .replace("__KODOSI_ENDPOINT__", &endpoint_literal)
            .replace(
                "__KODOSI_PERMISSION_ENDPOINT__",
                &permission_endpoint_literal,
            )
            .replace("__KODOSI_CLI__", &kodosi_literal);
        write_private(&extension.join("extension.mjs"), source.as_bytes(), false)?;
        write_private(
            &skill.join("SKILL.md"),
            include_bytes!("../../skills/kodosi-room/SKILL.md"),
            false,
        )?;
        let wrapper = copilot_wrapper(&copilot, &plugin);
        write_private(&bin.join("copilot"), wrapper.as_bytes(), true)?;
    }

    if let Some(claude) = claude {
        let plugin = root.join("claude-plugin");
        let skill = plugin.join("skills").join("kodosi-room");
        let manifest = plugin.join(".claude-plugin");
        create_private_dir(&skill)?;
        create_private_dir(&manifest)?;
        write_private(
            &manifest.join("plugin.json"),
            CLAUDE_PLUGIN_MANIFEST.as_bytes(),
            false,
        )?;
        write_private(
            &skill.join("SKILL.md"),
            include_bytes!("../../skills/kodosi-room/SKILL.md"),
            false,
        )?;
        let development_channel = std::env::var_os("KODOSI_CLAUDE_DEVELOPMENT_CHANNELS").as_deref()
            == Some(std::ffi::OsStr::new("1"));
        let channel_entry = std::env::var("KODOSI_CLAUDE_CHANNEL_ENTRY")
            .ok()
            .filter(|entry| !entry.trim().is_empty());
        let channel_enabled = development_channel || channel_entry.is_some();
        let mcp_config = root.join("claude-room-mcp.json");
        let config = serde_json::to_vec_pretty(&serde_json::json!({
            "mcpServers": {
                "kodosi-room": {
                    "command": kodosi.to_string_lossy(),
                    "args": ["__internal-room-channel-serve"],
                    "env": {
                        "KODOSI_ROOM_CHANNEL_ENABLED": if channel_enabled { "1" } else { "0" },
                        "KODOSI_ROOM_EVENTS_ENDPOINT": endpoint,
                    }
                }
            }
        }))
        .map_err(|error| format!("could not encode Claude room MCP config: {error}"))?;
        write_private(&mcp_config, &config, false)?;
        let channel_arguments = if development_channel {
            format!(
                " --mcp-config {} --dangerously-load-development-channels server:kodosi-room",
                shell_quote(&mcp_config.to_string_lossy())
            )
        } else if let Some(channel_entry) = channel_entry {
            format!(
                " --mcp-config {} --channels {}",
                shell_quote(&mcp_config.to_string_lossy()),
                shell_quote(&channel_entry)
            )
        } else {
            String::new()
        };
        let wrapper = claude_wrapper(
            &claude,
            &plugin,
            &channel_arguments,
            &registration.claude_telemetry_environment(),
        );
        write_private(&bin.join("claude"), wrapper.as_bytes(), true)?;
    }
    Ok(Some(SessionIntegration {
        root,
        environment: session_shell_environment(&bin),
    }))
}

fn create_private_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path)
        .map_err(|error| format!("could not create {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("could not protect {}: {error}", path.display()))?;
    }
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8], executable: bool) -> Result<(), String> {
    std::fs::write(path, bytes)
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if executable { 0o700 } else { 0o600 };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .map_err(|error| format!("could not protect {}: {error}", path.display()))?;
    }
    Ok(())
}

fn find_executable(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| candidate.canonicalize().ok())
}

pub(crate) fn installed_provider_executable(
    provider: kodosi_domain::provider_conversation::ProviderConversationProvider,
) -> Option<PathBuf> {
    find_executable(provider.executable())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn session_shell_environment(bin: &Path) -> Vec<(String, String)> {
    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let path = if inherited_path.is_empty() {
        bin.to_string_lossy().into_owned()
    } else {
        format!("{}:{inherited_path}", bin.display())
    };
    vec![("PATH".to_owned(), path)]
}

fn copilot_wrapper(copilot: &Path, plugin: &Path) -> String {
    format!(
        "#!/bin/sh\nCOPILOT_PLUGIN_DIR_ONLY=true exec {} --experimental --plugin-dir {} \"$@\"\n",
        shell_quote(&copilot.to_string_lossy()),
        shell_quote(&plugin.to_string_lossy()),
    )
}

fn claude_wrapper(
    claude: &Path,
    plugin: &Path,
    channel_arguments: &str,
    telemetry_environment: &[(String, String)],
) -> String {
    let no_conflicts = crate::agent_intel::telemetry::CLAUDE_TELEMETRY_CONFLICTING_ENV
        .iter()
        .map(|name| format!("[ \"${{{name}+x}}\" != x ]"))
        .collect::<Vec<_>>()
        .join(" && ");
    let mut exports = String::new();
    for (name, value) in telemetry_environment {
        let _ = writeln!(&mut exports, "  export {name}={}", shell_quote(value));
    }
    format!(
        "#!/bin/sh\nif {no_conflicts}; then\n{exports}fi\nexec {} --plugin-dir {}{} \"$@\"\n",
        shell_quote(&claude.to_string_lossy()),
        shell_quote(&plugin.to_string_lossy()),
        channel_arguments,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rooms::mailbox_store::{MailboxEntry, RoomTaskDeliveryReason};
    use kodosi_domain::ids::{SessionId, UserId};
    use time::OffsetDateTime;

    #[test]
    fn extension_source_contains_no_prompt_or_tool_arguments() {
        assert!(!EXTENSION_TEMPLATE.contains("data.content"));
        assert!(!EXTENSION_TEMPLATE.contains("data.arguments"));
        assert!(!EXTENSION_TEMPLATE.contains("fullCommandText"));
        assert!(!EXTENSION_TEMPLATE.contains("error: short(data.error)"));
        assert!(!EXTENSION_TEMPLATE.contains("message: short(data.message)"));
        assert!(EXTENSION_TEMPLATE.contains("onEvent: (event) =>"));
        let manifest: serde_json::Value =
            serde_json::from_str(PLUGIN_MANIFEST).expect("plugin manifest");
        assert_eq!(
            manifest["extensions"],
            serde_json::json!({"paths":["./extensions"],"exclusive":true})
        );
        let wrapper = copilot_wrapper(Path::new("/usr/bin/copilot"), Path::new("/plugin"));
        assert!(wrapper.contains("COPILOT_PLUGIN_DIR_ONLY=true"));
        assert!(wrapper.contains("--experimental --plugin-dir"));
    }

    #[test]
    fn copilot_formatter_uses_actual_serialized_mailbox_entry_keys() {
        let assigned_session_id = SessionId::new();
        let assigned_incarnation_id = uuid::Uuid::now_v7();
        let task = MailboxEntry::RoomTask {
            room_id: "room-a".to_owned(),
            room_name: Some("Engineering".to_owned()),
            task_id: "task-1".to_owned(),
            title: "Ship it".to_owned(),
            status: "Open".to_owned(),
            revision: 17,
            assigned_session_id: Some(assigned_session_id),
            assigned_session_incarnation_id: Some(assigned_incarnation_id),
            delivery_reason: RoomTaskDeliveryReason::CurrentAssignmentOrUpdate,
            at: OffsetDateTime::UNIX_EPOCH,
        };
        let chat = MailboxEntry::RoomChat {
            room_id: "room-a".to_owned(),
            room_name: Some("Engineering".to_owned()),
            author_user_id: UserId::try_from("01900000-0000-7000-8000-000000000001")
                .expect("user id"),
            author_session_id: Some(SessionId::new()),
            recipient_session_ids: Vec::new(),
            recipient_user_ids: Vec::new(),
            body: "hello".to_owned(),
            seq: 7,
            at: OffsetDateTime::UNIX_EPOCH,
        };

        let task_json = serde_json::to_value(task).expect("serialize actual task entry");
        let chat_json = serde_json::to_value(chat).expect("serialize actual chat entry");
        assert_eq!(task_json["kind"], "roomTask");
        assert_eq!(task_json["room_id"], "room-a");
        assert_eq!(task_json["task_id"], "task-1");
        assert_eq!(task_json["revision"], 17);
        assert_eq!(
            task_json["assigned_session_id"],
            assigned_session_id.to_string()
        );
        assert_eq!(
            task_json["assigned_session_incarnation_id"],
            assigned_incarnation_id.to_string()
        );
        assert_eq!(task_json["delivery_reason"], "currentAssignmentOrUpdate");
        assert!(task_json.get("roomId").is_none());
        assert_eq!(chat_json["room_id"], "room-a");
        assert!(chat_json.get("roomId").is_none());

        for formatter_access in [
            "entry.request_id",
            "entry.from_session_id",
            "entry.room_id",
            "entry.room_name",
            "entry.task_id",
            "entry.assigned_session_id",
            "entry.assigned_session_incarnation_id",
            "entry.delivery_reason",
        ] {
            assert!(
                EXTENSION_TEMPLATE.contains(formatter_access),
                "formatter did not consume actual mailbox key {formatter_access}"
            );
        }
        assert!(!EXTENSION_TEMPLATE.contains("entry.roomId"));
        assert!(!EXTENSION_TEMPLATE.contains("entry.taskId"));
        assert!(EXTENSION_TEMPLATE.contains("\"--expected-revision\""));
    }

    #[test]
    fn otel_is_claude_wrapper_only_and_preserves_managed_settings() {
        let directory = tempfile::tempdir().expect("tempdir");
        let fake_claude = directory.path().join("claude-real");
        write_private(
            &fake_claude,
            br#"#!/bin/sh
printf '%s|%s|%s\n' "${CLAUDE_CODE_ENABLE_TELEMETRY-unset}" "${OTEL_EXPORTER_OTLP_LOGS_ENDPOINT-unset}" "${OTEL_EXPORTER_OTLP_ENDPOINT-unset}"
"#,
            true,
        )
        .expect("fake Claude");
        let telemetry = vec![
            ("CLAUDE_CODE_ENABLE_TELEMETRY".to_owned(), "1".to_owned()),
            (
                "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT".to_owned(),
                "http://127.0.0.1:1/v1/logs/session/incarnation/capability".to_owned(),
            ),
        ];
        let wrapper = directory.path().join("claude");
        write_private(
            &wrapper,
            claude_wrapper(&fake_claude, directory.path(), "", &telemetry).as_bytes(),
            true,
        )
        .expect("Claude wrapper");

        let shell_environment = session_shell_environment(directory.path());
        assert_eq!(shell_environment.len(), 1);
        assert_eq!(shell_environment[0].0, "PATH");
        assert!(
            shell_environment
                .iter()
                .all(|(name, _)| !name.starts_with("OTEL_")
                    && name != "CLAUDE_CODE_ENABLE_TELEMETRY")
        );

        let injected = std::process::Command::new(&wrapper)
            .env_clear()
            .output()
            .expect("run wrapper");
        assert!(injected.status.success());
        assert_eq!(
            String::from_utf8_lossy(&injected.stdout).trim(),
            "1|http://127.0.0.1:1/v1/logs/session/incarnation/capability|unset"
        );

        let managed = std::process::Command::new(&wrapper)
            .env_clear()
            .env("OTEL_EXPORTER_OTLP_ENDPOINT", "https://managed.invalid")
            .output()
            .expect("run wrapper with managed telemetry");
        assert!(managed.status.success());
        assert_eq!(
            String::from_utf8_lossy(&managed.stdout).trim(),
            "unset|unset|https://managed.invalid"
        );
    }

    #[test]
    fn local_plugin_manifest_is_accepted_by_installed_copilot() {
        let Some(copilot) = find_executable("copilot") else {
            return;
        };
        let directory = tempfile::tempdir().expect("tempdir");
        let plugin = directory.path().join("plugin");
        let extension = plugin.join("extensions").join("kodosi");
        let skill = plugin.join("skills").join("kodosi-room");
        create_private_dir(&extension).expect("extension directory");
        create_private_dir(&skill).expect("skill directory");
        write_private(
            &plugin.join("plugin.json"),
            PLUGIN_MANIFEST.as_bytes(),
            false,
        )
        .expect("manifest");
        write_private(
            &skill.join("SKILL.md"),
            include_bytes!("../../skills/kodosi-room/SKILL.md"),
            false,
        )
        .expect("skill");
        let source = EXTENSION_TEMPLATE
            .replace(
                "__KODOSI_ENDPOINT__",
                "\"http://127.0.0.1:1/v1/copilot/test\"",
            )
            .replace(
                "__KODOSI_PERMISSION_ENDPOINT__",
                "\"http://127.0.0.1:1/v1/copilot-permission/test\"",
            )
            .replace("__KODOSI_CLI__", "\"/usr/bin/false\"");
        write_private(&extension.join("extension.mjs"), source.as_bytes(), false)
            .expect("extension");

        let output = std::process::Command::new(copilot)
            .env("COPILOT_PLUGIN_DIR_ONLY", "true")
            .args(["--experimental", "--plugin-dir"])
            .arg(&plugin)
            .args(["plugin", "list"])
            .output()
            .expect("run Copilot");
        assert!(
            output.status.success(),
            "Copilot rejected plugin: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("kodosi-session"),
            "Copilot did not list the session-scoped plugin: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }

    #[test]
    fn local_room_plugin_manifest_is_accepted_by_installed_claude() {
        let Some(claude) = find_executable("claude") else {
            return;
        };
        let directory = tempfile::tempdir().expect("tempdir");
        let plugin = directory.path().join("plugin");
        let manifest = plugin.join(".claude-plugin");
        let skill = plugin.join("skills").join("kodosi-room");
        create_private_dir(&manifest).expect("manifest directory");
        create_private_dir(&skill).expect("skill directory");
        write_private(
            &manifest.join("plugin.json"),
            CLAUDE_PLUGIN_MANIFEST.as_bytes(),
            false,
        )
        .expect("manifest");
        write_private(
            &skill.join("SKILL.md"),
            include_bytes!("../../skills/kodosi-room/SKILL.md"),
            false,
        )
        .expect("skill");

        let output = std::process::Command::new(claude)
            .args(["plugin", "validate"])
            .arg(&plugin)
            .output()
            .expect("run Claude");
        assert!(
            output.status.success(),
            "Claude rejected plugin: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

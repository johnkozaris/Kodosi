use super::Context;
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) fn status(context: &Context) -> String {
    format!("{}\n{}", account(context), terminals(context))
}

pub(super) fn terminals(context: &Context) -> String {
    let listed = context
        .sessions
        .iter()
        .filter(|session| session["hidden"] != true)
        .collect::<Vec<_>>();
    if listed.is_empty() {
        return "No terminals. Start one: kodosi terminal start".to_owned();
    }
    let mut rows = vec![["NAME", "WHERE", "STATE", "SHARED WITH", "ID"].map(str::to_owned)];
    rows.extend(listed.into_iter().map(|session| {
        [
            text(session, "name"),
            place(session),
            state(session),
            names(&users(session), &context.friends),
            short(session, "id"),
        ]
    }));
    table(&rows)
}

pub(super) fn session(session: &Value, context: &Context) -> String {
    let mut rows = vec![
        ["Name".to_owned(), text(session, "name")],
        ["Where".to_owned(), place(session)],
        ["State".to_owned(), state(session)],
        ["Message".to_owned(), message(session)],
        ["Folder".to_owned(), text(session, "workingDir")],
        ["Room".to_owned(), text(session, "missionName")],
        [
            "Shared with".to_owned(),
            names(&users(session), &context.friends),
        ],
        ["ID".to_owned(), text(session, "id")],
    ];
    rows.retain(|row| !row[1].is_empty());
    table(&rows)
}

pub(super) fn sharing(session: &Value, users: &BTreeSet<String>, friends: &[Value]) -> String {
    let name = text(session, "name");
    if users.is_empty() {
        format!("{name} is shared with nobody.")
    } else {
        format!("{name} is shared with {}.", names(users, friends))
    }
}

pub(super) fn link_code(event: &Value) -> String {
    format!(
        "Type this code on a device that you already approved\n(choose Approve in the app there, or run `kodosi devices approve <code>`):\n\n    {}\n\nWaiting for approval. Press Ctrl-C to stop waiting.",
        text(event, "code")
    )
}

pub(super) fn result(event: &Value, context: &Context) -> String {
    let latest = |kind: &str| context.latest.get(kind).cloned().unwrap_or_default();
    match (
        event["type"].as_str().unwrap_or_default(),
        event["operation"].as_str().unwrap_or_default(),
    ) {
        ("auth.result", "auth.logout") => "Signed out.".to_owned(),
        ("auth.result", "auth.deleteAccount") => "The account is deleted.".to_owned(),
        ("auth.result", _) => account(context),
        ("devices.result", "devices.recovery.create") => format!(
            "{}\nKeep this key in a safe place. It approves a new device when you have no other approved device. Kodosi does not show it again.",
            text(event, "key")
        ),
        ("devices.result", "devices.link.approve") => format!(
            "{} is approved.",
            text(&latest("devices.link.resolved"), "deviceLabel")
        ),
        ("devices.result", "devices.link.cancelSelf") => "The request is cancelled.".to_owned(),
        ("devices.result", _) => devices(&latest("devices.list"), &latest("devices.hosts")),
        ("friends.result" | "mission.result", _) if unapproved(context) => account(context),
        ("friends.result", "friends.invite") => text(event, "text"),
        ("friends.result", _) => friends(&latest("friends.snapshot")),
        ("mission.result", "mission.show") => {
            "The Kodosi app shows the room when it is open.".to_owned()
        }
        ("mission.result", _) => "Done.".to_owned(),
        ("session.result", _) => session_result(event, context),
        ("provider.result", "provider.agents") => agents(&event["result"]),
        ("provider.result", "provider.agentModels") => agent_models(&event["result"]),
        ("provider.result", "provider.usage") => usage(&event["result"]),
        ("dosi.result", "dosi.stop") => "Dosi stops.".to_owned(),
        ("dosi.result", "dosi.ask") => {
            "The person sees the question. End your turn; the answer comes as the next message."
                .to_owned()
        }
        _ => serde_json::to_string_pretty(event).unwrap_or_default(),
    }
}

fn unapproved(context: &Context) -> bool {
    context
        .account
        .as_ref()
        .is_some_and(|account| account["enrolled"] == false)
}

fn account(context: &Context) -> String {
    let Some(account) = &context.account else {
        return context.restoring.as_ref().map_or_else(
            || "Not signed in. Terminals on this computer work without an account.\nSign in to use them from your other devices and to share them: kodosi auth login".to_owned(),
            restoring,
        );
    };
    let name = text(account, "displayName");
    let mut signed_in = if name.is_empty() {
        "Signed in.".to_owned()
    } else {
        format!("Signed in as {name}.")
    };
    if let Some(kept) = kept(account) {
        signed_in = format!("{signed_in}\n{kept}");
    }
    if unapproved(context) {
        format!(
            "{signed_in}\nThis device is not approved. Run `kodosi devices link`, `kodosi devices recover`, or `kodosi devices reset` to start fresh."
        )
    } else {
        signed_in
    }
}

fn restoring(event: &Value) -> String {
    match event["reason"].as_str() {
        Some("offline") => "Signing in again when Kodosi reaches the server.".to_owned(),
        Some("updateRequired") => {
            "Signing in again needs a newer Kodosi. Update Kodosi.".to_owned()
        }
        Some("unreadable") => match event["credentialStore"].as_str() {
            Some("keychain") => format!(
                "Kodosi cannot open the {KEYCHAIN}, where it keeps your sign-in.\nUnlock the {KEYCHAIN}, or open Kodosi in your desktop session."
            ),
            Some(_) => format!(
                "Kodosi cannot read your sign-in in {}.",
                text(event, "credentialFolder")
            ),
            None => "Kodosi cannot read your saved sign-in.".to_owned(),
        },
        _ => "Signing in again when the Kodosi server answers.".to_owned(),
    }
}

const KEYCHAIN: &str = if cfg!(target_os = "linux") {
    "keychain"
} else {
    "Keychain"
};

fn kept(account: &Value) -> Option<String> {
    let folder = text(account, "credentialFolder");
    Some(match account["credentialStore"].as_str()? {
        "keychain" if cfg!(target_os = "linux") => {
            "Kodosi keeps your sign-in in the keychain (Secret Service).".to_owned()
        }
        "keychain" => "Kodosi keeps your sign-in in the Keychain.".to_owned(),
        "protectedFile" => {
            format!(
                "Kodosi keeps your sign-in in files that Windows protects for you, in {folder}."
            )
        }
        "privateFile" => format!("Kodosi keeps your sign-in in private files in {folder}."),
        _ => return None,
    })
}

fn session_result(event: &Value, context: &Context) -> String {
    match event["operation"].as_str().unwrap_or_default() {
        "session.create" => {
            let id = text(event, "sessionId");
            let name = context
                .sessions
                .iter()
                .find(|session| session["id"] == id.as_str())
                .map_or_else(
                    || "the terminal".to_owned(),
                    |session| text(session, "name"),
                );
            let reference = crate::protocol::short_id(&id);
            format!("Started {name} ({reference}). Open it: kodosi terminal attach {reference}")
        }
        "session.run" => "The agent is starting in the terminal.".to_owned(),
        "session.input" => "Sent to the terminal.".to_owned(),
        "session.message" => "Kodosi gave the message to the agent.".to_owned(),
        "session.read" => text(event, "text"),
        "session.show" => "The Kodosi app shows the terminal when it is open.".to_owned(),
        "session.close" => event["text"]
            .as_str()
            .unwrap_or("The terminal is closed.")
            .to_owned(),
        "session.leave" => "You left the terminal.".to_owned(),
        _ => "Done.".to_owned(),
    }
}

fn label(agent: &str) -> &'static str {
    crate::agents::Agent::named(agent).map_or("Agent", crate::agents::Agent::label)
}

fn pick<'a>(result: &'a Value, agent: &str) -> &'a Value {
    result["choice"]["picks"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|pick| pick["agent"] == agent)
        .unwrap_or(&Value::Null)
}

fn chosen(pick: &Value) -> String {
    ["account", "model", "effort"]
        .into_iter()
        .filter_map(|key| pick[key].as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn agents(result: &Value) -> String {
    let mut lines = Vec::new();
    for info in result["agents"].as_array().into_iter().flatten() {
        let agent = text(info, "agent");
        if info["installed"] != true {
            lines.push(format!(
                "{agent}  {}: not installed. Install: {}",
                label(&agent),
                text(info, "install")
            ));
            continue;
        }
        let accounts = info["accounts"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|account| {
                account["identity"].as_str().map_or_else(
                    || text(account, "name"),
                    |identity| format!("{} ({identity})", text(account, "name")),
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let last = chosen(pick(result, &agent));
        lines.push(format!(
            "{agent}  {}: accounts {accounts}{}",
            label(&agent),
            if last.is_empty() {
                String::new()
            } else {
                format!("; last choice {last}")
            }
        ));
    }
    lines.join("\n")
}

fn agent_models(result: &Value) -> String {
    let models = result["models"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|model| {
            let id = text(model, "id");
            let name = text(model, "label");
            if name == id {
                id
            } else {
                format!("{id} ({name})")
            }
        })
        .collect::<Vec<_>>();
    let efforts = result["efforts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|effort| text(effort, "id"))
        .collect::<Vec<_>>();
    let list = |items: Vec<String>| {
        if items.is_empty() {
            "none".to_owned()
        } else {
            items.join(", ")
        }
    };
    format!("Models: {}\nEfforts: {}", list(models), list(efforts))
}

fn usage(result: &Value) -> String {
    let lines = result["accounts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|account| {
            let limits = account["limits"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|limit| {
                    let level = match limit["level"].as_str() {
                        Some("near") => " (near the limit)",
                        Some("reached") => " (limit reached)",
                        _ => "",
                    };
                    format!("{} {}%{level}", window(limit), limit["used"])
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{}  {}: {limits}",
                text(account, "agent"),
                text(account, "account")
            )
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        "No signed-in account reports plan limits.".to_owned()
    } else {
        lines.join("\n")
    }
}

fn window(limit: &Value) -> String {
    if let Some(scope) = limit["scope"].as_str() {
        return scope.to_owned();
    }
    match limit["hours"].as_u64() {
        None => "month".to_owned(),
        Some(24) => "day".to_owned(),
        Some(168) => "week".to_owned(),
        Some(hours) if hours % 24 == 0 => format!("{}d", hours / 24),
        Some(hours) => format!("{hours}h"),
    }
}

fn devices(event: &Value, hosts: &Value) -> String {
    let online = |device: &Value| {
        hosts["deviceIds"]
            .as_array()
            .is_some_and(|online| online.contains(&device["deviceId"]))
    };
    let mut lines = Vec::new();
    if event["localDeviceEnrolled"] == true {
        lines.push("This device is approved.".to_owned());
    } else {
        lines.push(format!(
            "This device is not approved. {}",
            text(event, "notice")
        ));
        lines.push(
            "Run `kodosi devices link`, `kodosi devices recover`, or `kodosi devices reset` to start fresh.".to_owned(),
        );
    }
    let rows = event["devices"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|device| {
            [
                text(device, "label"),
                if device["deviceId"] == event["selfDeviceId"] {
                    "this device".to_owned()
                } else if device["recoveryKey"] == true {
                    "recovery key".to_owned()
                } else if online(device) {
                    "online".to_owned()
                } else {
                    String::new()
                },
                short(device, "deviceId"),
            ]
        })
        .collect::<Vec<_>>();
    if !rows.is_empty() {
        let mut all = vec![["DEVICE", "", "ID"].map(str::to_owned)];
        all.extend(rows);
        lines.push(table(&all));
    }
    lines.join("\n")
}

fn friends(event: &Value) -> String {
    let list = |key: &str| event[key].as_array().cloned().unwrap_or_default();
    let mut lines = Vec::new();
    let rows = list("friends")
        .iter()
        .map(|friend| {
            let state = if friend["identityState"] == "changed" {
                format!(
                    "identity changed: kodosi friends trust {}",
                    text(friend, "handle")
                )
            } else if friend["verified"] == true {
                "verified".to_owned()
            } else {
                "not verified".to_owned()
            };
            [text(friend, "handle"), text(friend, "displayName"), state]
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        lines.push("No friends. Add one: kodosi friends add <username or invite>".to_owned());
    } else {
        let mut all = vec![["FRIEND", "NAME", "STATE"].map(str::to_owned)];
        all.extend(rows);
        lines.push(table(&all));
    }
    for (key, title, hint) in [
        ("incoming", "Requests to you", "kodosi friends accept"),
        ("outgoing", "Requests from you", "kodosi friends cancel"),
    ] {
        let handles = list(key)
            .iter()
            .map(|request| text(request, "handle"))
            .collect::<Vec<_>>();
        if !handles.is_empty() {
            lines.push(format!(
                "{title}: {} ({hint} <username>)",
                handles.join(", ")
            ));
        }
    }
    lines.join("\n")
}

pub(super) fn missions(event: &Value) -> String {
    let list = |key: &str| event[key].as_array().cloned().unwrap_or_default();
    let mut lines = Vec::new();
    let rows = list("missions")
        .iter()
        .map(|mission| {
            let unread = if mission["unread"] == true {
                "new messages"
            } else {
                ""
            };
            [
                text(mission, "name"),
                unread.to_owned(),
                short(mission, "id"),
            ]
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        lines.push("No rooms. Make one: kodosi room create <name>".to_owned());
    } else {
        let mut all = vec![["ROOM", "", "ID"].map(str::to_owned)];
        all.extend(rows);
        lines.push(table(&all));
    }
    for invitation in list("invitations") {
        lines.push(format!(
            "{} invited you to {}: kodosi room accept \"{}\"",
            text(&invitation, "inviterName"),
            text(&invitation, "missionName"),
            text(&invitation, "missionName")
        ));
    }
    lines.join("\n")
}

fn place(session: &Value) -> String {
    if session["kind"] == "local" {
        return "this computer".to_owned();
    }
    let host = text(session, "hostName");
    if session["isOwner"] == true {
        host
    } else {
        format!("{}, {host}", text(session, "ownerName"))
    }
}

fn state(session: &Value) -> String {
    let state = if session["kind"] == "local" {
        text(session, "status")
    } else {
        match (
            session["connectionState"].as_str().unwrap_or_default(),
            session["status"].as_str().unwrap_or_default(),
        ) {
            (_, "offline") => "host offline".to_owned(),
            ("connected", "reconnecting") => "reconnecting".to_owned(),
            ("connected", _) => "open".to_owned(),
            ("offline", _) => "not open".to_owned(),
            (other, _) => other.to_owned(),
        }
    };
    if matches!(state.as_str(), "running" | "open" | "not open")
        && let Some(activity) = activity(session)
    {
        return activity;
    }
    state
}

pub(super) fn offline(session: &Value) -> Option<String> {
    (session["status"] == "offline").then(|| {
        session["hostName"].as_str().map_or_else(
            || "Its computer is offline.".to_owned(),
            |host| format!("{host} is offline."),
        )
    })
}

fn activity(session: &Value) -> Option<String> {
    let status = &session["programStatus"];
    Some(match (status["state"].as_str()?, status["kind"].as_str()) {
        ("working", _) => status["progress"].as_u64().map_or_else(
            || "working".to_owned(),
            |progress| format!("working {progress}%"),
        ),
        ("blocked", Some("permission")) => "needs approval".to_owned(),
        ("blocked", Some("question")) => "needs an answer".to_owned(),
        ("blocked", Some("auth")) => "needs sign-in".to_owned(),
        ("blocked", _) => "needs you".to_owned(),
        ("done", _) => "done".to_owned(),
        ("error", _) => "failed".to_owned(),
        _ => return None,
    })
}

fn message(session: &Value) -> String {
    let status = &session["programStatus"];
    [text(status, "title"), text(status, "message")]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(": ")
}

fn users(session: &Value) -> BTreeSet<String> {
    if session["isOwner"] != true {
        return BTreeSet::new();
    }
    session["sharedWith"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|user| user.as_str().map(str::to_owned))
        .collect()
}

fn names(users: &BTreeSet<String>, friends: &[Value]) -> String {
    let known = users
        .iter()
        .filter_map(|user| {
            friends
                .iter()
                .find(|friend| friend["userId"] == user.as_str())
        })
        .map(|friend| text(friend, "handle"))
        .collect::<Vec<_>>();
    match users.len() - known.len() {
        0 => known.join(", "),
        1 if known.is_empty() => "1 friend".to_owned(),
        unknown if known.is_empty() => format!("{unknown} friends"),
        unknown => format!("{} and {unknown} more", known.join(", ")),
    }
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}

fn short(value: &Value, key: &str) -> String {
    crate::protocol::short_id(value[key].as_str().unwrap_or_default()).to_owned()
}

fn table<const N: usize>(rows: &[[String; N]]) -> String {
    let widths: [usize; N] = std::array::from_fn(|column| {
        rows.iter()
            .map(|row| row[column].chars().count())
            .max()
            .unwrap_or(0)
    });
    rows.iter()
        .map(|row| {
            row.iter()
                .zip(widths)
                .map(|(cell, width)| format!("{cell:<width$}"))
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_saved_sign_in_that_comes_back_is_not_called_signed_out() {
        let context = Context::from_events(&[
            json!({"type":"auth.restoring","accountEpoch":0,"accountUserId":null,"reason":"offline"}),
        ]);
        assert_eq!(
            account(&context),
            "Signing in again when Kodosi reaches the server."
        );
    }

    #[test]
    fn the_account_says_where_the_sign_in_is_kept() {
        let context = Context::from_events(&[json!({
            "type":"auth.ready","accountEpoch":1,"accountUserId":"me","displayName":"alice","enrolled":true,
            "credentialStore":"privateFile","credentialFolder":"/home/alice/.config/kodosi/secrets",
        })]);
        assert_eq!(
            account(&context),
            "Signed in as alice.\nKodosi keeps your sign-in in private files in /home/alice/.config/kodosi/secrets."
        );
        let locked = Context::from_events(&[json!({
            "type":"auth.restoring","accountEpoch":0,"accountUserId":null,"reason":"unreadable","credentialStore":"keychain",
        })]);
        assert!(account(&locked).starts_with(&format!("Kodosi cannot open the {KEYCHAIN}, where")));
        let unreadable = Context::from_events(&[json!({
            "type":"auth.restoring","accountEpoch":0,"accountUserId":null,"reason":"unreadable",
            "credentialStore":"privateFile","credentialFolder":"/home/alice/.config/kodosi/secrets",
        })]);
        assert_eq!(
            account(&unreadable),
            "Kodosi cannot read your sign-in in /home/alice/.config/kodosi/secrets."
        );
    }

    #[test]
    fn a_terminal_list_names_the_place_the_state_and_the_friends() {
        let context = Context::from_events(&[
            json!({"type":"auth.ready","accountEpoch":1,"accountUserId":"me","displayName":"alice","enrolled":true}),
            json!({"type":"friends.snapshot","accountEpoch":1,"accountUserId":"me","friends":[{"userId":"u-bob","handle":"bob"}]}),
            json!({"type":"sessions.snapshot","accountEpoch":1,"accountUserId":"me","sessions":[
                {"id":"01a10d06-92a7-7367-8536-65c4895d05ae","name":"work","kind":"local","status":"running","isOwner":true,"sharedWith":["u-bob","u-else"]},
                {"id":"01a10d06-92a7-7367-8536-000000000002","name":"build","kind":"remote","status":"running","connectionState":"offline","isOwner":false,"ownerName":"carol","hostName":"Studio","sharedWith":[],"programStatus":{"state":"working"}},
                {"id":"01a10d06-92a7-7367-8536-000000000003","name":"agent","kind":"local","status":"running","isOwner":true,"sharedWith":[],"programStatus":{"state":"blocked","kind":"permission","title":"Review","message":"Allow the command?"}},
                {"id":"01a10d06-92a7-7367-8536-000000000004","name":"deploy","kind":"remote","status":"running","connectionState":"connected","isOwner":true,"hostName":"Studio","sharedWith":[],"programStatus":{"state":"working","progress":40}},
                {"id":"01a10d06-92a7-7367-8536-000000000005","name":"shell","kind":"local","status":"running","isOwner":true,"sharedWith":[],"programStatus":{"state":"idle"}},
                {"id":"01a10d06-92a7-7367-8536-000000000006","name":"away","kind":"remote","status":"offline","connectionState":"offline","isOwner":false,"ownerName":"carol","hostName":"Studio","sharedWith":[]},
                {"id":"01a10d06-92a7-7367-8536-000000000007","name":"tv","kind":"remote","status":"offline","connectionState":"connected","isOwner":true,"hostName":"Studio","sharedWith":[]},
                {"id":"01a10d06-92a7-7367-8536-000000000008","name":"old","kind":"remote","status":"offline","connectionState":"offline","isOwner":true,"hostName":"Studio","sharedWith":[],"hidden":true},
            ]}),
        ]);
        assert_eq!(
            status(&context),
            "Signed in as alice.\n\
             NAME    WHERE          STATE           SHARED WITH     ID\n\
             work    this computer  running         bob and 1 more  5d05ae\n\
             build   carol, Studio  working                         000002\n\
             agent   this computer  needs approval                  000003\n\
             deploy  Studio         working 40%                     000004\n\
             shell   this computer  running                         000005\n\
             away    carol, Studio  host offline                    000006\n\
             tv      Studio         host offline                    000007"
        );
        assert!(
            session(&context.sessions[2], &context)
                .lines()
                .any(|line| line.starts_with("Message ")
                    && line.ends_with(" Review: Allow the command?"))
        );
    }

    #[test]
    fn a_device_that_is_not_approved_shows_its_reason_and_the_way_out() {
        let listed = devices(
            &json!({"localDeviceEnrolled":false,"selfDeviceId":"a","notice":"Approve this device from one of your existing devices.","devices":[]}),
            &Value::Null,
        );
        assert!(listed.starts_with(
            "This device is not approved. Approve this device from one of your existing devices."
        ));
        assert!(listed.contains("kodosi devices link"));
    }

    #[test]
    fn usage_names_each_window_and_says_when_a_limit_is_near() {
        fn limit(hours: Option<u32>, scope: Option<&str>, used: u8, level: &str) -> Value {
            json!({"hours": hours, "scope": scope, "used": used, "level": level, "headline": false})
        }
        assert_eq!(
            usage(&json!({"accounts": [
                {"agent": "claude", "account": "work", "readAtMs": 1, "limits": [
                    limit(Some(5), None, 42, "normal"),
                    limit(Some(168), None, 85, "near"),
                    limit(Some(168), Some("Fable"), 100, "reached"),
                ]},
                {"agent": "codex", "account": "default", "readAtMs": 1, "limits": [limit(Some(72), None, 3, "normal")]},
                {"agent": "copilot", "account": "default", "readAtMs": 1, "limits": [limit(None, None, 59, "normal")]},
            ]})),
            "claude  work: 5h 42%, week 85% (near the limit), Fable 100% (limit reached)\n\
             codex  default: 3d 3%\n\
             copilot  default: month 59%"
        );
        assert_eq!(
            usage(&json!({"accounts": []})),
            "No signed-in account reports plan limits."
        );
    }

    #[test]
    fn a_room_list_names_the_rooms_and_the_command_that_accepts_an_invitation() {
        assert_eq!(
            missions(&json!({
                "missions":[
                    {"id":"019f0000-0000-7000-8000-00000000a1b2","name":"Work","unread":true},
                    {"id":"019f0000-0000-7000-8000-00000000c3d4","name":"Design"},
                ],
                "invitations":[{"inviterName":"bob","missionName":"Launch"}]
            })),
            "ROOM                  ID\n\
             Work    new messages  00a1b2\n\
             Design                00c3d4\n\
             bob invited you to Launch: kodosi room accept \"Launch\""
        );
        assert_eq!(
            missions(&json!({})),
            "No rooms. Make one: kodosi room create <name>"
        );
    }
}

use super::Context;
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) fn status(context: &Context) -> String {
    let mut lines = vec![account(context)];
    if context.sessions.is_empty() {
        lines.push("No terminals. Start one: kodosi session start".to_owned());
    } else {
        let mut rows = vec![["NAME", "WHERE", "STATE", "SHARED WITH", "ID"].map(str::to_owned)];
        rows.extend(context.sessions.iter().map(|session| {
            [
                text(session, "name"),
                place(session),
                state(session),
                names(&users(session), &context.friends),
                short(session, "id"),
            ]
        }));
        lines.push(table(&rows));
    }
    lines.join("\n")
}

pub(super) fn session(session: &Value, context: &Context) -> String {
    let mut rows = vec![
        ["Name".to_owned(), text(session, "name")],
        ["Where".to_owned(), place(session)],
        ["State".to_owned(), state(session)],
        ["Message".to_owned(), message(session)],
        ["Folder".to_owned(), text(session, "workingDir")],
        ["Mission".to_owned(), text(session, "missionName")],
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
        "Type this code on a device that you already approved\n(Settings, then Devices, or `kodosi devices approve <code>`):\n\n    {}\n\nWaiting for approval. Press Ctrl-C to stop waiting.",
        text(event, "code")
    )
}

pub(super) fn result(event: &Value, context: &Context) -> String {
    match event["type"].as_str().unwrap_or_default() {
        "auth.ready" => account(context),
        "auth.required" => "Signed out.".to_owned(),
        "devices.list" => devices(event),
        "devices.link.resolved" => format!("{} is approved.", text(event, "deviceLabel")),
        "devices.link.selfResolved" => "The request is cancelled.".to_owned(),
        "friends.snapshot" | "missions.snapshot" if unapproved(context) => account(context),
        "friends.snapshot" => friends(event),
        "friends.invite" => text(event, "text"),
        "missions.snapshot" => missions(event),
        "mission.snapshot" => mission(event),
        "mission.result" => "Done.".to_owned(),
        "session.result" => session_result(event, context),
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
        return "Not signed in. Terminals on this computer work without an account.\nSign in to use them from your other devices and to share them: kodosi auth login".to_owned();
    };
    let name = text(account, "displayName");
    let signed_in = if name.is_empty() {
        "Signed in.".to_owned()
    } else {
        format!("Signed in as {name}.")
    };
    if unapproved(context) {
        format!(
            "{signed_in}\nThis device is not approved. Run `kodosi devices link`, or `kodosi devices reset` to start fresh."
        )
    } else {
        signed_in
    }
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
            let reference = &id[id.len().saturating_sub(6)..];
            format!("Started {name} ({reference}). Open it: kodosi session attach {reference}")
        }
        "session.close" => "The terminal is closed.".to_owned(),
        "session.leave" => "You left the terminal.".to_owned(),
        _ => "Done.".to_owned(),
    }
}

fn devices(event: &Value) -> String {
    let mut lines = Vec::new();
    if event["localDeviceEnrolled"] == true {
        lines.push("This device is approved.".to_owned());
    } else {
        lines.push(format!(
            "This device is not approved. {}",
            text(event, "notice")
        ));
        lines.push(
            "Run `kodosi devices link`, or `kodosi devices reset` to start fresh.".to_owned(),
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

fn missions(event: &Value) -> String {
    let list = |key: &str| event[key].as_array().cloned().unwrap_or_default();
    let mut lines = Vec::new();
    let names = list("missions")
        .iter()
        .map(|mission| text(mission, "name"))
        .collect::<Vec<_>>();
    if names.is_empty() {
        lines.push("No Missions. Make one: kodosi mission create <name>".to_owned());
    } else {
        lines.push(format!("Missions: {}", names.join(", ")));
    }
    for invitation in list("invitations") {
        lines.push(format!(
            "{} invited you to {}: kodosi mission accept \"{}\"",
            text(&invitation, "inviterName"),
            text(&invitation, "missionName"),
            text(&invitation, "missionName")
        ));
    }
    lines.join("\n")
}

fn mission(event: &Value) -> String {
    let members = event["members"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|member| {
            if member["isOwner"] == true {
                format!("{} (owner)", text(member, "handle"))
            } else {
                text(member, "handle")
            }
        })
        .collect::<Vec<_>>();
    format!(
        "{}\nMembers: {}",
        text(&event["mission"], "name"),
        members.join(", ")
    )
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
            ("connected", "reconnecting") => "reconnecting".to_owned(),
            ("connected", _) => "open".to_owned(),
            ("offline", _) => "not open".to_owned(),
            (other, _) => other.to_owned(),
        }
    };
    if matches!(state.as_str(), "running" | "open")
        && let Some(activity) = activity(session)
    {
        return activity;
    }
    state
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
    let id = text(value, key);
    id[id.len().saturating_sub(6)..].to_owned()
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
            ]}),
        ]);
        assert_eq!(
            status(&context),
            "Signed in as alice.\n\
             NAME    WHERE          STATE           SHARED WITH     ID\n\
             work    this computer  running         bob and 1 more  5d05ae\n\
             build   carol, Studio  not open                        000002\n\
             agent   this computer  needs approval                  000003\n\
             deploy  Studio         working 40%                     000004\n\
             shell   this computer  running                         000005"
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
        );
        assert!(listed.starts_with(
            "This device is not approved. Approve this device from one of your existing devices."
        ));
        assert!(listed.contains("kodosi devices link"));
    }
}

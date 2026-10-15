use super::*;
use crate::{network::BackendConfig, rooms::crypto_tests::person};

#[test]
fn received_content_requires_the_signed_identity_metadata_and_matching_room_key() {
    let (keys, identity) = person();
    let user = identity.user_id.clone();
    let anchor = identity.root().unwrap();
    let room = Uuid::now_v7();
    let key = room_crypto::secret().unwrap();
    let mut state = KeyState {
        room_id: room,
        owner_user_id: user.clone(),
        author_id: user.clone(),
        device_id: keys.device_id.clone(),
        version: 1,
        epoch: 1,
        created_at_ms: 2000,
        previous_hash: String::new(),
        members: BTreeMap::from([(user.clone(), identity)]),
        recipients: vec![],
        previous_key: None,
    };
    state
        .recipients
        .push(room_crypto::wrap(&state, &user, &keys.device_id, keys.room_public(), &key).unwrap());
    let (body, signature) = room_crypto::sign(room_crypto::STATE_DOMAIN, &keys, &state).unwrap();
    let verified = room_crypto::verify_state(
        &SignedState {
            version: 1,
            body,
            signature,
        },
        room,
        &user,
        &anchor,
        None,
    )
    .unwrap();
    let cached = CachedRoom {
        states: BTreeMap::from([(1, verified)]),
        keys: BTreeMap::from([(1, key.clone())]),
        ..CachedRoom::default()
    };
    let mut content = room_crypto::Content {
        room_id: room,
        id: Uuid::now_v7(),
        kind: "message".into(),
        version: 1,
        key_version: 1,
        epoch: 1,
        author_id: user.clone(),
        device_id: keys.device_id.clone(),
        nonce: String::new(),
        ciphertext: String::new(),
    };
    content
        .encrypt(
            &key,
            &serde_json::to_vec(&Payload::Message {
                text: "Ready for review".into(),
                author_name: "Alice".into(),
                agent: Some("Claude".into()),
                terminal_id: None,
            })
            .unwrap(),
        )
        .unwrap();
    let (body, signature) =
        room_crypto::sign(room_crypto::CONTENT_DOMAIN, &keys, &content).unwrap();
    let item = Item {
        id: content.id,
        kind: "message".into(),
        version: 1,
        sequence: 1,
        key_version: 1,
        user_id: user,
        device_id: keys.device_id.clone(),
        created_at: "now".into(),
        body,
        signature,
    };
    assert!(
        matches!(BackendClient::room_payload(&cached,room,&item).unwrap(),Payload::Message { text,.. } if text == "Ready for review")
    );
    assert!(BackendClient::room_payload(&cached, Uuid::now_v7(), &item).is_err());
    let mut changed = item.clone();
    changed.version += 1;
    assert!(BackendClient::room_payload(&cached, room, &changed).is_err());
    changed = item.clone();
    changed.signature = BASE64.encode(vec![0; 3309]);
    assert!(BackendClient::room_payload(&cached, room, &changed).is_err());
    let mut missing = cached.clone();
    missing.keys.clear();
    assert!(BackendClient::room_payload(&missing, room, &item).is_err());
    missing = cached.clone();
    missing.states.clear();
    assert!(BackendClient::room_payload(&missing, room, &item).is_err());
    content.epoch = 2;
    let (body, signature) =
        room_crypto::sign(room_crypto::CONTENT_DOMAIN, &keys, &content).unwrap();
    changed.body = body;
    changed.signature = signature;
    assert!(BackendClient::room_payload(&cached, room, &changed).is_err());
}

#[test]
fn provider_changes_refresh_task_state_and_preserve_room_context() {
    let repository = rooms::providers::repository("https://github.com/team/project", None).unwrap();
    let mut task: rooms::Task = serde_json::from_value(json!({
        "id":Uuid::now_v7(),"version":3,"title":"Original","description":"Old body","closed":false,
        "assignedTo":null,"assignedName":null,"terminalId":"terminal","repositoryIds":["repo"],
        "note":"PR #42","issue":null
    }))
    .unwrap();
    let mut issue = rooms::Issue {
        repository_id: "repo".into(),
        number: 1,
        title: "Edited on GitHub".into(),
        body: "New body".into(),
        url: "https://github.com/team/project/issues/1".into(),
        closed: true,
        assignees: vec!["bob".into()],
    };
    BackendClient::apply_linked_issue(&mut task, &repository, issue.clone());
    assert_eq!(task.title, issue.title);
    assert!(task.closed);
    assert_eq!(task.assigned_to.as_deref(), Some("github:github.com:bob"));
    assert_eq!(task.note.as_deref(), Some("PR #42"));
    assert_eq!(task.terminal_id.as_deref(), Some("terminal"));
    issue.closed = false;
    issue.assignees.clear();
    BackendClient::apply_linked_issue(&mut task, &repository, issue);
    assert!(!task.closed);
    assert!(task.assigned_to.is_none());
    assert!(task.assigned_name.is_none());
}

#[tokio::test]
async fn a_device_with_new_keys_registers_its_own_room_key() {
    let (_root, network) = super::super::tests::fixture();
    let credentials = Credentials {
        user_id: Uuid::now_v7().to_string(),
        token: Zeroizing::new("token".into()),
        keys: Arc::new(DeviceKeys::generate().unwrap()),
        enrolled: true,
        notice: None,
        generation: network.generation(),
        cancel: CancellationToken::new(),
    };
    *network.inner.rooms.lock().await = RoomCache {
        generation: credentials.generation,
        registered: Some(credentials.keys.device_id.clone()),
        ..RoomCache::default()
    };
    network.register_room_key(&credentials).await.unwrap();
    let replaced = Credentials {
        keys: Arc::new(DeviceKeys::generate().unwrap()),
        ..credentials
    };
    assert!(matches!(
        network.register_room_key(&replaced).await,
        Err(Error::Unreachable)
    ));
}

fn signed_room(keys: &DeviceKeys, identity: IdentityBundle) -> (Uuid, CachedRoom, Secret) {
    let user = identity.user_id.clone();
    let anchor = identity.root().unwrap();
    let room = Uuid::now_v7();
    let key = room_crypto::secret().unwrap();
    let mut state = KeyState {
        room_id: room,
        owner_user_id: user.clone(),
        author_id: user.clone(),
        device_id: keys.device_id.clone(),
        version: 1,
        epoch: 1,
        created_at_ms: 2000,
        previous_hash: String::new(),
        members: BTreeMap::from([(user.clone(), identity)]),
        recipients: vec![],
        previous_key: None,
    };
    state
        .recipients
        .push(room_crypto::wrap(&state, &user, &keys.device_id, keys.room_public(), &key).unwrap());
    let (body, signature) = room_crypto::sign(room_crypto::STATE_DOMAIN, keys, &state).unwrap();
    let signed = SignedState {
        version: 1,
        body,
        signature,
    };
    let verified = room_crypto::verify_state(&signed, room, &user, &anchor, None).unwrap();
    let cached = CachedRoom {
        states: BTreeMap::from([(1, verified)]),
        keys: BTreeMap::from([(1, key.clone())]),
        ..CachedRoom::default()
    };
    (room, cached, key)
}

fn message(text: &str) -> Payload {
    Payload::Message {
        text: text.into(),
        author_name: "Alice".into(),
        agent: None,
        terminal_id: None,
    }
}

struct FakeRoom {
    room: Uuid,
    key: Secret,
    keys: Arc<DeviceKeys>,
    user: String,
    items: std::sync::Mutex<Vec<Value>>,
    requests: std::sync::Mutex<Vec<String>>,
}

impl FakeRoom {
    fn add(&self, id: Uuid, version: u64, payload: &Payload) -> u64 {
        let kind = match payload {
            Payload::Message { .. } => "message",
            Payload::Task { .. } => "task",
            Payload::Repository { .. } => "repository",
        };
        let mut content = room_crypto::Content {
            room_id: self.room,
            id,
            kind: kind.into(),
            version,
            key_version: 1,
            epoch: 1,
            author_id: self.user.clone(),
            device_id: self.keys.device_id.clone(),
            nonce: String::new(),
            ciphertext: String::new(),
        };
        content
            .encrypt(&self.key, &serde_json::to_vec(payload).unwrap())
            .unwrap();
        let (body, signature) =
            room_crypto::sign(room_crypto::CONTENT_DOMAIN, &self.keys, &content).unwrap();
        self.store(id, kind, version, &body, &signature)
    }

    fn store(&self, id: Uuid, kind: &str, version: u64, body: &str, signature: &str) -> u64 {
        let mut items = self.items.lock().unwrap();
        let sequence = items.len() as u64 + 1;
        items.push(json!({"id":id,"kind":kind,"version":version,"sequence":sequence,"keyVersion":1,
            "userId":self.user,"deviceId":self.keys.device_id,"createdAt":"now","body":body,"signature":signature}));
        drop(items);
        sequence
    }

    fn page(&self, query: &str) -> Value {
        let field = |name: &str| {
            query
                .split('&')
                .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
        };
        let number = |name: &str| field(name).and_then(|value| value.parse::<u64>().ok());
        let items = self.items.lock().unwrap().clone();
        let mut selected = items
            .iter()
            .filter(|item| field("kind").is_none_or(|kind| item["kind"] == kind))
            .filter(|item| item["sequence"].as_u64().unwrap() > number("after").unwrap_or(0))
            .filter(|item| {
                number("before").is_none_or(|before| item["sequence"].as_u64().unwrap() < before)
            })
            .cloned()
            .collect::<Vec<_>>();
        if field("before").is_some() {
            selected.reverse();
        }
        let limit = usize::try_from(number("limit").unwrap_or(100)).unwrap();
        let has_more = selected.len() > limit;
        selected.truncate(limit);
        selected.sort_by_key(|item| item["sequence"].as_u64().unwrap());
        json!({"roomId":self.room,"sequence":items.len(),"keyVersion":1,"hasMore":has_more,"items":selected})
    }

    fn answer(&self, line: &str, body: &[u8]) -> Value {
        let (method, target) = line.split_once(' ').unwrap();
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        match path {
            "/health/ready" => return crate::network::http::current_versions(),
            "/api/me/devices/challenge" => {
                return json!({"challengeId": Uuid::now_v7(), "challengeBytes": BASE64.encode([7; 32])});
            }
            "/api/me/device-sessions" => return json!({"session": "fixture-session"}),
            _ => {}
        }
        self.requests.lock().unwrap().push(line.to_owned());
        if method == "GET" {
            return self.page(query);
        }
        let written: Value = serde_json::from_slice(body).unwrap();
        let encoded = written["body"].as_str().unwrap();
        let content: Value = serde_json::from_slice(&BASE64.decode(encoded).unwrap()).unwrap();
        let id = Uuid::parse_str(path.rsplit('/').next().unwrap()).unwrap();
        let sequence = self.store(
            id,
            content["kind"].as_str().unwrap(),
            content["version"].as_u64().unwrap(),
            encoded,
            written["signature"].as_str().unwrap(),
        );
        self.items.lock().unwrap()[usize::try_from(sequence).unwrap() - 1].clone()
    }

    fn take_requests(&self) -> Vec<String> {
        std::mem::take(&mut *self.requests.lock().unwrap())
            .into_iter()
            .map(|line| line.replace(&self.room.to_string(), "ROOM"))
            .collect()
    }
}

async fn fake_room() -> (tempfile::TempDir, BackendClient, Credentials, Arc<FakeRoom>) {
    use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader};
    let (keys, identity) = person();
    let user = identity.user_id.clone();
    let (room, cached, key) = signed_room(&keys, identity);
    let fake = Arc::new(FakeRoom {
        room,
        key,
        keys: Arc::new(keys),
        user,
        items: std::sync::Mutex::default(),
        requests: std::sync::Mutex::default(),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = Arc::clone(&fake);
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let server = Arc::clone(&server);
            tokio::spawn(async move {
                let mut reader = BufReader::new(stream);
                let mut head = String::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                        return;
                    }
                    head.push_str(&line);
                    if line == "\r\n" {
                        break;
                    }
                }
                let length = head
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|value| value.parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                let mut body = vec![0; length];
                reader.read_exact(&mut body).await.unwrap();
                let line = head.lines().next().unwrap().rsplit_once(' ').unwrap().0;
                let answer = server.answer(line, &body).to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}",
                    answer.len()
                );
                drop(reader.get_mut().write_all(response.as_bytes()).await);
            });
        }
    });
    let storage = tempfile::tempdir().unwrap();
    let network = BackendClient::new(BackendConfig {
        api_url: format!("http://{address}/").parse().unwrap(),
        issuer: format!("http://{address}"),
        client_id: "test".into(),
        scopes: vec![],
        data_root: storage.path().to_owned(),
        secret_service: "test".into(),
        isolated: true,
    })
    .unwrap();
    let credentials = Credentials {
        user_id: fake.user.clone(),
        token: Zeroizing::new("token".into()),
        keys: Arc::clone(&fake.keys),
        enrolled: true,
        notice: None,
        generation: network.generation(),
        cancel: CancellationToken::new(),
    };
    *network.inner.rooms.lock().await = RoomCache {
        generation: credentials.generation,
        rooms: BTreeMap::from([(room, cached)]),
        registered: Some(fake.keys.device_id.clone()),
        ..RoomCache::default()
    };
    (storage, network, credentials, fake)
}

#[tokio::test]
async fn a_room_reads_only_the_items_after_the_sequence_that_it_has() {
    let (_root, network, credentials, fake) = fake_room().await;
    let room = fake.room;
    let task = Uuid::now_v7();
    let mut first: rooms::Task = serde_json::from_value(json!({
        "id":task,"version":1,"title":"Ship it","description":"","closed":false,"assignedTo":null,
        "assignedName":null,"terminalId":null,"repositoryIds":[],"note":null,"issue":null
    }))
    .unwrap();
    fake.add(Uuid::now_v7(), 1, &message("one"));
    fake.add(Uuid::now_v7(), 1, &message("two"));
    fake.add(
        task,
        1,
        &Payload::Task {
            task: first.clone(),
        },
    );
    assert!(network.sync_room(&credentials, room, None).await.unwrap());
    assert_eq!(fake.take_requests().len(), 3);
    let shown = network.show_room(&credentials, room).await.unwrap();
    assert_eq!(shown.messages.len(), 2);
    assert_eq!(shown.sequence, 3);

    fake.add(Uuid::now_v7(), 1, &message("three"));
    first.closed = true;
    let latest = fake.add(task, 2, &Payload::Task { task: first });
    assert!(network.sync_room(&credentials, room, None).await.unwrap());
    assert_eq!(
        fake.take_requests(),
        ["GET /api/missions/ROOM/content?limit=200&after=3"]
    );
    let shown = network.show_room(&credentials, room).await.unwrap();
    assert_eq!(
        shown
            .messages
            .iter()
            .map(|m| m.text.as_str())
            .collect::<Vec<_>>(),
        ["one", "two", "three"]
    );
    assert!(shown.tasks[0].closed && shown.tasks[0].version == 2);

    assert!(
        !network
            .sync_room(&credentials, room, Some(latest))
            .await
            .unwrap()
    );
    assert_eq!(fake.take_requests(), Vec::<String>::new());

    fake.items.lock().unwrap().truncate(1);
    assert!(network.sync_room(&credentials, room, None).await.unwrap());
    assert_eq!(fake.take_requests().len(), 4);
    let shown = network.show_room(&credentials, room).await.unwrap();
    assert_eq!(shown.sequence, 1);
    assert_eq!(shown.tasks, Vec::new());
}

#[tokio::test]
async fn a_post_keeps_its_own_item_and_reads_only_what_others_wrote_in_between() {
    let (_root, network, credentials, fake) = fake_room().await;
    let room = fake.room;
    fake.add(Uuid::now_v7(), 1, &message("before"));
    network.sync_room(&credentials, room, None).await.unwrap();
    fake.take_requests();

    let posted = Uuid::now_v7();
    let sequence = network
        .room_write(&credentials, room, posted, "message", 0, &message("mine"))
        .await
        .unwrap();
    assert!(
        !network
            .sync_room(&credentials, room, Some(sequence))
            .await
            .unwrap()
    );
    assert_eq!(
        fake.take_requests(),
        [format!("PUT /api/missions/ROOM/content/{posted}")]
    );

    fake.add(Uuid::now_v7(), 1, &message("theirs"));
    let sequence = network
        .room_write(
            &credentials,
            room,
            Uuid::now_v7(),
            "message",
            0,
            &message("again"),
        )
        .await
        .unwrap();
    assert!(
        network
            .sync_room(&credentials, room, Some(sequence))
            .await
            .unwrap()
    );
    assert_eq!(fake.take_requests().len(), 2);
    let shown = network.show_room(&credentials, room).await.unwrap();
    assert_eq!(
        shown
            .messages
            .iter()
            .map(|m| m.text.as_str())
            .collect::<Vec<_>>(),
        ["before", "mine", "theirs", "again"]
    );
}

#[test]
fn a_snapshot_has_the_newest_hundred_messages_and_every_message_not_shown_yet() {
    let room = Uuid::now_v7();
    let mut content = Content::default();
    let add = |content: &mut Content, sequences: std::ops::RangeInclusive<u64>| {
        for sequence in sequences {
            content.messages.insert(
                sequence,
                rooms::Message {
                    id: sequence.to_string(),
                    sequence,
                    author_id: "user".into(),
                    author_name: "Alice".into(),
                    agent: None,
                    terminal_id: None,
                    text: String::new(),
                    created_at: String::new(),
                },
            );
        }
        content.trim();
    };
    let range = |snapshot: &Snapshot| {
        (
            snapshot.messages.first().map(|m| m.sequence),
            snapshot.messages.len(),
            snapshot.has_older,
        )
    };
    add(&mut content, 1..=150);
    assert_eq!(range(&content.show(room)), (Some(51), 100, true));
    add(&mut content, 151..=270);
    assert_eq!(range(&content.show(room)), (Some(151), 120, true));
    add(&mut content, 271..=275);
    assert_eq!(range(&content.show(room)), (Some(176), 100, true));
    add(&mut content, 276..=1300);
    assert_eq!(
        content
            .messages
            .first_key_value()
            .map(|(sequence, _)| *sequence),
        Some(276)
    );
    assert_eq!(range(&content.show(room)), (Some(276), 1025, true));
    add(&mut content, 1301..=1301);
    assert_eq!(content.messages.len(), KEPT_MESSAGES);
}

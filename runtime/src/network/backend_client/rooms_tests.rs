use super::*;
use crate::rooms::crypto_tests::person;

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

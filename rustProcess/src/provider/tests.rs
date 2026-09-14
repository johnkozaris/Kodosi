use std::{
    fs,
    io::{Seek, SeekFrom, Write},
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
};

use rusqlite::Connection;

use super::*;

fn id(index: u128) -> String {
    uuid::Uuid::from_u128(index).to_string()
}

fn workspace(home: &Path) -> PathBuf {
    let path = home.join("project");
    fs::create_dir_all(&path).unwrap();
    path.canonicalize().unwrap()
}

fn claude_transcript(home: &Path, directory: &Path, index: u128, records: &str) -> PathBuf {
    let project = home
        .join(".claude/projects")
        .join(storage::encoded_claude_directory(directory).unwrap());
    fs::create_dir_all(&project).unwrap();
    let path = project.join(format!("{}.jsonl", id(index)));
    let cwd = serde_json::to_string(directory.to_str().unwrap()).unwrap();
    fs::write(
        &path,
        format!("{{\"type\":\"system\",\"cwd\":{cwd}}}\n{records}"),
    )
    .unwrap();
    path
}

fn copilot_transcript(home: &Path, directory: &Path, index: u128, finished: bool) -> PathBuf {
    let session = home.join(".copilot/session-state").join(id(index));
    fs::create_dir_all(&session).unwrap();
    let path = session.join("events.jsonl");
    let start = serde_json::json!({"type":"session.start","data":{"context":{"cwd":directory}}});
    let ended = if finished {
        "{\"type\":\"session.shutdown\"}\n"
    } else {
        ""
    };
    fs::write(
        &path,
        format!(
            "{start}\n{{\"type\":\"user.message\",\"data\":{{\"content\":\"hello\"}}}}\n{ended}"
        ),
    )
    .unwrap();
    let connection = Connection::open(home.join(".copilot/session-store.db")).unwrap();
    connection.execute_batch("CREATE TABLE IF NOT EXISTS schema_version(version INTEGER); INSERT INTO schema_version SELECT 3 WHERE NOT EXISTS(SELECT 1 FROM schema_version); CREATE TABLE IF NOT EXISTS sessions(id TEXT PRIMARY KEY, cwd TEXT, summary TEXT, created_at TEXT, updated_at TEXT);").unwrap();
    connection
        .execute(
            "INSERT INTO sessions VALUES (?1, ?2, 'Example', '2026-09-11', '2026-09-11')",
            rusqlite::params![id(index), directory.to_str().unwrap()],
        )
        .unwrap();
    path
}

#[tokio::test]
async fn claude_discovery_and_read_share_directory_validation() {
    let home = tempfile::tempdir().unwrap();
    let directory = workspace(home.path());
    let path = claude_transcript(
        home.path(),
        &directory,
        1,
        "{\"type\":\"user\",\"message\":{\"content\":\"hello\"}}\n",
    );
    let original = fs::read(&path).unwrap();
    let page = discover(
        &home.path().join(".claude"),
        Provider::Claude,
        directory.to_str().unwrap(),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].native_conversation_id, id(1));
    let transcript = read(
        &home.path().join(".claude"),
        Provider::Claude,
        directory.to_str().unwrap(),
        &id(1),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(transcript.entries[0].content, "hello");
    assert_eq!(fs::read(&path).unwrap(), original);
    let wrong = home.path().join("wrong");
    fs::create_dir(&wrong).unwrap();
    assert!(
        read(
            &home.path().join(".claude"),
            Provider::Claude,
            wrong.to_str().unwrap(),
            &id(1),
            None,
            None,
            None
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn claude_path_encoding_collision_does_not_authorize_another_directory() {
    let home = tempfile::tempdir().unwrap();
    let first = home.path().join("a-b");
    let second = home.path().join("a/b");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    let first = first.canonicalize().unwrap();
    let second = second.canonicalize().unwrap();
    assert_eq!(
        storage::encoded_claude_directory(&first).unwrap(),
        storage::encoded_claude_directory(&second).unwrap()
    );
    claude_transcript(home.path(), &first, 1, "");
    assert!(
        discover(
            &home.path().join(".claude"),
            Provider::Claude,
            second.to_str().unwrap(),
            None,
            None,
            None
        )
        .await
        .unwrap()
        .items
        .is_empty()
    );
    assert!(
        read(
            &home.path().join(".claude"),
            Provider::Claude,
            second.to_str().unwrap(),
            &id(1),
            None,
            None,
            None
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn conversation_cursors_are_exact_provider_scoped_and_bounded() {
    let home = tempfile::tempdir().unwrap();
    let directory = workspace(home.path());
    for index in 1..=3 {
        claude_transcript(home.path(), &directory, index, "");
    }
    let first = discover(
        &home.path().join(".claude"),
        Provider::Claude,
        directory.to_str().unwrap(),
        None,
        Some(1),
        None,
    )
    .await
    .unwrap();
    let second = discover(
        &home.path().join(".claude"),
        Provider::Claude,
        directory.to_str().unwrap(),
        first.next_cursor.as_deref(),
        Some(1),
        None,
    )
    .await
    .unwrap();
    assert_ne!(
        first.items[0].native_conversation_id,
        second.items[0].native_conversation_id
    );
    let cursor = format!("copilot:{}", id(1));
    assert!(
        discover(
            &home.path().join(".claude"),
            Provider::Claude,
            directory.to_str().unwrap(),
            Some(&cursor),
            None,
            None
        )
        .await
        .is_err()
    );
    assert!(
        discover(
            &home.path().join(".claude"),
            Provider::Claude,
            directory.to_str().unwrap(),
            None,
            Some(0),
            None
        )
        .await
        .is_err()
    );
    assert!(
        discover(
            &home.path().join(".claude"),
            Provider::Claude,
            directory.to_str().unwrap(),
            None,
            None,
            Some(1)
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn conversation_reader_rejects_symlink_files_and_parent_directories() {
    let home = tempfile::tempdir().unwrap();
    let directory = workspace(home.path());
    let path = claude_transcript(home.path(), &directory, 1, "");
    let moved = home.path().join("outside.jsonl");
    fs::rename(&path, &moved).unwrap();
    symlink(&moved, &path).unwrap();
    assert!(
        read(
            &home.path().join(".claude"),
            Provider::Claude,
            directory.to_str().unwrap(),
            &id(1),
            None,
            None,
            None
        )
        .await
        .is_err()
    );
    fs::remove_file(&path).unwrap();
    let project = path.parent().unwrap();
    let outside_directory = home.path().join("outside");
    fs::rename(project, &outside_directory).unwrap();
    fs::rename(&moved, outside_directory.join(path.file_name().unwrap())).unwrap();
    symlink(&outside_directory, project).unwrap();
    assert!(
        read(
            &home.path().join(".claude"),
            Provider::Claude,
            directory.to_str().unwrap(),
            &id(1),
            None,
            None,
            None
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn copilot_catalog_reader_and_resume_are_read_only() {
    let home = tempfile::tempdir().unwrap();
    let directory = workspace(home.path());
    let path = copilot_transcript(home.path(), &directory, 2, true);
    let bytes = fs::read(&path).unwrap();
    let database = home.path().join(".copilot/session-store.db");
    let db = Connection::open(&database).unwrap();
    db.execute_batch("UPDATE schema_version SET version = 8; ALTER TABLE sessions ADD COLUMN host_type TEXT; ALTER TABLE sessions ADD COLUMN repository TEXT; ALTER TABLE sessions ADD COLUMN branch TEXT;").unwrap();
    drop(db);
    let database_bytes = fs::read(&database).unwrap();
    let list = discover(
        &home.path().join(".copilot"),
        Provider::Copilot,
        directory.to_str().unwrap(),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(list.items.len(), 1);
    assert_eq!(list.items[0].title.as_deref(), Some("Example"));
    let page = read(
        &home.path().join(".copilot"),
        Provider::Copilot,
        directory.to_str().unwrap(),
        &id(2),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(page.entries[0].content, "hello");
    assert_eq!(
        validate_resume(
            &home.path().join(".copilot"),
            Provider::Copilot,
            directory.to_str().unwrap(),
            &id(2)
        )
        .await
        .unwrap(),
        directory
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::read(&database).unwrap(), database_bytes);
}

#[tokio::test]
async fn copilot_missing_columns_or_active_session_is_not_resumed() {
    let home = tempfile::tempdir().unwrap();
    let directory = workspace(home.path());
    copilot_transcript(home.path(), &directory, 1, false);
    assert!(
        validate_resume(
            &home.path().join(".copilot"),
            Provider::Copilot,
            directory.to_str().unwrap(),
            &id(1)
        )
        .await
        .unwrap_err()
        .contains("may still be open")
    );
    let db = Connection::open(home.path().join(".copilot/session-store.db")).unwrap();
    db.execute("ALTER TABLE sessions DROP COLUMN cwd", [])
        .unwrap();
    assert!(
        discover(
            &home.path().join(".copilot"),
            Provider::Copilot,
            directory.to_str().unwrap(),
            None,
            None,
            None
        )
        .await
        .unwrap_err()
        .contains("missing required conversation columns")
    );
}

#[tokio::test]
async fn copilot_resume_rejects_records_after_an_old_shutdown() {
    let home = tempfile::tempdir().unwrap();
    let directory = workspace(home.path());
    let path = copilot_transcript(home.path(), &directory, 1, true);
    let finished = fs::read_to_string(&path).unwrap();
    for tail in [
        "{\"type\":\"session.start\",\"data\":",
        "{\"type\":\"user.message\",\"data\":{\"content\":\"new work\"}}\n",
    ] {
        fs::write(&path, format!("{finished}{tail}")).unwrap();
        assert!(
            validate_resume(
                &home.path().join(".copilot"),
                Provider::Copilot,
                directory.to_str().unwrap(),
                &id(1)
            )
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn preview_rejects_decoded_response_amplification() {
    let home = tempfile::tempdir().unwrap();
    let directory = workspace(home.path());
    let record = serde_json::json!({
        "type":"assistant", "timestamp":"x".repeat(16*1024),
        "message":{"content":vec![serde_json::json!({"type":"tool_use","name":"x"});1024]}
    });
    claude_transcript(home.path(), &directory, 1, &record.to_string());
    let result = read(
        &home.path().join(".claude"),
        Provider::Claude,
        directory.to_str().unwrap(),
        &id(1),
        None,
        Some(10),
        Some(128 * 1024),
    )
    .await;
    assert!(result.unwrap_err().contains("preview byte limit"));
}

#[tokio::test]
async fn copilot_resume_uses_workspace_metadata_when_start_record_has_no_cwd() {
    let home = tempfile::tempdir().unwrap();
    let directory = workspace(home.path());
    let path = copilot_transcript(home.path(), &directory, 1, true);
    fs::write(
        &path,
        "{\"type\":\"session.start\",\"data\":{}}\n{\"type\":\"session.shutdown\"}\n",
    )
    .unwrap();
    fs::write(
        path.parent().unwrap().join("workspace.yaml"),
        format!("cwd: '{}'\n", directory.display()),
    )
    .unwrap();
    assert_eq!(
        validate_resume(
            &home.path().join(".copilot"),
            Provider::Copilot,
            directory.to_str().unwrap(),
            &id(1)
        )
        .await
        .unwrap(),
        directory
    );
}

#[tokio::test]
async fn missing_copilot_index_is_not_created() {
    let home = tempfile::tempdir().unwrap();
    let directory = workspace(home.path());
    assert!(
        discover(
            &home.path().join(".copilot"),
            Provider::Copilot,
            directory.to_str().unwrap(),
            None,
            None,
            None
        )
        .await
        .unwrap()
        .items
        .is_empty()
    );
    assert!(!home.path().join(".copilot").exists());
}

#[tokio::test]
async fn active_claude_native_identity_is_not_resumed() {
    let home = tempfile::tempdir().unwrap();
    let directory = workspace(home.path());
    claude_transcript(home.path(), &directory, 1, "");
    assert_eq!(
        validate_resume(
            &home.path().join(".claude"),
            Provider::Claude,
            directory.to_str().unwrap(),
            &id(1)
        )
        .await
        .unwrap(),
        directory
    );
    let sessions = home.path().join(".claude/sessions");
    fs::create_dir_all(&sessions).unwrap();
    fs::write(
        sessions.join("active.json"),
        serde_json::to_vec(
            &serde_json::json!({"sessionId":id(1),"cwd":directory,"pid":std::process::id()}),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        validate_resume(
            &home.path().join(".claude"),
            Provider::Claude,
            directory.to_str().unwrap(),
            &id(1)
        )
        .await
        .unwrap_err()
        .contains("may already be open")
    );
}

#[test]
fn paging_is_byte_based_and_does_not_repeat_records() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let records = ["one", "two", "three"]
        .map(|text| format!("{{\"type\":\"user\",\"message\":{{\"content\":\"{text}\"}}}}\n"));
    fs::write(file.path(), records.concat()).unwrap();
    let mut cursor = None;
    for text in ["three", "two", "one"] {
        let page = read::page(
            File::open(file.path()).unwrap(),
            Provider::Claude,
            cursor,
            Some(1),
            Some(4096),
        )
        .unwrap();
        assert_eq!(page.entries[0].content, text);
        assert_eq!(page.source_records, 1);
        cursor = page.next_before_byte;
    }
    assert!(cursor.is_none());
    let page = read::page(
        File::open(file.path()).unwrap(),
        Provider::Claude,
        None,
        Some(1),
        Some(records[2].len()),
    )
    .unwrap();
    assert_eq!(page.entries[0].content, "three");
}

#[test]
fn page_keeps_complete_records_before_an_incomplete_tail() {
    let file = tempfile::NamedTempFile::new().unwrap();
    fs::write(file.path(), "{\"type\":\"user\",\"message\":{\"content\":\"complete\"}}\n{\"type\":\"user\",\"message\":").unwrap();
    let page = read::page(
        File::open(file.path()).unwrap(),
        Provider::Claude,
        None,
        None,
        None,
    )
    .unwrap();
    assert_eq!(page.entries[0].content, "complete");
    fs::write(
        file.path(),
        "{\"type\":\"user\",\"message\":{\"content\":\"valid eof\"}}",
    )
    .unwrap();
    let page = read::page(
        File::open(file.path()).unwrap(),
        Provider::Claude,
        None,
        None,
        None,
    )
    .unwrap();
    assert_eq!(page.entries[0].content, "valid eof");
}

#[test]
fn huge_transcript_only_reads_a_bounded_window() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut writer = file.reopen().unwrap();
    writer
        .seek(SeekFrom::Start(2 * 1024 * 1024 * 1024))
        .unwrap();
    writer
        .write_all(b"\n{\"type\":\"user\",\"message\":{\"content\":\"tail\"}}\n")
        .unwrap();
    let page = read::page(
        file.reopen().unwrap(),
        Provider::Claude,
        None,
        None,
        Some(1024),
    )
    .unwrap();
    assert_eq!(page.entries[0].content, "tail");
    assert_eq!(page.read_bytes, 1024);
    assert!(page.next_before_byte.is_some());
}

#[test]
fn oversized_record_advances_with_an_explicit_notice() {
    let file = tempfile::NamedTempFile::new().unwrap();
    fs::write(file.path(), vec![b'x'; 4096]).unwrap();
    let page = read::page(
        file.reopen().unwrap(),
        Provider::Claude,
        None,
        None,
        Some(128),
    )
    .unwrap();
    assert!(page.entries.is_empty());
    assert_eq!(page.next_before_byte, Some(4096 - 128));
    assert!(page.degraded_reason.is_some());
    assert!(
        read::page(
            file.reopen().unwrap(),
            Provider::Claude,
            Some(9999),
            None,
            None
        )
        .is_err()
    );
}

#[test]
fn decoders_preserve_native_content_and_bound_tool_summaries() {
    let claude = "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"first\"},{\"type\":\"text\",\"text\":\"second\"}]}}\n{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"name\":\"Bash\",\"input\":{\"command\":\"API_TOKEN=secret curl Bearer hidden\"}}]}}\n";
    let entries = decode::conversation(Provider::Claude, claude).unwrap();
    assert_eq!(entries[0].content, "first\nsecond");
    assert!(!entries[1].content.contains("secret"));
    assert!(!entries[1].content.contains("hidden"));
    let copilot = "{\"type\":\"assistant.message\",\"data\":{\"content\":\"working\",\"toolRequests\":[{\"toolCallId\":\"a\",\"name\":\"bash\",\"arguments\":{\"command\":\"ls\"}}]}}\n{\"type\":\"tool.execution_start\",\"data\":{\"toolCallId\":\"a\",\"toolName\":\"bash\",\"arguments\":{\"command\":\"ls\"}}}\n";
    assert_eq!(
        decode::conversation(Provider::Copilot, copilot)
            .unwrap()
            .len(),
        2
    );
    assert!(decode::conversation(Provider::Claude, "not-json\n").is_err());
    let unicode = serde_json::json!({"type":"tool.execution_start","data":{"toolName":"bash","arguments":{"command":"你".repeat(2000)}}});
    let rows = decode::conversation(Provider::Copilot, &unicode.to_string()).unwrap();
    assert!(rows[0].content.len() <= 1027);
    assert!(rows[0].content.ends_with('…'));
}

#[test]
fn config_inspection_never_creates_or_edits_provider_files() {
    let home = tempfile::tempdir().unwrap();
    let directory = workspace(home.path());
    let user = home.path().join(".claude/settings.json");
    fs::create_dir_all(user.parent().unwrap()).unwrap();
    fs::write(&user, b"{\"permissions\":{}}").unwrap();
    let info = config::inspect(
        &home.path().join(".claude"),
        Provider::Claude,
        Some(&directory),
    );
    assert!(info.files[0].exists);
    assert_eq!(
        info.files[0].path,
        user.canonicalize().unwrap().to_string_lossy()
    );
    assert_eq!(fs::read(&user).unwrap(), b"{\"permissions\":{}}");
    assert!(!directory.join(".claude").exists());
    assert!(
        serde_json::to_value(&info)
            .unwrap()
            .get("version")
            .is_none()
    );
}

#[test]
fn effective_provider_roots_are_explicit_read_only_paths() {
    let home = tempfile::tempdir().unwrap();
    for (provider, name) in [
        (Provider::Claude, ".claude"),
        (Provider::Copilot, ".copilot"),
    ] {
        assert_eq!(
            storage::resolve_provider_root(home.path(), provider, None).unwrap(),
            home.path().join(name)
        );
        let native = home.path().join("native");
        let root = storage::resolve_provider_root(home.path(), provider, Some(native.as_os_str()))
            .unwrap();
        assert_eq!(root, native);
        let info = config::inspect(&root, provider, None);
        assert_eq!(Path::new(&info.files[0].path), root.join("settings.json"));
        assert!(!root.exists());
        for invalid in ["relative", "/tmp/../outside", "/tmp/bad\nroot"] {
            assert!(
                storage::resolve_provider_root(home.path(), provider, Some(invalid.as_ref()))
                    .is_err()
            );
        }
    }
}

#[tokio::test]
async fn native_state_override_is_shared_by_discovery_read_and_resume() {
    let home = tempfile::tempdir().unwrap();
    let directory = workspace(home.path());
    claude_transcript(
        home.path(),
        &directory,
        1,
        "{\"type\":\"user\",\"message\":{\"content\":\"native\"}}\n",
    );
    let native = home.path().join("custom-native-state");
    fs::rename(home.path().join(".claude"), &native).unwrap();
    let root =
        storage::resolve_provider_root(home.path(), Provider::Claude, Some(native.as_os_str()))
            .unwrap();
    assert_eq!(
        discover(
            &root,
            Provider::Claude,
            directory.to_str().unwrap(),
            None,
            None,
            None
        )
        .await
        .unwrap()
        .items
        .len(),
        1
    );
    assert_eq!(
        read(
            &root,
            Provider::Claude,
            directory.to_str().unwrap(),
            &id(1),
            None,
            None,
            None
        )
        .await
        .unwrap()
        .entries[0]
            .content,
        "native"
    );
    validate_resume(&root, Provider::Claude, directory.to_str().unwrap(), &id(1))
        .await
        .unwrap();
    assert!(!home.path().join(".claude").exists());
}

#[test]
fn executable_resolution_requires_effective_execute_permission() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("claude");
    fs::write(&path, b"#!/bin/sh\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(config::executable(&path).is_none());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        config::executable(&path),
        Some(path.canonicalize().unwrap())
    );
}

#[test]
fn path_inputs_reject_traversal_relative_controls_and_invalid_ids() {
    for path in ["relative", "/tmp/../etc", "/tmp/\nname", ""] {
        assert!(storage::canonical_directory(path).is_err());
    }
    for value in [
        "../secret",
        "00000000-0000-0000-0000-000000000000",
        "a",
        "550E8400-E29B-41D4-A716-446655440000",
    ] {
        assert!(storage::conversation_id(value).is_err());
    }
}

use std::fs::File;

#[tokio::test]
async fn all_project_history_and_large_file_pages_are_bounded() {
    let home = tempfile::tempdir().unwrap();
    let first = workspace(home.path());
    let second = home.path().join("second");
    fs::create_dir(&second).unwrap();
    let path = claude_transcript(home.path(), &first, 201, "");
    let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
    file.set_len(100 * 1024 * 1024).unwrap();
    file.write_all(b"\n{\"type\":\"user\",\"message\":{\"content\":\"Recent message\"}}\n")
        .unwrap();
    claude_transcript(home.path(), &second, 202, "");
    let page = discover_history(
        &home.path().join(".claude"),
        Provider::Claude,
        None,
        None,
        Some(1),
        Some(16384),
    )
    .await
    .unwrap();
    assert_eq!(page.items.len(), 1);
    assert!(page.has_more);
    let next = discover_history(
        &home.path().join(".claude"),
        Provider::Claude,
        None,
        page.next_cursor.as_deref(),
        Some(1),
        Some(16384),
    )
    .await
    .unwrap();
    assert_eq!(next.items.len(), 1);
    assert_ne!(
        page.items[0].native_conversation_id,
        next.items[0].native_conversation_id
    );
    let read = read(
        &home.path().join(".claude"),
        Provider::Claude,
        first.to_str().unwrap(),
        &id(201),
        None,
        Some(50),
        Some(131_072),
    )
    .await
    .unwrap();
    assert!(read.source_file_bytes >= 100 * 1024 * 1024);
    assert!(read.read_bytes <= 131_072);
    assert_eq!(read.entries[0].content, "Recent message");
}

#[tokio::test]
async fn malformed_history_ids_still_advance_the_cursor() {
    let home = tempfile::tempdir().unwrap();
    let directory = workspace(home.path());
    copilot_transcript(home.path(), &directory, 10, true);
    copilot_transcript(home.path(), &directory, 20, true);
    let database = Connection::open(home.path().join(".copilot/session-store.db")).unwrap();
    database
        .execute("UPDATE sessions SET id = 'invalid' WHERE id = ?", [id(10)])
        .unwrap();
    database
        .execute(
            "UPDATE sessions SET updated_at = '9999' WHERE id = 'invalid'",
            [],
        )
        .unwrap();
    drop(database);
    let first = discover_history(
        &home.path().join(".copilot"),
        Provider::Copilot,
        None,
        None,
        Some(1),
        None,
    )
    .await
    .unwrap();
    assert!(first.items.is_empty());
    assert!(first.has_more);
    let next = discover_history(
        &home.path().join(".copilot"),
        Provider::Copilot,
        None,
        first.next_cursor.as_deref(),
        Some(1),
        None,
    )
    .await
    .unwrap();
    assert_eq!(next.items[0].native_conversation_id, id(20));
}

#[tokio::test]
async fn history_paging_does_not_repeat_entries_when_new_conversations_arrive() {
    let home = tempfile::tempdir().unwrap();
    let directory = workspace(home.path());
    copilot_transcript(home.path(), &directory, 10, true);
    copilot_transcript(home.path(), &directory, 20, true);
    let first = discover_history(
        &home.path().join(".copilot"),
        Provider::Copilot,
        None,
        None,
        Some(1),
        None,
    )
    .await
    .unwrap();
    assert_eq!(first.items[0].native_conversation_id, id(10));
    copilot_transcript(home.path(), &directory, 1, true);
    let second = discover_history(
        &home.path().join(".copilot"),
        Provider::Copilot,
        None,
        first.next_cursor.as_deref(),
        Some(1),
        None,
    )
    .await
    .unwrap();
    assert_eq!(second.items[0].native_conversation_id, id(20));
}

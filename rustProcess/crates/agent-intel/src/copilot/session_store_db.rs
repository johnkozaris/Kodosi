use std::path::PathBuf;

use rusqlite::{Connection, ErrorCode, OpenFlags};
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const SUPPORTED_SCHEMA_VERSIONS: &[i64] = &[2, 3];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStoreRead {
    Missing,
    State(SessionStoreState),
    Degraded(crate::domain::DegradationNotice),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStoreState {
    pub schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_host_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_repository: Option<String>,
    pub repo_total_sessions: u64,
    pub repo_total_turns: u64,
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub additional_fields: BTreeMap<String, Value>,
}

impl SessionStoreState {
    #[must_use]
    pub const fn has_content(&self) -> bool {
        self.current_summary.is_some()
            || self.current_host_type.is_some()
            || self.current_created_at.is_some()
            || self.current_repository.is_some()
            || self.repo_total_sessions > 0
            || self.repo_total_turns > 0
    }
}

const MAX_REPOSITORIES_RETURNED: i64 = 1000;

const MAX_SESSIONS_PER_REPO_RETURNED: i64 = 500;
const MAX_SESSIONS_PER_DIRECTORY_RETURNED: i64 = 4_096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryRow {
    pub repository: String,
    pub session_count: u64,
    pub turn_count: u64,
    pub last_seen: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoSessionRow {
    pub id: String,
    pub summary: Option<String>,
    pub host_type: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CopilotSessionStore {
    db_path: PathBuf,
}

impl CopilotSessionStore {
    #[must_use]
    pub const fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    fn open_readonly(&self) -> Result<Option<Connection>, crate::domain::DegradationNotice> {
        match std::fs::metadata(&self.db_path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(store_failure(
                    crate::domain::CopilotSessionStoreFailureKind::Open,
                    error.to_string(),
                ));
            }
        }
        let conn = Connection::open_with_flags(
            &self.db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_PRIVATE_CACHE,
        )
        .map_err(|error| classify_sqlite_failure(&error, StoreStage::Open))?;
        conn.busy_timeout(std::time::Duration::from_millis(250))
            .map_err(|error| classify_sqlite_failure(&error, StoreStage::Open))?;
        Ok(Some(conn))
    }

    fn check_schema_version(tx: &rusqlite::Transaction<'_>) -> Result<u32, SchemaFailure> {
        match tx.query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
            row.get::<_, i64>(0)
        }) {
            Ok(v) if SUPPORTED_SCHEMA_VERSIONS.contains(&v) => {
                u32::try_from(v).map_err(|_| SchemaFailure::Unsupported(v))
            }
            Ok(v) => Err(SchemaFailure::Unsupported(v)),
            Err(error) => Err(SchemaFailure::Sql(error)),
        }
    }

    pub fn read_state_with_notice(&self, session_id: &str) -> SessionStoreRead {
        let mut conn = match self.open_readonly() {
            Ok(Some(conn)) => conn,
            Ok(None) => return SessionStoreRead::Missing,
            Err(notice) => return SessionStoreRead::Degraded(notice),
        };
        let tx = match conn.transaction_with_behavior(rusqlite::TransactionBehavior::Deferred) {
            Ok(tx) => tx,
            Err(error) => {
                return SessionStoreRead::Degraded(classify_sqlite_failure(
                    &error,
                    StoreStage::Query,
                ));
            }
        };

        let schema_version = match Self::check_schema_version(&tx) {
            Ok(version) => version,
            Err(SchemaFailure::Unsupported(version)) => {
                return SessionStoreRead::Degraded(
                    crate::domain::DegradationNotice::UnsupportedCopilotSessionStore {
                        schema_version: Some(version),
                        supported_versions: SUPPORTED_SCHEMA_VERSIONS
                            .iter()
                            .filter_map(|version| u32::try_from(*version).ok())
                            .collect(),
                    },
                );
            }
            Err(SchemaFailure::Sql(error)) => {
                return SessionStoreRead::Degraded(classify_sqlite_failure(
                    &error,
                    StoreStage::Schema,
                ));
            }
        };

        let current = match tx.query_row(
            "SELECT summary, host_type, created_at, repository \
             FROM sessions WHERE id = ? LIMIT 1",
            [session_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        ) {
            Ok(row) => row,
            Err(rusqlite::Error::QueryReturnedNoRows) => (None, None, None, None),
            Err(error) => {
                return SessionStoreRead::Degraded(classify_sqlite_failure(
                    &error,
                    StoreStage::Query,
                ));
            }
        };
        let (current_summary, current_host_type, current_created_at, current_repository) = current;

        let (repo_total_sessions, repo_total_turns) = match current_repository.as_deref() {
            Some(repo) if !repo.is_empty() => {
                let sessions_count = match tx.query_row(
                    "SELECT COUNT(*) FROM sessions WHERE repository = ?",
                    [repo],
                    |row| row.get::<_, i64>(0),
                ) {
                    Ok(v) => v.max(0).cast_unsigned(),
                    Err(error) => {
                        return SessionStoreRead::Degraded(classify_sqlite_failure(
                            &error,
                            StoreStage::Query,
                        ));
                    }
                };

                let turns_count = match tx.query_row(
                    "SELECT COUNT(t.id) FROM turns t \
                     JOIN sessions s ON s.id = t.session_id \
                     WHERE s.repository = ?",
                    [repo],
                    |row| row.get::<_, i64>(0),
                ) {
                    Ok(v) => v.max(0).cast_unsigned(),
                    Err(error) => {
                        return SessionStoreRead::Degraded(classify_sqlite_failure(
                            &error,
                            StoreStage::Query,
                        ));
                    }
                };

                (sessions_count, turns_count)
            }
            _ => (0, 0),
        };

        if let Err(error) = tx.commit() {
            return SessionStoreRead::Degraded(classify_sqlite_failure(&error, StoreStage::Query));
        }

        SessionStoreRead::State(SessionStoreState {
            schema_version,
            current_summary,
            current_host_type,
            current_created_at,
            current_repository,
            repo_total_sessions,
            repo_total_turns,
            additional_fields: BTreeMap::new(),
        })
    }

    pub fn list_repositories(&self) -> Option<Vec<RepositoryRow>> {
        let mut conn = self.open_readonly().ok()??;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)
            .ok()?;
        Self::check_schema_version(&tx).ok()?;

        let mut stmt = tx
            .prepare(
                "SELECT s.repository, \
                        COUNT(s.id), \
                        COALESCE(SUM(t.n), 0), \
                        MAX(COALESCE(s.updated_at, s.created_at)) \
                 FROM sessions s \
                 LEFT JOIN ( \
                     SELECT session_id, COUNT(*) AS n FROM turns \
                     WHERE session_id IN ( \
                         SELECT id FROM sessions \
                         WHERE repository IS NOT NULL AND repository != '' \
                     ) \
                     GROUP BY session_id \
                 ) t ON s.id = t.session_id \
                 WHERE s.repository IS NOT NULL AND s.repository != '' \
                 GROUP BY s.repository \
                 ORDER BY MAX(COALESCE(s.updated_at, s.created_at)) DESC \
                 LIMIT ?",
            )
            .ok()?;

        let rows: Vec<RepositoryRow> = stmt
            .query_map(rusqlite::params![MAX_REPOSITORIES_RETURNED], |row| {
                Ok(RepositoryRow {
                    repository: row.get::<_, String>(0)?,
                    session_count: row.get::<_, i64>(1)?.max(0).cast_unsigned(),
                    turn_count: row.get::<_, i64>(2)?.max(0).cast_unsigned(),
                    last_seen: row.get::<_, Option<String>>(3)?,
                })
            })
            .ok()?
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        drop(stmt);

        drop(tx.commit());
        Some(rows)
    }

    pub fn list_sessions_for_repository(&self, repository: &str) -> Option<Vec<RepoSessionRow>> {
        let mut conn = self.open_readonly().ok()??;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)
            .ok()?;
        Self::check_schema_version(&tx).ok()?;

        let mut stmt = tx
            .prepare(
                "SELECT id, summary, host_type, created_at, updated_at \
                 FROM sessions \
                 WHERE repository = ? \
                 ORDER BY COALESCE(updated_at, created_at) DESC \
                 LIMIT ?",
            )
            .ok()?;

        let rows: Vec<RepoSessionRow> = stmt
            .query_map(
                rusqlite::params![repository, MAX_SESSIONS_PER_REPO_RETURNED],
                |row| {
                    Ok(RepoSessionRow {
                        id: row.get::<_, String>(0)?,
                        summary: row.get::<_, Option<String>>(1)?,
                        host_type: row.get::<_, Option<String>>(2)?,
                        created_at: row.get::<_, Option<String>>(3)?,
                        updated_at: row.get::<_, Option<String>>(4)?,
                    })
                },
            )
            .ok()?
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        drop(stmt);

        drop(tx.commit());
        Some(rows)
    }

    pub fn list_sessions_for_directory(&self, cwd: &str) -> Option<Vec<RepoSessionRow>> {
        let mut conn = self.open_readonly().ok()??;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)
            .ok()?;
        Self::check_schema_version(&tx).ok()?;

        let mut stmt = tx
            .prepare(
                "SELECT substr(id, 1, 64), substr(summary, 1, 1024),
                        substr(host_type, 1, 128), substr(created_at, 1, 128),
                        substr(updated_at, 1, 128) \
                 FROM sessions \
                 WHERE cwd = ? \
                 ORDER BY COALESCE(updated_at, created_at) DESC, id ASC \
                 LIMIT ?",
            )
            .ok()?;
        let rows = stmt
            .query_map(
                rusqlite::params![cwd, MAX_SESSIONS_PER_DIRECTORY_RETURNED],
                |row| {
                    Ok(RepoSessionRow {
                        id: row.get::<_, String>(0)?,
                        summary: row.get::<_, Option<String>>(1)?,
                        host_type: row.get::<_, Option<String>>(2)?,
                        created_at: row.get::<_, Option<String>>(3)?,
                        updated_at: row.get::<_, Option<String>>(4)?,
                    })
                },
            )
            .ok()?
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        drop(stmt);
        drop(tx.commit());
        Some(rows)
    }

    pub fn session_directory(&self, session_id: &str) -> Option<String> {
        let mut conn = self.open_readonly().ok()??;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)
            .ok()?;
        Self::check_schema_version(&tx).ok()?;
        let (length, cwd) = tx
            .query_row(
                "SELECT length(CAST(cwd AS BLOB)),
                        CASE WHEN length(CAST(cwd AS BLOB)) <= 4096 THEN cwd ELSE NULL END
                 FROM sessions WHERE id = ? LIMIT 1",
                rusqlite::params![session_id],
                |row| {
                    Ok((
                        row.get::<_, Option<i64>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .ok()?;
        if length.is_none_or(|length| !(1..=4_096).contains(&length)) {
            return None;
        }
        let cwd = cwd?;
        drop(tx.commit());
        Some(cwd)
    }
}

enum SchemaFailure {
    Unsupported(i64),
    Sql(rusqlite::Error),
}

#[derive(Clone, Copy)]
enum StoreStage {
    Open,
    Schema,
    Query,
}

fn classify_sqlite_failure(
    error: &rusqlite::Error,
    stage: StoreStage,
) -> crate::domain::DegradationNotice {
    let failure = match error {
        rusqlite::Error::SqliteFailure(sqlite, _) => match sqlite.code {
            ErrorCode::DatabaseBusy
            | ErrorCode::DatabaseLocked
            | ErrorCode::FileLockingProtocolFailed => {
                crate::domain::CopilotSessionStoreFailureKind::Locked
            }
            ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase => {
                crate::domain::CopilotSessionStoreFailureKind::Corrupt
            }
            ErrorCode::SchemaChanged => crate::domain::CopilotSessionStoreFailureKind::Schema,
            _ => fallback_failure(stage),
        },
        _ => fallback_failure(stage),
    };
    store_failure(failure, error.to_string())
}

const fn fallback_failure(stage: StoreStage) -> crate::domain::CopilotSessionStoreFailureKind {
    match stage {
        StoreStage::Open => crate::domain::CopilotSessionStoreFailureKind::Open,
        StoreStage::Schema => crate::domain::CopilotSessionStoreFailureKind::Schema,
        StoreStage::Query => crate::domain::CopilotSessionStoreFailureKind::Query,
    }
}

fn store_failure(
    failure: crate::domain::CopilotSessionStoreFailureKind,
    message: String,
) -> crate::domain::DegradationNotice {
    crate::domain::DegradationNotice::CopilotSessionStoreFailure { failure, message }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use std::fs;

    fn seed_schema(conn: &Connection) {
        conn.execute_batch(
            r"
            CREATE TABLE schema_version (version INTEGER NOT NULL);
            INSERT INTO schema_version(version) VALUES (2);

            CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                cwd TEXT,
                repository TEXT,
                branch TEXT,
                summary TEXT,
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now')),
                host_type TEXT
            );
            CREATE INDEX idx_sessions_repo ON sessions(repository);

            CREATE TABLE turns (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES sessions(id),
                turn_index INTEGER NOT NULL,
                user_message TEXT,
                assistant_response TEXT,
                timestamp TEXT DEFAULT (datetime('now')),
                UNIQUE(session_id, turn_index)
            );
            CREATE INDEX idx_turns_session ON turns(session_id);
            ",
        )
        .unwrap();
    }

    fn mk_store(dir: &tempfile::TempDir, name: &str) -> (CopilotSessionStore, PathBuf) {
        let path = dir.path().join(name);
        let conn = Connection::open(&path).unwrap();
        seed_schema(&conn);
        drop(conn);
        (CopilotSessionStore::new(path.clone()), path)
    }

    fn state(store: &CopilotSessionStore, session_id: &str) -> SessionStoreState {
        match store.read_state_with_notice(session_id) {
            SessionStoreRead::State(state) => state,
            other => panic!("expected session store state, got {other:?}"),
        }
    }

    #[test]
    fn missing_db_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = CopilotSessionStore::new(dir.path().join("does-not-exist.db"));
        assert_eq!(
            store.read_state_with_notice("any-id"),
            SessionStoreRead::Missing
        );
    }

    #[test]
    fn schema_version_mismatch_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL); \
             INSERT INTO schema_version(version) VALUES (99);",
        )
        .unwrap();
        drop(conn);
        let store = CopilotSessionStore::new(path);
        std::assert_matches!(
            store.read_state_with_notice("any-id"),
            SessionStoreRead::Degraded(
                crate::domain::DegradationNotice::UnsupportedCopilotSessionStore {
                    schema_version: Some(99),
                    supported_versions,
                }
            ) if supported_versions == vec![2, 3]
        );
    }

    #[test]
    fn missing_schema_table_is_typed_degradation_not_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.db");
        let conn = Connection::open(&path).unwrap();
        drop(conn);
        let store = CopilotSessionStore::new(path);
        std::assert_matches!(
            store.read_state_with_notice("any-id"),
            SessionStoreRead::Degraded(
                crate::domain::DegradationNotice::CopilotSessionStoreFailure {
                    failure: crate::domain::CopilotSessionStoreFailureKind::Schema,
                    ..
                }
            )
        );
    }

    #[test]
    fn happy_path_populates_current_and_repo_counts() {
        let dir = tempfile::tempdir().unwrap();
        let (store, path) = mk_store(&dir, "happy.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO sessions(id, repository, summary, host_type, created_at) \
             VALUES (?, ?, ?, ?, ?)",
            params![
                "session-a",
                "owner/repo",
                "My summary",
                "terminal",
                "2026-04-25T00:00:00Z",
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sessions(id, repository, summary, host_type) \
             VALUES ('session-b', 'owner/repo', NULL, NULL)",
            [],
        )
        .unwrap();
        for i in 0..5 {
            conn.execute(
                "INSERT INTO turns(session_id, turn_index) VALUES ('session-a', ?)",
                params![i],
            )
            .unwrap();
        }
        for i in 0..3 {
            conn.execute(
                "INSERT INTO turns(session_id, turn_index) VALUES ('session-b', ?)",
                params![i],
            )
            .unwrap();
        }
        drop(conn);

        let state = state(&store, "session-a");
        assert_eq!(state.schema_version, 2);
        assert_eq!(state.current_summary.as_deref(), Some("My summary"));
        assert_eq!(state.current_host_type.as_deref(), Some("terminal"));
        assert_eq!(
            state.current_created_at.as_deref(),
            Some("2026-04-25T00:00:00Z")
        );
        assert_eq!(state.current_repository.as_deref(), Some("owner/repo"));
        assert_eq!(state.repo_total_sessions, 2);
        assert_eq!(state.repo_total_turns, 8);
        assert!(state.has_content());
    }

    #[test]
    fn session_without_repository_skips_aggregates() {
        let dir = tempfile::tempdir().unwrap();
        let (store, path) = mk_store(&dir, "no-repo.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO sessions(id, summary) VALUES ('session-a', 'hi')",
            [],
        )
        .unwrap();
        drop(conn);

        let state = state(&store, "session-a");
        assert_eq!(state.current_summary.as_deref(), Some("hi"));
        assert!(state.current_repository.is_none());
        assert_eq!(state.repo_total_sessions, 0);
        assert_eq!(state.repo_total_turns, 0);
    }

    #[test]
    fn unknown_session_id_still_returns_schema_and_empty_current() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = mk_store(&dir, "unknown.db");
        let state = state(&store, "ghost-id");
        assert_eq!(state.schema_version, 2);
        assert!(state.current_summary.is_none());
        assert!(state.current_host_type.is_none());
        assert!(state.current_repository.is_none());
        assert_eq!(state.repo_total_sessions, 0);
        assert_eq!(state.repo_total_turns, 0);
        assert!(!state.has_content());
    }

    #[test]
    fn schema_version_three_is_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v3.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            r"
            CREATE TABLE schema_version (version INTEGER NOT NULL);
            INSERT INTO schema_version(version) VALUES (3);

            CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                cwd TEXT,
                repository TEXT,
                branch TEXT,
                summary TEXT,
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now')),
                host_type TEXT
            );
            CREATE TABLE turns (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES sessions(id),
                turn_index INTEGER NOT NULL,
                user_message TEXT,
                assistant_response TEXT,
                timestamp TEXT DEFAULT (datetime('now')),
                UNIQUE(session_id, turn_index)
            );

            INSERT INTO sessions(id, repository, summary)
                VALUES ('s1', 'owner/repo', 'hello');
            INSERT INTO turns(session_id, turn_index)
                VALUES ('s1', 0);
            ",
        )
        .unwrap();
        drop(conn);
        let store = CopilotSessionStore::new(path);

        let state = state(&store, "s1");
        assert_eq!(state.schema_version, 3);
        assert_eq!(state.current_summary.as_deref(), Some("hello"));
        assert_eq!(state.current_repository.as_deref(), Some("owner/repo"));

        let repos = store
            .list_repositories()
            .expect("list_repositories must work on v3");
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].repository, "owner/repo");

        let sessions = store
            .list_sessions_for_repository("owner/repo")
            .expect("list_sessions_for_repository must work on v3");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "s1");
    }

    #[test]
    fn sql_injection_payload_is_parameterised_safely() {
        let dir = tempfile::tempdir().unwrap();
        let (store, path) = mk_store(&dir, "inj.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO sessions(id, repository, summary) \
             VALUES ('session-a', 'owner/repo', 'legitimate summary')",
            [],
        )
        .unwrap();
        drop(conn);

        let malicious = "session-a' OR 1=1 --";
        let state = state(&store, malicious);
        assert!(
            state.current_summary.is_none(),
            "parameterised query must treat injection payload as a literal id with no match"
        );
        assert_eq!(state.repo_total_sessions, 0);
        assert_eq!(state.repo_total_turns, 0);
    }

    #[test]
    fn corrupt_db_file_returns_typed_degradation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt.db");
        fs::write(&path, b"not a sqlite file").unwrap();
        let store = CopilotSessionStore::new(path);
        std::assert_matches!(
            store.read_state_with_notice("session-a"),
            SessionStoreRead::Degraded(
                crate::domain::DegradationNotice::CopilotSessionStoreFailure {
                    failure: crate::domain::CopilotSessionStoreFailureKind::Corrupt,
                    ..
                }
            )
        );
    }

    #[test]
    fn supported_version_with_missing_sessions_table_is_query_degradation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing-sessions.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL); \
             INSERT INTO schema_version(version) VALUES (3);",
        )
        .unwrap();
        drop(conn);
        let store = CopilotSessionStore::new(path);
        std::assert_matches!(
            store.read_state_with_notice("session-a"),
            SessionStoreRead::Degraded(
                crate::domain::DegradationNotice::CopilotSessionStoreFailure {
                    failure: crate::domain::CopilotSessionStoreFailureKind::Query,
                    ..
                }
            )
        );
    }

    #[test]
    fn locked_database_returns_typed_degradation() {
        let dir = tempfile::tempdir().unwrap();
        let (store, path) = mk_store(&dir, "locked.db");
        let locker = Connection::open(&path).unwrap();
        locker.execute_batch("BEGIN EXCLUSIVE;").unwrap();

        std::assert_matches!(
            store.read_state_with_notice("session-a"),
            SessionStoreRead::Degraded(
                crate::domain::DegradationNotice::CopilotSessionStoreFailure {
                    failure: crate::domain::CopilotSessionStoreFailureKind::Locked,
                    ..
                }
            )
        );
        locker.execute_batch("ROLLBACK;").unwrap();
    }

    #[test]
    fn read_side_is_safe_against_concurrent_writer() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        use std::thread;

        let dir = tempfile::tempdir().unwrap();
        let (store, path) = mk_store(&dir, "concurrent.db");
        let stop = Arc::new(AtomicBool::new(false));

        let writer_path = path;
        let writer_stop = Arc::clone(&stop);
        let writer = thread::spawn(move || {
            let conn = Connection::open(&writer_path).unwrap();
            conn.execute_batch("PRAGMA journal_mode = WAL;").unwrap();
            conn.execute(
                "INSERT INTO sessions(id, repository, summary) \
                 VALUES ('session-a', 'owner/repo', 'initial')",
                [],
            )
            .unwrap();
            let mut n = 0_i64;
            while !writer_stop.load(Ordering::Relaxed) {
                drop(conn.execute(
                    "INSERT INTO turns(session_id, turn_index) VALUES ('session-a', ?)",
                    params![n],
                ));
                n = n.wrapping_add(1);
            }
        });

        for _ in 0..500 {
            drop(store.read_state_with_notice("session-a"));
        }

        stop.store(true, Ordering::Relaxed);
        writer.join().unwrap();
    }

    #[test]
    fn list_repositories_excludes_null_and_empty_repo_rows() {
        let dir = tempfile::tempdir().unwrap();
        let (store, path) = mk_store(&dir, "store.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO sessions(id, repository) VALUES ('s1', NULL)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO sessions(id, repository) VALUES ('s2', '')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO sessions(id, repository) VALUES ('s3', 'owner/repo')",
            [],
        )
        .unwrap();
        drop(conn);

        let rows = store.list_repositories().expect("must succeed");
        assert_eq!(rows.len(), 1, "null + empty repos must be excluded");
        assert_eq!(rows[0].repository, "owner/repo");
        assert_eq!(rows[0].session_count, 1);
        assert_eq!(rows[0].turn_count, 0);
    }

    #[test]
    fn list_repositories_aggregates_counts_per_repo() {
        let dir = tempfile::tempdir().unwrap();
        let (store, path) = mk_store(&dir, "store.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO sessions(id, repository, updated_at) VALUES ('a1', 'owner/a', '2026-04-23T10:00:00Z')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO sessions(id, repository, updated_at) VALUES ('a2', 'owner/a', '2026-04-23T11:00:00Z')",
            [],
        ).unwrap();
        for i in 0..3 {
            conn.execute(
                "INSERT INTO turns(session_id, turn_index) VALUES ('a1', ?)",
                params![i],
            )
            .unwrap();
        }
        for i in 0..2 {
            conn.execute(
                "INSERT INTO turns(session_id, turn_index) VALUES ('a2', ?)",
                params![i],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO sessions(id, repository, updated_at) VALUES ('b1', 'owner/b', '2026-04-22T09:00:00Z')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO turns(session_id, turn_index) VALUES ('b1', 0)",
            [],
        )
        .unwrap();
        drop(conn);

        let rows = store.list_repositories().expect("must succeed");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].repository, "owner/a");
        assert_eq!(rows[0].session_count, 2);
        assert_eq!(rows[0].turn_count, 5);
        assert_eq!(rows[0].last_seen.as_deref(), Some("2026-04-23T11:00:00Z"));
        assert_eq!(rows[1].repository, "owner/b");
        assert_eq!(rows[1].session_count, 1);
        assert_eq!(rows[1].turn_count, 1);
    }

    #[test]
    fn list_repositories_returns_some_empty_on_empty_db() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _path) = mk_store(&dir, "store.db");
        let rows = store
            .list_repositories()
            .expect("must succeed even when no rows");
        assert!(rows.is_empty());
    }

    #[test]
    fn list_repositories_returns_none_on_schema_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL); \
             INSERT INTO schema_version(version) VALUES (99);",
        )
        .unwrap();
        drop(conn);
        let store = CopilotSessionStore::new(path);
        assert!(store.list_repositories().is_none());
    }

    #[test]
    fn list_sessions_for_repository_orders_by_updated_at_desc() {
        let dir = tempfile::tempdir().unwrap();
        let (store, path) = mk_store(&dir, "store.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO sessions(id, repository, summary, host_type, created_at, updated_at) \
             VALUES ('s1', 'owner/repo', 'older', 'local', '2026-04-22T08:00:00Z', '2026-04-22T08:00:00Z')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO sessions(id, repository, summary, host_type, created_at, updated_at) \
             VALUES ('s2', 'owner/repo', 'newer', 'local', '2026-04-22T08:00:00Z', '2026-04-23T12:00:00Z')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO sessions(id, repository, summary) VALUES ('s3', 'other/repo', 'foreign')",
            [],
        )
        .unwrap();
        drop(conn);

        let rows = store
            .list_sessions_for_repository("owner/repo")
            .expect("must succeed");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "s2");
        assert_eq!(rows[0].summary.as_deref(), Some("newer"));
        assert_eq!(rows[0].host_type.as_deref(), Some("local"));
        assert_eq!(rows[1].id, "s1");
    }

    #[test]
    fn list_sessions_for_repository_treats_input_as_literal() {
        let dir = tempfile::tempdir().unwrap();
        let (store, path) = mk_store(&dir, "store.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO sessions(id, repository, summary) VALUES ('s1', 'owner/repo', 'real')",
            [],
        )
        .unwrap();
        drop(conn);

        let injected = "owner/repo' OR '1'='1";
        let rows = store
            .list_sessions_for_repository(injected)
            .expect("must succeed");
        assert!(
            rows.is_empty(),
            "literal repo string must not match any row"
        );
    }

    #[test]
    fn list_sessions_for_repository_returns_some_empty_for_missing_repo() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _path) = mk_store(&dir, "store.db");
        let rows = store
            .list_sessions_for_repository("never/seen")
            .expect("must succeed");
        assert!(rows.is_empty());
    }

    #[test]
    fn list_sessions_for_repository_returns_none_on_schema_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL); \
             INSERT INTO schema_version(version) VALUES (99);",
        )
        .unwrap();
        drop(conn);
        let store = CopilotSessionStore::new(path);
        assert!(store.list_sessions_for_repository("any/repo").is_none());
    }

    #[test]
    fn list_repositories_falls_back_to_created_at_when_updated_at_null() {
        let dir = tempfile::tempdir().unwrap();
        let (store, path) = mk_store(&dir, "store.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO sessions(id, repository, created_at, updated_at) \
             VALUES ('s1', 'owner/older', '2026-04-20T08:00:00Z', NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sessions(id, repository, created_at, updated_at) \
             VALUES ('s2', 'owner/newer', '2026-04-22T08:00:00Z', NULL)",
            [],
        )
        .unwrap();
        drop(conn);

        let rows = store.list_repositories().expect("must succeed");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].repository, "owner/newer");
        assert_eq!(rows[0].last_seen.as_deref(), Some("2026-04-22T08:00:00Z"));
        assert_eq!(rows[1].repository, "owner/older");
        assert_eq!(rows[1].last_seen.as_deref(), Some("2026-04-20T08:00:00Z"));
    }

    #[test]
    fn list_sessions_for_repository_falls_back_to_created_at_when_updated_at_null() {
        let dir = tempfile::tempdir().unwrap();
        let (store, path) = mk_store(&dir, "store.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO sessions(id, repository, summary, created_at, updated_at) \
             VALUES ('older', 'owner/repo', 'older', '2026-04-20T08:00:00Z', NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sessions(id, repository, summary, created_at, updated_at) \
             VALUES ('newer', 'owner/repo', 'newer', '2026-04-22T08:00:00Z', NULL)",
            [],
        )
        .unwrap();
        drop(conn);

        let rows = store
            .list_sessions_for_repository("owner/repo")
            .expect("must succeed");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "newer");
        assert_eq!(rows[1].id, "older");
    }
}

//! Disposable SQLite scan cache (`{data_dir}/scan-cache.db`).
//!
//! Session stamps (size + mtime) and encoded-dir workspace paths. Historical
//! `project_session_index.json` is imported once then removed.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use crate::error::Result;
use crate::models::AgentId;

const DB_FILE: &str = "scan-cache.db";
const LEGACY_JSON: &str = "project_session_index.json";
const SCHEMA_VERSION: i32 = 1;
const SURFACE_SESSIONS: &str = "sessions";
const SURFACE_PATHS: &str = "paths";
const PARSER_SESSIONS: u32 = 4;
const PARSER_PATHS: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexEntry {
    pub mtime_ms: u64,
    pub size: u64,
    pub project_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_count: Option<u32>,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_role: Option<String>,
}

pub struct SessionIndexStore {
    conn: Connection,
    in_tx: bool,
}

impl SessionIndexStore {
    pub fn load(data_dir: &Path) -> Option<Self> {
        open_cache(data_dir).ok()
    }

    pub fn get_fresh(
        &self,
        agent: AgentId,
        rel: &str,
        size: u64,
        mtime_ms: u64,
    ) -> Option<IndexEntry> {
        let v = get_fresh(
            &self.conn,
            agent,
            SURFACE_SESSIONS,
            rel,
            size,
            mtime_ms,
            PARSER_SESSIONS,
        )?;
        serde_json::from_value(v).ok()
    }

    pub fn put(&mut self, agent: AgentId, rel: &str, entry: IndexEntry) {
        let Ok(payload) = serde_json::to_value(&entry) else {
            return;
        };
        self.ensure_tx();
        put_row(
            &self.conn,
            agent,
            SURFACE_SESSIONS,
            rel,
            entry.size,
            entry.mtime_ms,
            PARSER_SESSIONS,
            &payload,
        );
    }

    pub fn retain_only(&mut self, agent: AgentId, keep: &HashSet<String>) {
        self.ensure_tx();
        retain_rows(&self.conn, agent, SURFACE_SESSIONS, keep);
    }

    pub fn save_if_dirty(&mut self) {
        self.commit();
    }

    pub fn cached_path(&self, agent: AgentId, source_id: &str) -> Option<String> {
        let v = get_payload(&self.conn, agent, SURFACE_PATHS, source_id, PARSER_PATHS)?;
        let path = v.get("actualPath")?.as_str()?.trim();
        if path.is_empty() {
            None
        } else {
            Some(path.to_string())
        }
    }

    pub fn put_path(&mut self, agent: AgentId, source_id: &str, actual: &str) {
        self.ensure_tx();
        put_row(
            &self.conn,
            agent,
            SURFACE_PATHS,
            source_id,
            0,
            0,
            PARSER_PATHS,
            &json!({ "actualPath": actual }),
        );
    }

    fn ensure_tx(&mut self) {
        if self.in_tx {
            return;
        }
        if self.conn.execute_batch("BEGIN IMMEDIATE").is_ok() {
            self.in_tx = true;
        }
    }

    fn commit(&mut self) {
        if !self.in_tx {
            return;
        }
        let _ = self.conn.execute_batch("COMMIT");
        self.in_tx = false;
    }
}

impl Drop for SessionIndexStore {
    fn drop(&mut self) {
        self.commit();
    }
}

fn open_cache(data_dir: &Path) -> Result<SessionIndexStore> {
    let data_dir = if data_dir.is_absolute() {
        data_dir.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(data_dir))
            .unwrap_or_else(|_| data_dir.to_path_buf())
    };
    fs::create_dir_all(&data_dir)?;
    let conn = Connection::open(data_dir.join(DB_FILE))?;
    conn.busy_timeout(Duration::from_millis(5000))?;
    let _ = conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;");
    init_schema(&conn)?;
    import_legacy_json(&conn, &data_dir);
    Ok(SessionIndexStore { conn, in_tx: false })
}

fn init_schema(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version != 0 && version != SCHEMA_VERSION {
        conn.execute_batch("DROP TABLE IF EXISTS scan_entries;")?;
    }
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS scan_entries (
            agent TEXT NOT NULL,
            surface TEXT NOT NULL,
            source_id TEXT NOT NULL,
            size INTEGER NOT NULL,
            mtime_ms INTEGER NOT NULL,
            parser_version INTEGER NOT NULL,
            payload TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (agent, surface, source_id)
        );
        "#,
    )?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

fn get_fresh(
    conn: &Connection,
    agent: AgentId,
    surface: &str,
    source_id: &str,
    size: u64,
    mtime_ms: u64,
    parser_version: u32,
) -> Option<JsonValue> {
    let payload: String = conn
        .query_row(
            r#"
            SELECT payload FROM scan_entries
            WHERE agent = ?1 AND surface = ?2 AND source_id = ?3
              AND size = ?4 AND mtime_ms = ?5 AND parser_version = ?6
            "#,
            params![
                agent.as_str(),
                surface,
                source_id,
                size as i64,
                mtime_ms as i64,
                parser_version as i64
            ],
            |row| row.get(0),
        )
        .optional()
        .ok()??;
    serde_json::from_str(&payload).ok()
}

fn get_payload(
    conn: &Connection,
    agent: AgentId,
    surface: &str,
    source_id: &str,
    parser_version: u32,
) -> Option<JsonValue> {
    let payload: String = conn
        .query_row(
            r#"
            SELECT payload FROM scan_entries
            WHERE agent = ?1 AND surface = ?2 AND source_id = ?3 AND parser_version = ?4
            "#,
            params![agent.as_str(), surface, source_id, parser_version as i64],
            |row| row.get(0),
        )
        .optional()
        .ok()??;
    serde_json::from_str(&payload).ok()
}

fn put_row(
    conn: &Connection,
    agent: AgentId,
    surface: &str,
    source_id: &str,
    size: u64,
    mtime_ms: u64,
    parser_version: u32,
    payload: &JsonValue,
) {
    let Ok(body) = serde_json::to_string(payload) else {
        return;
    };
    let _ = conn.execute(
        r#"
        INSERT INTO scan_entries (
            agent, surface, source_id, size, mtime_ms, parser_version, payload, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(agent, surface, source_id) DO UPDATE SET
            size = excluded.size,
            mtime_ms = excluded.mtime_ms,
            parser_version = excluded.parser_version,
            payload = excluded.payload,
            updated_at = excluded.updated_at
        "#,
        params![
            agent.as_str(),
            surface,
            source_id,
            size as i64,
            mtime_ms as i64,
            parser_version as i64,
            body,
            chrono::Utc::now().to_rfc3339()
        ],
    );
}

fn retain_rows(conn: &Connection, agent: AgentId, surface: &str, keep: &HashSet<String>) {
    if keep.is_empty() {
        let _ = conn.execute(
            "DELETE FROM scan_entries WHERE agent = ?1 AND surface = ?2",
            params![agent.as_str(), surface],
        );
        return;
    }
    let existing: Vec<String> = {
        let Ok(mut rows) =
            conn.prepare("SELECT source_id FROM scan_entries WHERE agent = ?1 AND surface = ?2")
        else {
            return;
        };
        rows.query_map(params![agent.as_str(), surface], |row| row.get(0))
            .ok()
            .map(|it| it.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    };
    for id in existing {
        if !keep.contains(&id) {
            let _ = conn.execute(
                "DELETE FROM scan_entries WHERE agent = ?1 AND surface = ?2 AND source_id = ?3",
                params![agent.as_str(), surface, id],
            );
        }
    }
}

fn import_legacy_json(conn: &Connection, data_dir: &Path) {
    let path = data_dir.join(LEGACY_JSON);
    if !path.is_file() {
        return;
    }
    let Ok(raw) = fs::read_to_string(&path) else {
        return;
    };
    let Ok(doc) = serde_json::from_str::<LegacyIndexFile>(&raw) else {
        let _ = fs::remove_file(&path);
        return;
    };
    if doc.version != PARSER_SESSIONS {
        let _ = fs::remove_file(&path);
        return;
    }
    let _ = conn.execute_batch("BEGIN IMMEDIATE");
    let mut ok = true;
    if let Ok(mut stmt) = conn.prepare(
        r#"
        INSERT OR REPLACE INTO scan_entries (
            agent, surface, source_id, size, mtime_ms, parser_version, payload, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
    ) {
        'outer: for (agent, files) in doc.agents {
            for (source_id, ent) in files.files {
                let Ok(payload) = serde_json::to_string(&ent) else {
                    continue;
                };
                if stmt
                    .execute(params![
                        agent,
                        SURFACE_SESSIONS,
                        source_id,
                        ent.size as i64,
                        ent.mtime_ms as i64,
                        PARSER_SESSIONS as i64,
                        payload,
                        ent.updated_at
                    ])
                    .is_err()
                {
                    ok = false;
                    break 'outer;
                }
            }
        }
    } else {
        ok = false;
    }
    if ok {
        let _ = conn.execute_batch("COMMIT");
        let _ = fs::remove_file(&path);
    } else {
        let _ = conn.execute_batch("ROLLBACK");
    }
}

#[derive(Deserialize)]
struct LegacyIndexFile {
    version: u32,
    #[serde(default)]
    agents: std::collections::BTreeMap<String, LegacyAgentIndex>,
}

#[derive(Deserialize, Default)]
struct LegacyAgentIndex {
    #[serde(default)]
    files: std::collections::BTreeMap<String, LegacyEntry>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LegacyEntry {
    mtime_ms: u64,
    size: u64,
    project_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
    title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    message_count: Option<u32>,
    updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
}

pub fn mtime_ms_from_system(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn file_size_mtime(path: &Path) -> Option<(u64, u64, SystemTime)> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    Some((meta.len(), mtime_ms_from_system(modified), modified))
}

#[cfg(test)]
mod tests;

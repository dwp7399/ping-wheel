//! 本地数据：SQLite（应用数据目录下的 ping.db）+ 截图文件（captures/）。

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::scene::Scene;

pub struct Store(pub Mutex<Connection>);

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

pub fn data_dir(app: &AppHandle) -> PathBuf {
    app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("ping-wheel"))
}

pub fn captures_dir(app: &AppHandle) -> PathBuf {
    let d = data_dir(app).join("captures");
    let _ = std::fs::create_dir_all(&d);
    d
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS conversations (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  title TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS messages (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  conversation_id INTEGER NOT NULL,
  role TEXT NOT NULL,
  content TEXT NOT NULL DEFAULT '',
  reasoning TEXT NOT NULL DEFAULT '',
  image_path TEXT,
  status TEXT NOT NULL DEFAULT 'done',
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_messages_conv ON messages(conversation_id, id);
CREATE TABLE IF NOT EXISTS marks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  created_at INTEGER NOT NULL,
  app_name TEXT NOT NULL DEFAULT '',
  app_path TEXT,
  window_title TEXT,
  url TEXT,
  doc_path TEXT,
  image_path TEXT,
  note TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS collections (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  created_at INTEGER NOT NULL,
  image_path TEXT NOT NULL,
  app_name TEXT NOT NULL DEFAULT '',
  app_path TEXT,
  window_title TEXT,
  url TEXT,
  doc_path TEXT,
  source_key TEXT NOT NULL,
  title TEXT NOT NULL DEFAULT '',
  category TEXT NOT NULL DEFAULT '',
  tags TEXT NOT NULL DEFAULT '[]',
  summary TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'pending'
);
CREATE TABLE IF NOT EXISTS llm_calls (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  at INTEGER NOT NULL,
  purpose TEXT NOT NULL,
  model TEXT NOT NULL,
  ms INTEGER NOT NULL,
  prompt_tokens INTEGER NOT NULL DEFAULT 0,
  completion_tokens INTEGER NOT NULL DEFAULT 0,
  reasoning_tokens INTEGER NOT NULL DEFAULT 0,
  ok INTEGER NOT NULL,
  error TEXT
);
"#;

pub fn open(app: &AppHandle) -> Result<Store, String> {
    let dir = data_dir(app);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建数据目录失败：{e}"))?;
    let path = dir.join("ping.db");
    let conn = Connection::open(&path).map_err(|e| format!("打开数据库 {} 失败：{e}", path.display()))?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .and_then(|_| conn.execute_batch(SCHEMA))
        .map_err(|e| format!("初始化数据库失败：{e}"))?;
    log::info!("存储：数据库 {}", path.display());
    Ok(Store(Mutex::new(conn)))
}

/// 在数据库上执行一段操作。
pub fn with<T>(app: &AppHandle, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T, String> {
    let store = app.state::<Store>();
    let conn = store.0.lock().unwrap();
    f(&conn).map_err(|e| {
        log::warn!("存储：数据库操作失败：{e}");
        format!("数据库操作失败：{e}")
    })
}

pub fn record_llm_call(
    app: &AppHandle,
    purpose: &str,
    model: &str,
    ms: u64,
    result: &Result<crate::llm::Reply, String>,
) {
    let (ok, usage, err) = match result {
        Ok(r) => (1, r.usage.clone(), None),
        Err(e) => (0, Default::default(), Some(e.clone())),
    };
    let _ = with(app, |c| {
        c.execute(
            "INSERT INTO llm_calls (at, purpose, model, ms, prompt_tokens, completion_tokens, reasoning_tokens, ok, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                now_ms(),
                purpose,
                model,
                ms as i64,
                usage.prompt_tokens as i64,
                usage.completion_tokens as i64,
                usage.reasoning_tokens as i64,
                ok,
                err
            ],
        )
    });
}

// ---------- 对话 ----------

#[derive(Serialize, Clone)]
pub struct ConversationRow {
    pub id: i64,
    pub title: String,
    pub updated_at: i64,
}

#[derive(Serialize, Clone)]
pub struct MessageRow {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub reasoning: String,
    pub image_path: Option<String>,
    pub status: String,
    pub created_at: i64,
}

pub fn create_conversation(app: &AppHandle, title: &str) -> Result<i64, String> {
    with(app, |c| {
        let t = now_ms();
        c.execute(
            "INSERT INTO conversations (title, created_at, updated_at) VALUES (?1, ?2, ?2)",
            params![title, t],
        )?;
        Ok(c.last_insert_rowid())
    })
}

pub fn add_message(
    app: &AppHandle,
    conversation_id: i64,
    role: &str,
    content: &str,
    image_path: Option<&str>,
    status: &str,
) -> Result<i64, String> {
    with(app, |c| {
        let t = now_ms();
        c.execute(
            "INSERT INTO messages (conversation_id, role, content, image_path, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![conversation_id, role, content, image_path, status, t],
        )?;
        c.execute("UPDATE conversations SET updated_at = ?1 WHERE id = ?2", params![t, conversation_id])?;
        Ok(c.last_insert_rowid())
    })
}

pub fn finish_message(app: &AppHandle, id: i64, content: &str, reasoning: &str, status: &str) -> Result<(), String> {
    with(app, |c| {
        c.execute(
            "UPDATE messages SET content = ?1, reasoning = ?2, status = ?3 WHERE id = ?4",
            params![content, reasoning, status, id],
        )
        .map(|_| ())
    })
}

pub fn set_conversation_title(app: &AppHandle, id: i64, title: &str) -> Result<(), String> {
    with(app, |c| {
        c.execute("UPDATE conversations SET title = ?1 WHERE id = ?2", params![title, id]).map(|_| ())
    })
}

pub fn conversation_title(app: &AppHandle, id: i64) -> Result<Option<String>, String> {
    with(app, |c| {
        c.query_row("SELECT title FROM conversations WHERE id = ?1", [id], |r| r.get(0)).optional()
    })
}

pub fn list_conversations(app: &AppHandle) -> Result<Vec<ConversationRow>, String> {
    with(app, |c| {
        let mut st = c.prepare("SELECT id, title, updated_at FROM conversations ORDER BY updated_at DESC LIMIT 200")?;
        let rows = st.query_map([], |r| Ok(ConversationRow { id: r.get(0)?, title: r.get(1)?, updated_at: r.get(2)? }))?;
        rows.collect()
    })
}

pub fn list_messages(app: &AppHandle, conversation_id: i64) -> Result<Vec<MessageRow>, String> {
    with(app, |c| {
        let mut st = c.prepare(
            "SELECT id, role, content, reasoning, image_path, status, created_at FROM messages
             WHERE conversation_id = ?1 ORDER BY id",
        )?;
        let rows = st.query_map([conversation_id], |r| {
            Ok(MessageRow {
                id: r.get(0)?,
                role: r.get(1)?,
                content: r.get(2)?,
                reasoning: r.get(3)?,
                image_path: r.get(4)?,
                status: r.get(5)?,
                created_at: r.get(6)?,
            })
        })?;
        rows.collect()
    })
}

pub fn delete_conversation(app: &AppHandle, id: i64) -> Result<(), String> {
    with(app, |c| {
        c.execute("DELETE FROM messages WHERE conversation_id = ?1", [id])?;
        c.execute("DELETE FROM conversations WHERE id = ?1", [id]).map(|_| ())
    })
}

// ---------- 标记 ----------

#[derive(Serialize, Clone)]
pub struct MarkRow {
    pub id: i64,
    pub created_at: i64,
    pub app_name: String,
    pub app_path: Option<String>,
    pub window_title: Option<String>,
    pub url: Option<String>,
    pub doc_path: Option<String>,
    pub image_path: Option<String>,
    pub note: String,
}

pub fn add_mark(app: &AppHandle, s: &Scene, image_path: Option<&str>) -> Result<i64, String> {
    with(app, |c| {
        c.execute(
            "INSERT INTO marks (created_at, app_name, app_path, window_title, url, doc_path, image_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![now_ms(), s.app_name, s.app_path, s.window_title, s.url, s.doc_path, image_path],
        )?;
        Ok(c.last_insert_rowid())
    })
}

fn mark_from_row(r: &rusqlite::Row) -> rusqlite::Result<MarkRow> {
    Ok(MarkRow {
        id: r.get(0)?,
        created_at: r.get(1)?,
        app_name: r.get(2)?,
        app_path: r.get(3)?,
        window_title: r.get(4)?,
        url: r.get(5)?,
        doc_path: r.get(6)?,
        image_path: r.get(7)?,
        note: r.get(8)?,
    })
}

const MARK_COLS: &str = "id, created_at, app_name, app_path, window_title, url, doc_path, image_path, note";

pub fn list_marks(app: &AppHandle) -> Result<Vec<MarkRow>, String> {
    with(app, |c| {
        let mut st = c.prepare(&format!("SELECT {MARK_COLS} FROM marks ORDER BY id DESC LIMIT 500"))?;
        let rows = st.query_map([], mark_from_row)?;
        rows.collect()
    })
}

pub fn get_mark(app: &AppHandle, id: i64) -> Result<Option<MarkRow>, String> {
    with(app, |c| {
        c.query_row(&format!("SELECT {MARK_COLS} FROM marks WHERE id = ?1"), [id], mark_from_row).optional()
    })
}

pub fn delete_mark(app: &AppHandle, id: i64) -> Result<Option<String>, String> {
    with(app, |c| {
        let img: Option<String> =
            c.query_row("SELECT image_path FROM marks WHERE id = ?1", [id], |r| r.get(0)).optional()?.flatten();
        c.execute("DELETE FROM marks WHERE id = ?1", [id])?;
        Ok(img)
    })
}

pub fn set_mark_note(app: &AppHandle, id: i64, note: &str) -> Result<(), String> {
    with(app, |c| c.execute("UPDATE marks SET note = ?1 WHERE id = ?2", params![note, id]).map(|_| ()))
}

// ---------- 收藏 ----------

#[derive(Serialize, Clone)]
pub struct CollectionRow {
    pub id: i64,
    pub created_at: i64,
    pub image_path: String,
    pub app_name: String,
    pub app_path: Option<String>,
    pub window_title: Option<String>,
    pub url: Option<String>,
    pub doc_path: Option<String>,
    pub source_key: String,
    pub title: String,
    pub category: String,
    pub tags: Vec<String>,
    pub summary: String,
    pub status: String,
}

const COLL_COLS: &str = "id, created_at, image_path, app_name, app_path, window_title, url, doc_path, source_key, title, category, tags, summary, status";

fn coll_from_row(r: &rusqlite::Row) -> rusqlite::Result<CollectionRow> {
    let tags: String = r.get(11)?;
    Ok(CollectionRow {
        id: r.get(0)?,
        created_at: r.get(1)?,
        image_path: r.get(2)?,
        app_name: r.get(3)?,
        app_path: r.get(4)?,
        window_title: r.get(5)?,
        url: r.get(6)?,
        doc_path: r.get(7)?,
        source_key: r.get(8)?,
        title: r.get(9)?,
        category: r.get(10)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        summary: r.get(12)?,
        status: r.get(13)?,
    })
}

pub fn add_collection(app: &AppHandle, s: &Scene, image_path: &str, source_key: &str) -> Result<i64, String> {
    with(app, |c| {
        c.execute(
            "INSERT INTO collections (created_at, image_path, app_name, app_path, window_title, url, doc_path, source_key, title)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                now_ms(),
                image_path,
                s.app_name,
                s.app_path,
                s.window_title,
                s.url,
                s.doc_path,
                source_key,
                s.window_title.clone().unwrap_or_else(|| s.app_name.clone())
            ],
        )?;
        Ok(c.last_insert_rowid())
    })
}

pub fn classify_collection(
    app: &AppHandle,
    id: i64,
    title: &str,
    category: &str,
    tags: &[String],
    summary: &str,
    status: &str,
) -> Result<(), String> {
    with(app, |c| {
        c.execute(
            "UPDATE collections SET title = ?1, category = ?2, tags = ?3, summary = ?4, status = ?5 WHERE id = ?6",
            params![title, category, serde_json::to_string(tags).unwrap_or_default(), summary, status, id],
        )
        .map(|_| ())
    })
}

pub fn set_collection_status(app: &AppHandle, id: i64, status: &str) -> Result<(), String> {
    with(app, |c| c.execute("UPDATE collections SET status = ?1 WHERE id = ?2", params![status, id]).map(|_| ()))
}

pub fn list_collections(app: &AppHandle) -> Result<Vec<CollectionRow>, String> {
    with(app, |c| {
        let mut st = c.prepare(&format!("SELECT {COLL_COLS} FROM collections ORDER BY id DESC LIMIT 1000"))?;
        let rows = st.query_map([], coll_from_row)?;
        rows.collect()
    })
}

pub fn get_collection(app: &AppHandle, id: i64) -> Result<Option<CollectionRow>, String> {
    with(app, |c| {
        c.query_row(&format!("SELECT {COLL_COLS} FROM collections WHERE id = ?1"), [id], coll_from_row).optional()
    })
}

/// 已有的分类，供归类时让模型优先复用。
pub fn categories(app: &AppHandle) -> Result<Vec<String>, String> {
    with(app, |c| {
        let mut st = c.prepare(
            "SELECT category FROM collections WHERE category != '' GROUP BY category ORDER BY COUNT(*) DESC LIMIT 30",
        )?;
        let rows = st.query_map([], |r| r.get(0))?;
        rows.collect()
    })
}

/// 同一来源已有的分类（同一篇文章 / 链接的收藏放在一起）。
pub fn category_of_source(app: &AppHandle, source_key: &str, exclude_id: i64) -> Result<Option<String>, String> {
    with(app, |c| {
        c.query_row(
            "SELECT category FROM collections WHERE source_key = ?1 AND id != ?2 AND category != '' ORDER BY id DESC LIMIT 1",
            params![source_key, exclude_id],
            |r| r.get(0),
        )
        .optional()
    })
}

pub fn delete_collection(app: &AppHandle, id: i64) -> Result<Option<String>, String> {
    with(app, |c| {
        let img: Option<String> =
            c.query_row("SELECT image_path FROM collections WHERE id = ?1", [id], |r| r.get(0)).optional()?;
        c.execute("DELETE FROM collections WHERE id = ?1", [id])?;
        Ok(img)
    })
}

// ---------- 用量 ----------

#[derive(Serialize)]
pub struct UsageSummary {
    pub calls: i64,
    pub failed: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
}

pub fn usage_since(app: &AppHandle, since_ms: i64) -> Result<UsageSummary, String> {
    with(app, |c| {
        c.query_row(
            "SELECT COUNT(*), COALESCE(SUM(1 - ok), 0), COALESCE(SUM(prompt_tokens), 0), COALESCE(SUM(completion_tokens), 0)
             FROM llm_calls WHERE at >= ?1",
            [since_ms],
            |r| Ok(UsageSummary { calls: r.get(0)?, failed: r.get(1)?, prompt_tokens: r.get(2)?, completion_tokens: r.get(3)? }),
        )
    })
}

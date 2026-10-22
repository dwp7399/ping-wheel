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
-- 旧版本的对话历史和模型调用记录：不再保存，清掉
DROP TABLE IF EXISTS llm_calls;
DROP TABLE IF EXISTS messages;
DROP TABLE IF EXISTS conversations;
"#;

pub fn open(app: &AppHandle) -> Result<Store, String> {
    let dir = data_dir(app);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建数据目录失败：{e}"))?;
    let path = dir.join("ping.db");
    let conn = Connection::open(&path).map_err(|e| format!("打开数据库 {} 失败：{e}", path.display()))?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")
        .and_then(|_| conn.execute_batch(SCHEMA))
        .map_err(|e| format!("初始化数据库失败：{e}"))?;
    // 迁移：标记增加 bundle_id（应用路径失效时用它启动应用）
    let _ = conn.execute("ALTER TABLE marks ADD COLUMN bundle_id TEXT", []);
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

// ---------- 标记 ----------

#[derive(Serialize, Clone)]
pub struct MarkRow {
    pub id: i64,
    pub created_at: i64,
    pub app_name: String,
    pub app_path: Option<String>,
    pub bundle_id: Option<String>,
    pub window_title: Option<String>,
    pub url: Option<String>,
    pub doc_path: Option<String>,
    pub image_path: Option<String>,
    pub note: String,
}

pub fn add_mark(app: &AppHandle, s: &Scene, image_path: Option<&str>) -> Result<i64, String> {
    with(app, |c| {
        c.execute(
            "INSERT INTO marks (created_at, app_name, app_path, bundle_id, window_title, url, doc_path, image_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![now_ms(), s.app_name, s.app_path, s.bundle_id, s.window_title, s.url, s.doc_path, image_path],
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
        bundle_id: r.get(9)?,
    })
}

const MARK_COLS: &str = "id, created_at, app_name, app_path, window_title, url, doc_path, image_path, note, bundle_id";

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

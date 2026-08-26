use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_MAX_BYTES: i64 = 500 * 1024 * 1024;

#[derive(Serialize, Clone)]
pub struct ItemDto {
    pub id: i64,
    pub kind: String,
    pub preview: Option<String>,
    pub image_path: Option<String>,
    pub thumb_path: Option<String>,
    pub file_paths: Option<Vec<String>>,
    pub source_app: Option<String>,
    pub source_icon_path: Option<String>,
    pub pinned: bool,
    pub last_copied_at: i64,
    pub byte_size: i64,
    pub has_rich: bool,
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn init(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         CREATE TABLE IF NOT EXISTS items (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           kind TEXT NOT NULL,
           hash TEXT NOT NULL UNIQUE,
           plain_text TEXT,
           rtf BLOB,
           html TEXT,
           image_path TEXT,
           thumb_path TEXT,
           file_paths TEXT,
           byte_size INTEGER NOT NULL DEFAULT 0,
           source_app TEXT,
           source_bundle_id TEXT,
           source_icon_path TEXT,
           pinned INTEGER NOT NULL DEFAULT 0,
           created_at INTEGER NOT NULL,
           last_copied_at INTEGER NOT NULL,
           sort_at INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
    )?;
    // migration for databases created before the frozen display order existed
    let _ = conn.execute(
        "ALTER TABLE items ADD COLUMN sort_at INTEGER NOT NULL DEFAULT 0",
        [],
    );
    conn.execute_batch(
        "UPDATE items SET sort_at = last_copied_at WHERE sort_at = 0;
         DROP INDEX IF EXISTS idx_items_order;
         CREATE INDEX IF NOT EXISTS idx_items_sort ON items (pinned DESC, sort_at DESC);",
    )?;
    Ok(conn)
}

/// Snapshot the display order from true recency. Called when the bar opens, so
/// paste-bumps never reshuffle the cards mid-session — order refreshes on the
/// next open instead.
pub fn snapshot_order(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("UPDATE items SET sort_at = last_copied_at", [])?;
    Ok(())
}

pub fn get_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |r| r.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) {
    let _ = conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    );
}

pub fn max_bytes(conn: &Connection) -> i64 {
    get_setting(conn, "max_bytes")
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAX_BYTES)
}

pub struct NewItem<'a> {
    pub kind: &'a str,
    pub hash: String,
    pub plain_text: Option<String>,
    pub rtf: Option<Vec<u8>>,
    pub html: Option<String>,
    pub image_path: Option<String>,
    pub thumb_path: Option<String>,
    pub file_paths: Option<Vec<String>>,
    pub byte_size: i64,
    pub source_app: Option<String>,
    pub source_bundle_id: Option<String>,
    pub source_icon_path: Option<String>,
}

/// Insert a captured item, or bump an identical existing one to the top.
pub fn insert_or_bump(conn: &Connection, item: NewItem) -> rusqlite::Result<()> {
    let now = now_ms();
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM items WHERE hash = ?1",
            params![item.hash],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        conn.execute(
            "UPDATE items SET last_copied_at = ?1, source_app = COALESCE(?2, source_app),
             source_bundle_id = COALESCE(?3, source_bundle_id),
             source_icon_path = COALESCE(?4, source_icon_path) WHERE id = ?5",
            params![now, item.source_app, item.source_bundle_id, item.source_icon_path, id],
        )?;
        return Ok(());
    }
    conn.execute(
        "INSERT INTO items (kind, hash, plain_text, rtf, html, image_path, thumb_path, file_paths,
                            byte_size, source_app, source_bundle_id, source_icon_path,
                            pinned, created_at, last_copied_at, sort_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 0, ?13, ?13, ?13)",
        params![
            item.kind,
            item.hash,
            item.plain_text,
            item.rtf,
            item.html,
            item.image_path,
            item.thumb_path,
            item.file_paths.as_ref().map(|p| serde_json::to_string(p).unwrap_or_default()),
            item.byte_size,
            item.source_app,
            item.source_bundle_id,
            item.source_icon_path,
            now,
        ],
    )?;
    evict_over_budget(conn)?;
    Ok(())
}

/// Delete oldest unpinned items (and their image files) until under the disk budget.
fn evict_over_budget(conn: &Connection) -> rusqlite::Result<()> {
    let budget = max_bytes(conn);
    loop {
        let total: i64 =
            conn.query_row("SELECT COALESCE(SUM(byte_size), 0) FROM items", [], |r| r.get(0))?;
        if total <= budget {
            return Ok(());
        }
        let victim: Option<(i64, Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT id, image_path, thumb_path FROM items WHERE pinned = 0
                 ORDER BY last_copied_at ASC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        match victim {
            Some((id, img, thumb)) => {
                remove_files(&img, &thumb);
                conn.execute("DELETE FROM items WHERE id = ?1", params![id])?;
            }
            None => return Ok(()), // everything left is pinned
        }
    }
}

fn remove_files(image_path: &Option<String>, thumb_path: &Option<String>) {
    for p in [image_path, thumb_path].into_iter().flatten() {
        let _ = std::fs::remove_file(p);
    }
}

pub fn list(conn: &Connection, query: &str) -> rusqlite::Result<Vec<ItemDto>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, substr(COALESCE(plain_text, ''), 1, 400), image_path, thumb_path,
                file_paths, source_app, source_icon_path, pinned, last_copied_at, byte_size,
                (rtf IS NOT NULL OR html IS NOT NULL)
         FROM items
         WHERE ?1 = '' OR plain_text LIKE '%' || ?1 || '%'
         ORDER BY pinned DESC, sort_at DESC
         LIMIT 200",
    )?;
    let rows = stmt.query_map(params![query], |r| {
        let file_paths: Option<String> = r.get(5)?;
        let preview: String = r.get(2)?;
        Ok(ItemDto {
            id: r.get(0)?,
            kind: r.get(1)?,
            preview: if preview.is_empty() { None } else { Some(preview) },
            image_path: r.get(3)?,
            thumb_path: r.get(4)?,
            file_paths: file_paths.and_then(|j| serde_json::from_str(&j).ok()),
            source_app: r.get(6)?,
            source_icon_path: r.get(7)?,
            pinned: r.get::<_, i64>(8)? != 0,
            last_copied_at: r.get(9)?,
            byte_size: r.get(10)?,
            has_rich: r.get::<_, i64>(11)? != 0,
        })
    })?;
    rows.collect()
}

pub struct FullItem {
    pub kind: String,
    pub plain_text: Option<String>,
    pub rtf: Option<Vec<u8>>,
    pub html: Option<String>,
    pub image_path: Option<String>,
    pub file_paths: Option<Vec<String>>,
}

pub fn get_full(conn: &Connection, id: i64) -> rusqlite::Result<Option<FullItem>> {
    conn.query_row(
        "SELECT kind, plain_text, rtf, html, image_path, file_paths FROM items WHERE id = ?1",
        params![id],
        |r| {
            let file_paths: Option<String> = r.get(5)?;
            Ok(FullItem {
                kind: r.get(0)?,
                plain_text: r.get(1)?,
                rtf: r.get(2)?,
                html: r.get(3)?,
                image_path: r.get(4)?,
                file_paths: file_paths.and_then(|j| serde_json::from_str(&j).ok()),
            })
        },
    )
    .optional()
}

pub fn bump(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE items SET last_copied_at = ?1 WHERE id = ?2",
        params![now_ms(), id],
    )?;
    Ok(())
}

pub fn toggle_pin(conn: &Connection, id: i64) -> rusqlite::Result<bool> {
    conn.execute("UPDATE items SET pinned = 1 - pinned WHERE id = ?1", params![id])?;
    conn.query_row("SELECT pinned FROM items WHERE id = ?1", params![id], |r| {
        r.get::<_, i64>(0).map(|v| v != 0)
    })
}

pub fn delete(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    let paths: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT image_path, thumb_path FROM items WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((img, thumb)) = paths {
        remove_files(&img, &thumb);
    }
    conn.execute("DELETE FROM items WHERE id = ?1", params![id])?;
    Ok(())
}

/// Delete all unpinned items and their image files.
pub fn clear_all(conn: &Connection) -> rusqlite::Result<()> {
    let mut stmt =
        conn.prepare("SELECT image_path, thumb_path FROM items WHERE pinned = 0")?;
    let rows: Vec<(Option<String>, Option<String>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);
    for (img, thumb) in &rows {
        remove_files(img, thumb);
    }
    conn.execute("DELETE FROM items WHERE pinned = 0", [])?;
    Ok(())
}

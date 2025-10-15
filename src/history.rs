use std::path::PathBuf;

use rusqlite::{params, Connection};

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub id: i64,
    pub prompt: String,
    pub response: String,
    #[allow(dead_code)]
    pub created_at: i64,
    pub image_path: Option<String>,
}

fn db_path() -> PathBuf {
    Config::get_config_dir().join("history.sqlite")
}

fn ensure_dir() -> std::io::Result<()> {
    let dir = Config::get_config_dir();
    std::fs::create_dir_all(dir)
}

pub fn init() -> anyhow::Result<()> {
    ensure_dir()?;
    let conn = Connection::open(db_path())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            prompt TEXT NOT NULL,
            response TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            image_path TEXT
        )",
        [],
    )?;

    // Migration: Add image_path column if it doesn't exist
    let _ = conn.execute(
        "ALTER TABLE history ADD COLUMN image_path TEXT",
        [],
    );
    // Ignore error if column already exists

    Ok(())
}

pub fn add_entry(prompt: &str, response: &str) -> anyhow::Result<()> {
    add_entry_with_image(prompt, response, None)
}

pub fn add_entry_with_image(prompt: &str, response: &str, image_path: Option<&str>) -> anyhow::Result<()> {
    ensure_dir()?;
    let conn = Connection::open(db_path())?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    conn.execute(
        "INSERT INTO history (prompt, response, created_at, image_path) VALUES (?1, ?2, ?3, ?4)",
        params![prompt, response, now, image_path],
    )?;
    Ok(())
}

pub fn list_entries(limit: usize) -> anyhow::Result<Vec<HistoryEntry>> {
    ensure_dir()?;
    let conn = Connection::open(db_path())?;
    let mut stmt = conn.prepare(
        "SELECT id, prompt, response, created_at, image_path
         FROM history
         ORDER BY created_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit as i64], |row| {
        Ok(HistoryEntry {
            id: row.get(0)?,
            prompt: row.get(1)?,
            response: row.get(2)?,
            created_at: row.get(3)?,
            image_path: row.get(4)?,
        })
    })?;

    let mut entries = Vec::new();
    for r in rows {
        if let Ok(e) = r { entries.push(e); }
    }
    Ok(entries)
}

#[allow(dead_code)]
pub fn get_entry(id: i64) -> anyhow::Result<Option<HistoryEntry>> {
    ensure_dir()?;
    let conn = Connection::open(db_path())?;
    let mut stmt = conn.prepare(
        "SELECT id, prompt, response, created_at, image_path FROM history WHERE id = ?1"
    )?;
    let mut rows = stmt.query([id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(HistoryEntry {
            id: row.get(0)?,
            prompt: row.get(1)?,
            response: row.get(2)?,
            created_at: row.get(3)?,
            image_path: row.get(4)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn delete_entry(id: i64) -> anyhow::Result<()> {
    ensure_dir()?;
    let conn = Connection::open(db_path())?;
    conn.execute("DELETE FROM history WHERE id = ?1", params![id])?;
    Ok(())
}

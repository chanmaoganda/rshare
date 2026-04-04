use chrono::{DateTime, Utc};
use rshare_common::FileMetadata;
use rusqlite::{Connection, params};
use std::path::Path;
use std::sync::Mutex;
use uuid::Uuid;

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(data_dir: &Path) -> Result<Self, rusqlite::Error> {
        let db_path = data_dir.join("rshare.db");
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS files (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                size INTEGER NOT NULL,
                uploaded_at TEXT NOT NULL,
                share_token TEXT,
                delete_token TEXT
            );",
        )?;
        // Safe migration for existing databases
        let has_delete_token: bool = conn
            .prepare("SELECT COUNT(*) FROM pragma_table_info('files') WHERE name = 'delete_token'")?
            .query_row([], |row| row.get::<_, i64>(0))
            .map(|c| c > 0)?;
        if !has_delete_token {
            conn.execute_batch("ALTER TABLE files ADD COLUMN delete_token TEXT;")?;
        }
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn insert(&self, meta: &FileMetadata, delete_token: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO files (id, name, size, uploaded_at, share_token, delete_token) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                meta.id.to_string(),
                meta.name,
                meta.size as i64,
                meta.uploaded_at.to_rfc3339(),
                meta.share_token,
                delete_token,
            ],
        )?;
        Ok(())
    }

    pub fn get_delete_token(&self, id: Uuid) -> Result<Option<String>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT delete_token FROM files WHERE id = ?1")?;
        let mut rows = stmt.query_map(params![id.to_string()], |row| row.get(0))?;
        match rows.next() {
            Some(Ok(token)) => Ok(token),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub fn list(&self) -> Result<Vec<FileMetadata>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, size, uploaded_at, share_token FROM files ORDER BY uploaded_at DESC")?;
        let rows = stmt.query_map([], |row| {
            let id_str: String = row.get(0)?;
            let uploaded_str: String = row.get(3)?;
            Ok(FileMetadata {
                id: Uuid::parse_str(&id_str).unwrap(),
                name: row.get(1)?,
                size: row.get::<_, i64>(2)? as u64,
                uploaded_at: uploaded_str.parse::<DateTime<Utc>>().unwrap(),
                share_token: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    pub fn get(&self, id: Uuid) -> Result<Option<FileMetadata>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, size, uploaded_at, share_token FROM files WHERE id = ?1")?;
        let mut rows = stmt.query_map(params![id.to_string()], |row| {
            let id_str: String = row.get(0)?;
            let uploaded_str: String = row.get(3)?;
            Ok(FileMetadata {
                id: Uuid::parse_str(&id_str).unwrap(),
                name: row.get(1)?,
                size: row.get::<_, i64>(2)? as u64,
                uploaded_at: uploaded_str.parse::<DateTime<Utc>>().unwrap(),
                share_token: row.get(4)?,
            })
        })?;
        match rows.next() {
            Some(Ok(meta)) => Ok(Some(meta)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub fn get_by_share_token(&self, token: &str) -> Result<Option<FileMetadata>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, size, uploaded_at, share_token FROM files WHERE share_token = ?1")?;
        let mut rows = stmt.query_map(params![token], |row| {
            let id_str: String = row.get(0)?;
            let uploaded_str: String = row.get(3)?;
            Ok(FileMetadata {
                id: Uuid::parse_str(&id_str).unwrap(),
                name: row.get(1)?,
                size: row.get::<_, i64>(2)? as u64,
                uploaded_at: uploaded_str.parse::<DateTime<Utc>>().unwrap(),
                share_token: row.get(4)?,
            })
        })?;
        match rows.next() {
            Some(Ok(meta)) => Ok(Some(meta)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub fn delete(&self, id: Uuid) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute("DELETE FROM files WHERE id = ?1", params![id.to_string()])?;
        Ok(count > 0)
    }

    pub fn set_share_token(&self, id: Uuid, token: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute(
            "UPDATE files SET share_token = ?1 WHERE id = ?2",
            params![token, id.to_string()],
        )?;
        Ok(count > 0)
    }
}

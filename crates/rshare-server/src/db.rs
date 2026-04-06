use chrono::{DateTime, Utc};
use rshare_common::{ApiToken, FileMetadata};
use rusqlite::{Connection, Row, params};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Mutex;
use uuid::Uuid;

fn hash_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

pub struct Db {
    conn: Mutex<Connection>,
}

const FILE_COLUMNS: &str =
    "id, name, size, uploaded_at, share_token, content_type, sha256, expires_at";

fn parse_file_row(row: &Row) -> rusqlite::Result<FileMetadata> {
    let id_str: String = row.get(0)?;
    let uploaded_str: String = row.get(3)?;
    let id = Uuid::parse_str(&id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let uploaded_at = uploaded_str.parse::<DateTime<Utc>>().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let expires_at: Option<String> = row.get(7)?;
    let expires_at = expires_at
        .map(|s| {
            s.parse::<DateTime<Utc>>().map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    7,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })
        })
        .transpose()?;
    Ok(FileMetadata {
        id,
        name: row.get(1)?,
        size: row.get::<_, i64>(2)? as u64,
        uploaded_at,
        share_token: row.get(4)?,
        content_type: row.get(5)?,
        sha256: row.get(6)?,
        expires_at,
    })
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
        // Migrate new columns
        for col in &["content_type", "sha256", "expires_at"] {
            let has_col: bool = conn
                .prepare(&format!(
                    "SELECT COUNT(*) FROM pragma_table_info('files') WHERE name = '{col}'"
                ))?
                .query_row([], |row| row.get::<_, i64>(0))
                .map(|c| c > 0)?;
            if !has_col {
                conn.execute_batch(&format!("ALTER TABLE files ADD COLUMN {col} TEXT;"))?;
            }
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS api_tokens (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                token_hash TEXT NOT NULL,
                permissions TEXT NOT NULL,
                created_at TEXT NOT NULL
            );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    // --- API Token methods ---

    pub fn insert_token(
        &self,
        name: &str,
        raw_token: &str,
        permissions: &[String],
    ) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO api_tokens (id, name, token_hash, permissions, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id,
                name,
                hash_token(raw_token),
                permissions.join(","),
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_token_by_hash(&self, raw_token: &str) -> Result<Option<ApiToken>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let token_hash = hash_token(raw_token);
        let mut stmt = conn.prepare(
            "SELECT name, permissions, created_at FROM api_tokens WHERE token_hash = ?1",
        )?;
        let mut rows = stmt.query_map(params![token_hash], |row| {
            let perms_str: String = row.get(1)?;
            let created_str: String = row.get(2)?;
            Ok((row.get::<_, String>(0)?, perms_str, created_str))
        })?;
        match rows.next() {
            Some(Ok((name, perms_str, created_str))) => {
                let permissions = perms_str.split(',').map(|s| s.trim().to_string()).collect();
                let created_at = created_str
                    .parse::<DateTime<Utc>>()
                    .unwrap_or_else(|_| Utc::now());
                Ok(Some(ApiToken {
                    name,
                    permissions,
                    created_at,
                }))
            }
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub fn list_tokens(&self) -> Result<Vec<ApiToken>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt =
            conn.prepare("SELECT name, permissions, created_at FROM api_tokens ORDER BY name")?;
        let rows = stmt.query_map([], |row| {
            let perms_str: String = row.get(1)?;
            let created_str: String = row.get(2)?;
            let permissions = perms_str.split(',').map(|s| s.trim().to_string()).collect();
            let created_at = created_str
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now());
            Ok(ApiToken {
                name: row.get(0)?,
                permissions,
                created_at,
            })
        })?;
        rows.collect()
    }

    pub fn delete_token(&self, name: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let count = conn.execute("DELETE FROM api_tokens WHERE name = ?1", params![name])?;
        Ok(count > 0)
    }

    pub fn has_any_tokens(&self) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM api_tokens", [], |row| row.get(0))?;
        Ok(count > 0)
    }

    // --- File methods ---

    pub fn insert(&self, meta: &FileMetadata, delete_token: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO files (id, name, size, uploaded_at, share_token, delete_token, content_type, sha256, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                meta.id.to_string(),
                meta.name,
                meta.size as i64,
                meta.uploaded_at.to_rfc3339(),
                meta.share_token,
                delete_token,
                meta.content_type,
                meta.sha256,
                meta.expires_at.map(|t| t.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    pub fn get_delete_token(&self, id: Uuid) -> Result<Option<String>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare("SELECT delete_token FROM files WHERE id = ?1")?;
        let mut rows = stmt.query_map(params![id.to_string()], |row| row.get(0))?;
        match rows.next() {
            Some(Ok(token)) => Ok(token),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub fn list(
        &self,
        page: u32,
        per_page: u32,
    ) -> Result<(Vec<FileMetadata>, u64), rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        let offset = (page.saturating_sub(1) * per_page) as i64;
        let mut stmt = conn.prepare(&format!(
            "SELECT {FILE_COLUMNS} FROM files ORDER BY uploaded_at DESC LIMIT ?1 OFFSET ?2"
        ))?;
        let rows = stmt.query_map(params![per_page as i64, offset], parse_file_row)?;
        let files: Result<Vec<_>, _> = rows.collect();
        Ok((files?, total as u64))
    }

    pub fn get(&self, id: Uuid) -> Result<Option<FileMetadata>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(&format!("SELECT {FILE_COLUMNS} FROM files WHERE id = ?1"))?;
        let mut rows = stmt.query_map(params![id.to_string()], parse_file_row)?;
        match rows.next() {
            Some(Ok(meta)) => Ok(Some(meta)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub fn get_by_share_token(&self, token: &str) -> Result<Option<FileMetadata>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(&format!(
            "SELECT {FILE_COLUMNS} FROM files WHERE share_token = ?1"
        ))?;
        let mut rows = stmt.query_map(params![token], parse_file_row)?;
        match rows.next() {
            Some(Ok(meta)) => Ok(Some(meta)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub fn delete(&self, id: Uuid) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let count = conn.execute("DELETE FROM files WHERE id = ?1", params![id.to_string()])?;
        Ok(count > 0)
    }

    pub fn list_expired(&self) -> Result<Vec<Uuid>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let now = Utc::now().to_rfc3339();
        let mut stmt =
            conn.prepare("SELECT id FROM files WHERE expires_at IS NOT NULL AND expires_at < ?1")?;
        let rows = stmt.query_map(params![now], |row| {
            let id_str: String = row.get(0)?;
            Ok(id_str)
        })?;
        let ids = rows
            .flatten()
            .filter_map(|id_str| Uuid::parse_str(&id_str).ok())
            .collect();
        Ok(ids)
    }

    pub fn set_share_token(&self, id: Uuid, token: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let count = conn.execute(
            "UPDATE files SET share_token = ?1 WHERE id = ?2",
            params![token, id.to_string()],
        )?;
        Ok(count > 0)
    }
}

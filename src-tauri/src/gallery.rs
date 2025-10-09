use std::{
    fs,
    path::Path,
    sync::{Mutex, OnceLock},
};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params, types::Value};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const DB_FILE_NAME: &str = "gallery.db";

static GALLERY_STORE: OnceLock<GalleryStore> = OnceLock::new();

#[derive(Debug, Serialize)]
pub struct GalleryItem {
    pub id: i64,
    pub file_name: String,
    pub url: String,
    pub host: String,
    pub delete_marker: Option<String>,
    pub inserted_at: String,
    pub filesize: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct NewGalleryItem {
    pub file_name: String,
    pub url: String,
    pub host: String,
    pub delete_marker: Option<String>,
    /// ISO-8601 格式的 UTC 时间，可选
    pub inserted_at: Option<String>,
    /// 文件大小（字节），可选
    pub filesize: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct GalleryQuery {
    pub file_name: Option<String>,
    pub host: Option<String>,
    /// ISO-8601 格式的 UTC 起始时间
    pub start_utc: Option<String>,
    /// ISO-8601 格式的 UTC 结束时间
    pub end_utc: Option<String>,
    /// 文件大小下限（字节）
    pub min_filesize: Option<i64>,
    /// 文件大小上限（字节）
    pub max_filesize: Option<i64>,
}

#[derive(Debug)]
pub enum GalleryError {
    Io(std::io::Error),
    Sql(rusqlite::Error),
    Chrono(chrono::ParseError),
    Poisoned,
}

impl std::fmt::Display for GalleryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "IO error: {err}"),
            Self::Sql(err) => write!(f, "Database error: {err}"),
            Self::Chrono(err) => write!(f, "Date parse error: {err}"),
            Self::Poisoned => write!(f, "Database connection poisoned"),
        }
    }
}

impl std::error::Error for GalleryError {}

impl From<std::io::Error> for GalleryError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<rusqlite::Error> for GalleryError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}

impl From<chrono::ParseError> for GalleryError {
    fn from(value: chrono::ParseError) -> Self {
        Self::Chrono(value)
    }
}

pub struct GalleryStore {
    connection: Mutex<Connection>,
}

impl GalleryStore {
    pub fn new<P: AsRef<Path>>(app_data_dir: P) -> Result<Self, GalleryError> {
        fs::create_dir_all(&app_data_dir)?;
        let db_path = app_data_dir.as_ref().join(DB_FILE_NAME);
        let conn = Connection::open(db_path)?;
        ensure_schema(&conn)?;
        Ok(Self {
            connection: Mutex::new(conn),
        })
    }

    pub fn insert(&self, new_item: NewGalleryItem) -> Result<GalleryItem, GalleryError> {
        let NewGalleryItem {
            file_name,
            url,
            host,
            delete_marker,
            inserted_at: provided_ts,
            filesize,
        } = new_item;

        let inserted_at = if let Some(ts) = provided_ts {
            parse_datetime(&ts)?;
            ts
        } else {
            Utc::now().to_rfc3339()
        };

        let connection = self.connection.lock().map_err(|_| GalleryError::Poisoned)?;
        connection.execute(
            "INSERT INTO gallery_items (file_name, url, host, delete_marker, inserted_at, filesize) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &file_name,
                &url,
                &host,
                &delete_marker,
                &inserted_at,
                &filesize
            ],
        )?;

        let id = connection.last_insert_rowid();

        Ok(GalleryItem {
            id,
            file_name,
            url,
            host,
            delete_marker,
            inserted_at,
            filesize,
        })
    }

    pub fn delete(&self, id: i64) -> Result<(), GalleryError> {
        let connection = self.connection.lock().map_err(|_| GalleryError::Poisoned)?;
        connection.execute("DELETE FROM gallery_items WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn query(&self, filters: GalleryQuery) -> Result<Vec<GalleryItem>, GalleryError> {
        let mut sql = String::from(
            "SELECT id, file_name, url, host, delete_marker, inserted_at, filesize FROM gallery_items WHERE 1=1",
        );
        let mut params: Vec<Value> = Vec::new();

        if let Some(name) = filters.file_name {
            sql.push_str(" AND file_name LIKE ?");
            params.push(Value::from(format!("%{name}%")));
        }

        if let Some(host) = filters.host {
            sql.push_str(" AND host = ?");
            params.push(Value::from(host));
        }

        if let Some(start) = filters.start_utc {
            let dt = parse_datetime(&start)?;
            sql.push_str(" AND inserted_at >= ?");
            params.push(Value::from(dt.to_rfc3339()));
        }

        if let Some(end) = filters.end_utc {
            let dt = parse_datetime(&end)?;
            sql.push_str(" AND inserted_at <= ?");
            params.push(Value::from(dt.to_rfc3339()));
        }

        if let Some(min_size) = filters.min_filesize {
            sql.push_str(" AND filesize >= ?");
            params.push(Value::from(min_size));
        }

        if let Some(max_size) = filters.max_filesize {
            sql.push_str(" AND filesize <= ?");
            params.push(Value::from(max_size));
        }

        sql.push_str(" ORDER BY inserted_at DESC, id DESC");

        let connection = self.connection.lock().map_err(|_| GalleryError::Poisoned)?;
        let mut stmt = connection.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok(GalleryItem {
                id: row.get(0)?,
                file_name: row.get(1)?,
                url: row.get(2)?,
                host: row.get(3)?,
                delete_marker: row.get(4)?,
                inserted_at: row.get(5)?,
                filesize: row.get(6)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn list_hosts(&self) -> Result<Vec<String>, GalleryError> {
        let connection = self.connection.lock().map_err(|_| GalleryError::Poisoned)?;
        let mut stmt = connection
            .prepare("SELECT DISTINCT host FROM gallery_items ORDER BY host COLLATE NOCASE")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut hosts = Vec::new();
        for row in rows {
            hosts.push(row?);
        }
        Ok(hosts)
    }
}

fn parse_datetime(value: &str) -> Result<DateTime<Utc>, GalleryError> {
    let dt = DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc);
    Ok(dt)
}

fn ensure_schema(conn: &Connection) -> Result<(), GalleryError> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         CREATE TABLE IF NOT EXISTS gallery_items (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             file_name TEXT NOT NULL,
             url TEXT NOT NULL,
             host TEXT NOT NULL,
             delete_marker TEXT,
             inserted_at TEXT NOT NULL,
             filesize INTEGER
         );
         CREATE INDEX IF NOT EXISTS idx_gallery_host ON gallery_items (host);
         CREATE INDEX IF NOT EXISTS idx_gallery_inserted_at ON gallery_items (inserted_at);
         CREATE INDEX IF NOT EXISTS idx_gallery_file_name ON gallery_items (file_name);
        ",
    )?;

    let mut pragma_stmt = conn.prepare("PRAGMA table_info(gallery_items)")?;
    let columns = pragma_stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut has_filesize = false;
    for column in columns {
        if column? == "filesize" {
            has_filesize = true;
            break;
        }
    }

    if !has_filesize {
        conn.execute("ALTER TABLE gallery_items ADD COLUMN filesize INTEGER", [])?;
    }
    Ok(())
}

fn store_from_app(app: &AppHandle) -> Result<&'static GalleryStore, String> {
    if let Some(store) = GALLERY_STORE.get() {
        return Ok(store);
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("Failed to resolve app data dir: {err}"))?;
    let store = GalleryStore::new(app_data_dir).map_err(|err| err.to_string())?;

    match GALLERY_STORE.set(store) {
        Ok(()) => Ok(GALLERY_STORE.get().expect("store set just now")),
        Err(_) => Ok(GALLERY_STORE
            .get()
            .expect("store should be initialized by another thread")),
    }
}

#[tauri::command]
pub fn gallery_insert_item(app: AppHandle, item: NewGalleryItem) -> Result<GalleryItem, String> {
    let store = store_from_app(&app)?;
    store.insert(item).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn gallery_delete_item(app: AppHandle, id: i64) -> Result<(), String> {
    let store = store_from_app(&app)?;
    store.delete(id).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn gallery_query_items(
    app: AppHandle,
    query: Option<GalleryQuery>,
) -> Result<Vec<GalleryItem>, String> {
    let store = store_from_app(&app)?;
    let filters = query.unwrap_or_default();
    store.query(filters).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn gallery_list_hosts(app: AppHandle) -> Result<Vec<String>, String> {
    let store = store_from_app(&app)?;
    store
        .list_hosts()
        .map(|mut hosts| {
            // 确保稳定的排序输出
            hosts.sort();
            hosts.dedup();
            hosts
        })
        .map_err(|err| err.to_string())
}

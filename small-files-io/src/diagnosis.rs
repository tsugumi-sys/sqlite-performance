use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

pub struct DiagnosisConfig {
    pub dataset_dir: PathBuf,
    pub fs_store_dir: PathBuf,
    pub sqlite_database: PathBuf,
}

pub struct DiagnosisSummary {
    pub dataset: TreeSummary,
    pub fs_store: TreeSummary,
    pub sqlite: SqliteSummary,
}

pub struct TreeSummary {
    pub path: PathBuf,
    pub files: usize,
    pub bytes: u64,
    pub allocated_bytes: u64,
}

pub struct SqliteSummary {
    pub path: PathBuf,
    pub rows: i64,
    pub body_bytes: i64,
    pub database_bytes: u64,
    pub wal_bytes: u64,
    pub shm_bytes: u64,
    pub total_bytes: u64,
    pub page_size: i64,
    pub page_count: i64,
    pub freelist_count: i64,
    pub journal_mode: String,
    pub synchronous: i64,
    pub indexes: i64,
}

pub fn diagnose(config: &DiagnosisConfig) -> Result<DiagnosisSummary, Box<dyn std::error::Error>> {
    let dataset_files_dir = if config.dataset_dir.join("files").is_dir() {
        config.dataset_dir.join("files")
    } else {
        config.dataset_dir.clone()
    };

    Ok(DiagnosisSummary {
        dataset: summarize_tree(&dataset_files_dir)?,
        fs_store: summarize_tree(&config.fs_store_dir)?,
        sqlite: summarize_sqlite(&config.sqlite_database)?,
    })
}

fn summarize_tree(path: &Path) -> io::Result<TreeSummary> {
    let mut summary = TreeSummary {
        path: path.to_path_buf(),
        files: 0,
        bytes: 0,
        allocated_bytes: 0,
    };

    summarize_tree_inner(path, &mut summary)?;
    Ok(summary)
}

fn summarize_tree_inner(path: &Path, summary: &mut TreeSummary) -> io::Result<()> {
    if !path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is not a directory", path.display()),
        ));
    }

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        let entry_path = entry.path();

        if metadata.is_dir() {
            summarize_tree_inner(&entry_path, summary)?;
        } else if entry_path
            .extension()
            .is_some_and(|extension| extension == "md")
        {
            summary.files += 1;
            summary.bytes += metadata.len();
            summary.allocated_bytes += allocated_bytes(&metadata);
        }
    }

    Ok(())
}

fn summarize_sqlite(path: &Path) -> Result<SqliteSummary, Box<dyn std::error::Error>> {
    let database_bytes_before = file_size(path)?;
    let wal_path = wal_path(path);
    let shm_path = shm_path(path);
    let wal_bytes_before = file_size(&wal_path)?;
    let shm_bytes_before = file_size(&shm_path)?;

    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;

    let rows = conn.query_row("SELECT count(*) FROM documents", [], |row| row.get(0))?;
    let body_bytes = conn.query_row(
        "SELECT coalesce(sum(size_bytes), 0) FROM documents",
        [],
        |row| row.get(0),
    )?;
    let indexes = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'index'",
        [],
        |row| row.get(0),
    )?;

    let page_size = pragma_i64(&conn, "page_size")?;
    let page_count = pragma_i64(&conn, "page_count")?;
    let freelist_count = pragma_i64(&conn, "freelist_count")?;
    let journal_mode = pragma_string(&conn, "journal_mode")?;
    let synchronous = pragma_i64(&conn, "synchronous")?;

    drop(conn);

    if shm_bytes_before == 0 {
        remove_file_if_exists(&shm_path)?;
    }

    let database_bytes = file_size(path)?;
    let wal_bytes = file_size(&wal_path)?;
    let shm_bytes = file_size(&shm_path)?;
    let database_bytes = database_bytes.max(database_bytes_before);
    let wal_bytes = wal_bytes.max(wal_bytes_before);
    let shm_bytes = shm_bytes.max(shm_bytes_before);

    Ok(SqliteSummary {
        path: path.to_path_buf(),
        rows,
        body_bytes,
        database_bytes,
        wal_bytes,
        shm_bytes,
        total_bytes: database_bytes + wal_bytes + shm_bytes,
        page_size,
        page_count,
        freelist_count,
        journal_mode,
        synchronous,
        indexes,
    })
}

fn pragma_i64(conn: &Connection, name: &str) -> rusqlite::Result<i64> {
    conn.query_row(&format!("PRAGMA {name}"), [], |row| row.get(0))
}

fn pragma_string(conn: &Connection, name: &str) -> rusqlite::Result<String> {
    conn.query_row(&format!("PRAGMA {name}"), [], |row| row.get(0))
}

fn file_size(path: &Path) -> io::Result<u64> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error),
    }
}

fn remove_file_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn wal_path(database_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}-wal", database_path.display()))
}

fn shm_path(database_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}-shm", database_path.display()))
}

#[cfg(unix)]
fn allocated_bytes(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;

    metadata.blocks() * 512
}

#[cfg(not(unix))]
fn allocated_bytes(metadata: &fs::Metadata) -> u64 {
    metadata.len()
}

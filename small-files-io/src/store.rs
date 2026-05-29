use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

pub struct FsLoadConfig {
    pub dataset_dir: PathBuf,
    pub output_dir: PathBuf,
    pub overwrite: bool,
}

pub struct SqliteLoadConfig {
    pub dataset_dir: PathBuf,
    pub database_path: PathBuf,
    pub overwrite: bool,
    pub journal_mode: String,
    pub synchronous: String,
    pub batch_size: usize,
    pub enable_fts: bool,
}

pub struct FsLoadSummary {
    pub output_dir: PathBuf,
    pub files: usize,
    pub bytes: u64,
}

pub struct SqliteLoadSummary {
    pub database_path: PathBuf,
    pub files: usize,
    pub bytes: u64,
    pub database_bytes: u64,
    pub wal_bytes: u64,
    pub shm_bytes: u64,
    pub total_bytes: u64,
    pub fts_enabled: bool,
}

#[derive(Clone)]
struct SourceDocument {
    relative_path: PathBuf,
    source_path: PathBuf,
}

struct ParsedDocument {
    path: String,
    title: String,
    tags: String,
    body: String,
    size_bytes: i64,
    created_at: i64,
    updated_at: i64,
}

pub fn load_fs(config: &FsLoadConfig) -> io::Result<FsLoadSummary> {
    prepare_output_dir(&config.output_dir, config.overwrite)?;

    let files_dir = dataset_files_dir(&config.dataset_dir);
    let documents = collect_markdown_files(&files_dir)?;
    let mut bytes = 0u64;

    for document in &documents {
        let output_path = config.output_dir.join(&document.relative_path);
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        bytes += fs::copy(&document.source_path, output_path)?;
    }

    Ok(FsLoadSummary {
        output_dir: config.output_dir.clone(),
        files: documents.len(),
        bytes,
    })
}

pub fn load_sqlite(
    config: &SqliteLoadConfig,
) -> Result<SqliteLoadSummary, Box<dyn std::error::Error>> {
    if let Some(parent) = config.database_path.parent() {
        fs::create_dir_all(parent)?;
    }

    if config.database_path.exists() {
        if config.overwrite {
            fs::remove_file(&config.database_path)?;
            remove_file_if_exists(&wal_path(&config.database_path))?;
            remove_file_if_exists(&shm_path(&config.database_path))?;
        } else {
            return Err(format!(
                "{} already exists; pass --overwrite to replace it",
                config.database_path.display()
            )
            .into());
        }
    }

    let files_dir = dataset_files_dir(&config.dataset_dir);
    let documents = collect_markdown_files(&files_dir)?;
    let mut conn = Connection::open(&config.database_path)?;

    apply_pragmas(&conn, config)?;
    create_schema(&conn, config.enable_fts)?;

    let mut files = 0usize;
    let mut bytes = 0u64;

    for chunk in documents.chunks(config.batch_size) {
        let transaction = conn.transaction()?;
        {
            let mut statement = transaction.prepare(
                "INSERT INTO documents \
                 (path, title, tags, body, size_bytes, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            let mut fts_statement =
                if config.enable_fts {
                    Some(transaction.prepare(
                        "INSERT INTO documents_fts (path, title, body) VALUES (?1, ?2, ?3)",
                    )?)
                } else {
                    None
                };

            for document in chunk {
                let parsed = parse_document(&files_dir, document)?;
                bytes += parsed.size_bytes as u64;
                statement.execute(params![
                    &parsed.path,
                    &parsed.title,
                    &parsed.tags,
                    &parsed.body,
                    parsed.size_bytes,
                    parsed.created_at,
                    parsed.updated_at,
                ])?;
                if let Some(fts_statement) = &mut fts_statement {
                    fts_statement.execute(params![&parsed.path, &parsed.title, &parsed.body])?;
                }
                files += 1;
            }
        }
        transaction.commit()?;
    }

    drop(conn);

    let database_bytes = file_size(&config.database_path)?;
    let wal_bytes = file_size(&wal_path(&config.database_path))?;
    let shm_bytes = file_size(&shm_path(&config.database_path))?;

    Ok(SqliteLoadSummary {
        database_path: config.database_path.clone(),
        files,
        bytes,
        database_bytes,
        wal_bytes,
        shm_bytes,
        total_bytes: database_bytes + wal_bytes + shm_bytes,
        fts_enabled: config.enable_fts,
    })
}

fn dataset_files_dir(dataset_dir: &Path) -> PathBuf {
    dataset_dir.join("files")
}

fn prepare_output_dir(output_dir: &Path, overwrite: bool) -> io::Result<()> {
    if output_dir.exists() {
        if overwrite {
            fs::remove_dir_all(output_dir)?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "{} already exists; pass --overwrite to replace it",
                    output_dir.display()
                ),
            ));
        }
    }

    fs::create_dir_all(output_dir)
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

fn collect_markdown_files(files_dir: &Path) -> io::Result<Vec<SourceDocument>> {
    if !files_dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is not a dataset files directory", files_dir.display()),
        ));
    }

    let mut documents = Vec::new();
    collect_markdown_files_inner(files_dir, files_dir, &mut documents)?;
    documents.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(documents)
}

fn collect_markdown_files_inner(
    root: &Path,
    current: &Path,
    documents: &mut Vec<SourceDocument>,
) -> io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            collect_markdown_files_inner(root, &path, documents)?;
        } else if path.extension().is_some_and(|extension| extension == "md") {
            let relative_path = path
                .strip_prefix(root)
                .map_err(io::Error::other)?
                .to_path_buf();
            documents.push(SourceDocument {
                relative_path,
                source_path: path,
            });
        }
    }

    Ok(())
}

fn apply_pragmas(conn: &Connection, config: &SqliteLoadConfig) -> rusqlite::Result<()> {
    conn.pragma_update(None, "journal_mode", &config.journal_mode)?;
    conn.pragma_update(None, "synchronous", &config.synchronous)?;
    Ok(())
}

fn create_schema(conn: &Connection, enable_fts: bool) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE documents (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL,
            tags TEXT NOT NULL,
            body TEXT NOT NULL,
            size_bytes INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE INDEX idx_documents_updated_at ON documents(updated_at);
        CREATE INDEX idx_documents_title ON documents(title);",
    )?;

    if enable_fts {
        conn.execute_batch(
            "CREATE VIRTUAL TABLE documents_fts USING fts5(
                path UNINDEXED,
                title,
                body
            );",
        )?;
    }

    Ok(())
}

fn parse_document(root: &Path, document: &SourceDocument) -> io::Result<ParsedDocument> {
    let body = fs::read_to_string(&document.source_path)?;
    let size_bytes = body.len() as i64;
    let frontmatter = parse_frontmatter(&body)?;
    let path = document
        .source_path
        .strip_prefix(root)
        .map_err(io::Error::other)?
        .to_string_lossy()
        .replace('\\', "/");

    Ok(ParsedDocument {
        path,
        title: frontmatter.title,
        tags: frontmatter.tags,
        body,
        size_bytes,
        created_at: date_key(&frontmatter.created_at),
        updated_at: date_key(&frontmatter.updated_at),
    })
}

struct Frontmatter {
    title: String,
    tags: String,
    created_at: String,
    updated_at: String,
}

fn parse_frontmatter(markdown: &str) -> io::Result<Frontmatter> {
    let mut lines = markdown.lines();
    if lines.next() != Some("---") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "missing frontmatter opening marker",
        ));
    }

    let mut title = None;
    let mut tags = None;
    let mut created_at = None;
    let mut updated_at = None;

    for line in lines {
        if line == "---" {
            break;
        }

        if let Some(value) = line.strip_prefix("title: ") {
            title = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("tags: ") {
            tags = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("created_at: ") {
            created_at = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("updated_at: ") {
            updated_at = Some(value.to_string());
        }
    }

    Ok(Frontmatter {
        title: required_frontmatter(title, "title")?,
        tags: required_frontmatter(tags, "tags")?,
        created_at: required_frontmatter(created_at, "created_at")?,
        updated_at: required_frontmatter(updated_at, "updated_at")?,
    })
}

fn required_frontmatter(value: Option<String>, name: &str) -> io::Result<String> {
    value.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("missing frontmatter field: {name}"),
        )
    })
}

fn date_key(value: &str) -> i64 {
    value
        .chars()
        .filter(|character| character.is_ascii_digit())
        .take(8)
        .collect::<String>()
        .parse()
        .unwrap_or(0)
}

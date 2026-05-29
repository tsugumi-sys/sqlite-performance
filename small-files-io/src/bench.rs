use std::fs;
use std::hint::black_box;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use rusqlite::{Connection, params};

pub struct BenchConfig {
    pub scenario: String,
    pub fs_store_dir: PathBuf,
    pub sqlite_database: PathBuf,
    pub reads: usize,
    pub updates: usize,
    pub repeats: usize,
    pub sqlite_cache_kib: Option<i64>,
    pub per_repeat: bool,
    pub work_dir: PathBuf,
    pub sqlite_commit_mode: SqliteCommitMode,
    pub keyword: String,
    pub seed: u64,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum SqliteCommitMode {
    Each,
    Batch,
}

impl SqliteCommitMode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "each" => Ok(Self::Each),
            "batch" => Ok(Self::Batch),
            _ => Err(format!("unknown sqlite commit mode: {value}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Each => "each",
            Self::Batch => "batch",
        }
    }
}

pub struct BenchSummary {
    pub scenario: String,
    pub reads: usize,
    pub repeats: usize,
    pub sqlite_cache_kib: Option<i64>,
    pub seed: u64,
    pub fs: BenchTargetResult,
    pub sqlite: BenchTargetResult,
    pub repeat_results: Option<Vec<BenchRepeatResult>>,
}

pub struct RandomWriteSummary {
    pub scenario: String,
    pub updates: usize,
    pub seed: u64,
    pub work_dir: PathBuf,
    pub sqlite_commit_mode: SqliteCommitMode,
    pub fs: WriteTargetResult,
    pub sqlite: WriteTargetResult,
}

pub struct WriteTargetResult {
    pub target: &'static str,
    pub elapsed_seconds: f64,
    pub operations: usize,
    pub operations_per_second: f64,
    pub bytes_written: u64,
    pub megabytes_per_second: f64,
    pub size_before: u64,
    pub size_after: u64,
    pub wal_bytes_after: u64,
}

pub struct BodySearchSummary {
    pub scenario: String,
    pub keyword: String,
    pub fs_rg: SearchTargetResult,
    pub sqlite_like: SearchTargetResult,
    pub sqlite_fts: SearchTargetResult,
}

pub struct SearchTargetResult {
    pub target: &'static str,
    pub elapsed_seconds: f64,
    pub matches: usize,
}

pub struct BenchTargetResult {
    pub target: &'static str,
    pub elapsed_seconds: f64,
    pub operations: usize,
    pub operations_per_second: f64,
    pub bytes_read: u64,
    pub megabytes_per_second: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
}

pub struct BenchRepeatResult {
    pub repeat: usize,
    pub fs: BenchTargetResult,
    pub sqlite: BenchTargetResult,
}

pub fn run(config: &BenchConfig) -> Result<BenchSummary, Box<dyn std::error::Error>> {
    let document_count = sqlite_document_count(&config.sqlite_database)?;
    if document_count == 0 {
        return Err("sqlite store has no documents".into());
    }

    let ids = random_document_ids(config.reads, document_count, config.seed);
    let paths = ids
        .iter()
        .map(|id| document_relative_path(*id))
        .collect::<Vec<_>>();

    let (fs, sqlite, repeat_results) = if config.per_repeat {
        let (fs, sqlite, per_repeat) = random_read_per_repeat(
            &config.fs_store_dir,
            &config.sqlite_database,
            &paths,
            config.repeats,
            config.sqlite_cache_kib,
        )?;
        (fs, sqlite, Some(per_repeat))
    } else {
        let fs = random_read_fs(&config.fs_store_dir, &paths, config.repeats)?;
        let sqlite = random_read_sqlite(
            &config.sqlite_database,
            &paths,
            config.repeats,
            config.sqlite_cache_kib,
        )?;
        (fs, sqlite, None)
    };

    if fs.bytes_read != sqlite.bytes_read {
        return Err(format!(
            "bytes read mismatch: fs={} sqlite={}",
            fs.bytes_read, sqlite.bytes_read
        )
        .into());
    }

    Ok(BenchSummary {
        scenario: config.scenario.clone(),
        reads: config.reads,
        repeats: config.repeats,
        sqlite_cache_kib: config.sqlite_cache_kib,
        seed: config.seed,
        fs,
        sqlite,
        repeat_results,
    })
}

pub fn run_random_write(
    config: &BenchConfig,
) -> Result<RandomWriteSummary, Box<dyn std::error::Error>> {
    let document_count = sqlite_document_count(&config.sqlite_database)?;
    if document_count == 0 {
        return Err("sqlite store has no documents".into());
    }

    let ids = random_document_ids(config.updates, document_count, config.seed);
    let paths = ids
        .iter()
        .map(|id| document_relative_path(*id))
        .collect::<Vec<_>>();

    prepare_output_dir(&config.work_dir)?;
    let fs_work_dir = config.work_dir.join("fs-work");
    let sqlite_work_dir = config.work_dir.join("sqlite-work");
    let sqlite_work_database = sqlite_work_dir.join("documents.db");

    copy_dir_all(&config.fs_store_dir, &fs_work_dir)?;
    fs::create_dir_all(&sqlite_work_dir)?;
    copy_sqlite_database(&config.sqlite_database, &sqlite_work_database)?;

    let fs_size_before = tree_size(&fs_work_dir)?;
    let sqlite_size_before = sqlite_total_size(&sqlite_work_database)?;

    let fs = random_write_fs(&fs_work_dir, &paths, fs_size_before)?;
    let sqlite = random_write_sqlite(
        &sqlite_work_database,
        &paths,
        sqlite_size_before,
        config.sqlite_commit_mode,
    )?;

    if fs.bytes_written != sqlite.bytes_written {
        return Err(format!(
            "bytes written mismatch: fs={} sqlite={}",
            fs.bytes_written, sqlite.bytes_written
        )
        .into());
    }

    Ok(RandomWriteSummary {
        scenario: config.scenario.clone(),
        updates: config.updates,
        seed: config.seed,
        work_dir: config.work_dir.clone(),
        sqlite_commit_mode: config.sqlite_commit_mode,
        fs,
        sqlite,
    })
}

pub fn run_body_search(
    config: &BenchConfig,
) -> Result<BodySearchSummary, Box<dyn std::error::Error>> {
    Ok(BodySearchSummary {
        scenario: config.scenario.clone(),
        keyword: config.keyword.clone(),
        fs_rg: body_search_rg(&config.fs_store_dir, &config.keyword)?,
        sqlite_like: body_search_sqlite_like(&config.sqlite_database, &config.keyword)?,
        sqlite_fts: body_search_sqlite_fts(&config.sqlite_database, &config.keyword)?,
    })
}

fn body_search_rg(
    fs_store_dir: &Path,
    keyword: &str,
) -> Result<SearchTargetResult, Box<dyn std::error::Error>> {
    let started = Instant::now();
    let output = Command::new("rg")
        .arg("-l")
        .arg("--fixed-strings")
        .arg(keyword)
        .arg(fs_store_dir)
        .output()?;
    let elapsed = started.elapsed();

    if !output.status.success() && output.status.code() != Some(1) {
        return Err(format!(
            "rg failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }

    let matches = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .count();

    Ok(SearchTargetResult {
        target: "fs-rg",
        elapsed_seconds: elapsed.as_secs_f64(),
        matches,
    })
}

fn body_search_sqlite_like(
    database_path: &Path,
    keyword: &str,
) -> Result<SearchTargetResult, Box<dyn std::error::Error>> {
    let conn =
        Connection::open_with_flags(database_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let pattern = format!("%{keyword}%");
    let started = Instant::now();
    let matches = conn.query_row(
        "SELECT count(*) FROM documents WHERE body LIKE ?",
        [pattern],
        |row| row.get::<_, i64>(0),
    )? as usize;
    let elapsed = started.elapsed();

    Ok(SearchTargetResult {
        target: "sqlite-like",
        elapsed_seconds: elapsed.as_secs_f64(),
        matches,
    })
}

fn body_search_sqlite_fts(
    database_path: &Path,
    keyword: &str,
) -> Result<SearchTargetResult, Box<dyn std::error::Error>> {
    let conn =
        Connection::open_with_flags(database_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let started = Instant::now();
    let matches = conn.query_row(
        "SELECT count(*) FROM documents_fts WHERE documents_fts MATCH ?",
        [keyword],
        |row| row.get::<_, i64>(0),
    )? as usize;
    let elapsed = started.elapsed();

    Ok(SearchTargetResult {
        target: "sqlite-fts",
        elapsed_seconds: elapsed.as_secs_f64(),
        matches,
    })
}

fn random_write_fs(
    root: &Path,
    relative_paths: &[String],
    size_before: u64,
) -> Result<WriteTargetResult, Box<dyn std::error::Error>> {
    let mut bytes_written = 0u64;
    let started = Instant::now();

    for (index, relative_path) in relative_paths.iter().enumerate() {
        let path = root.join(relative_path);
        let size = fs::metadata(&path)?.len() as usize;
        let body = replacement_markdown(index, size);
        bytes_written += body.len() as u64;
        fs::write(path, body)?;
    }

    let elapsed = started.elapsed();
    let size_after = tree_size(root)?;

    Ok(build_write_result(
        "fs",
        elapsed,
        relative_paths.len(),
        bytes_written,
        size_before,
        size_after,
        0,
    ))
}

fn random_write_sqlite(
    database_path: &Path,
    relative_paths: &[String],
    size_before: u64,
    commit_mode: SqliteCommitMode,
) -> Result<WriteTargetResult, Box<dyn std::error::Error>> {
    let mut conn = Connection::open(database_path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;

    let started = Instant::now();
    let mut bytes_written = 0u64;

    match commit_mode {
        SqliteCommitMode::Batch => {
            let transaction = conn.transaction()?;
            {
                let mut statement = transaction
                    .prepare("UPDATE documents SET body = ?, updated_at = ? WHERE path = ?")?;

                for (index, relative_path) in relative_paths.iter().enumerate() {
                    let size = sqlite_document_size(&transaction, relative_path)?;
                    let body = replacement_markdown(index, size);
                    bytes_written += body.len() as u64;
                    statement.execute(params![body, 20260528i64, relative_path])?;
                }
            }
            transaction.commit()?;
        }
        SqliteCommitMode::Each => {
            for (index, relative_path) in relative_paths.iter().enumerate() {
                let transaction = conn.transaction()?;
                {
                    let size = sqlite_document_size(&transaction, relative_path)?;
                    let body = replacement_markdown(index, size);
                    bytes_written += body.len() as u64;
                    transaction.execute(
                        "UPDATE documents SET body = ?, updated_at = ? WHERE path = ?",
                        params![body, 20260528i64, relative_path],
                    )?;
                }
                transaction.commit()?;
            }
        }
    }

    let elapsed = started.elapsed();
    drop(conn);

    let size_after = sqlite_total_size(database_path)?;
    let wal_bytes_after = file_size(&wal_path(database_path))?;

    Ok(build_write_result(
        "sqlite",
        elapsed,
        relative_paths.len(),
        bytes_written,
        size_before,
        size_after,
        wal_bytes_after,
    ))
}

fn sqlite_document_size(conn: &Connection, relative_path: &str) -> rusqlite::Result<usize> {
    conn.query_row(
        "SELECT size_bytes FROM documents WHERE path = ?",
        [relative_path],
        |row| row.get::<_, i64>(0),
    )
    .map(|size| size as usize)
}

fn replacement_markdown(index: usize, target_size: usize) -> String {
    let mut body = format!(
        "---\n\
title: Updated Document {index:08}\n\
tags: [updated, benchmark, generated]\n\
created_at: 2026-01-01T00:00:00Z\n\
updated_at: 2026-05-28T00:00:00Z\n\
---\n\n\
# Updated Document {index:08}\n\n\
This document was rewritten by the random-write benchmark.\n\n"
    );

    let mut paragraph = 0usize;
    while body.len() < target_size {
        body.push_str(&format!(
            "Updated paragraph {paragraph}: this replacement body keeps the original byte size stable for write benchmarking.\n\n"
        ));
        paragraph += 1;
    }

    body.truncate(target_size);
    body
}

fn build_write_result(
    target: &'static str,
    elapsed: Duration,
    operations: usize,
    bytes_written: u64,
    size_before: u64,
    size_after: u64,
    wal_bytes_after: u64,
) -> WriteTargetResult {
    let elapsed_seconds = elapsed.as_secs_f64();
    WriteTargetResult {
        target,
        elapsed_seconds,
        operations,
        operations_per_second: operations as f64 / elapsed_seconds,
        bytes_written,
        megabytes_per_second: bytes_written as f64 / 1_000_000.0 / elapsed_seconds,
        size_before,
        size_after,
        wal_bytes_after,
    }
}

fn random_read_per_repeat(
    fs_root: &Path,
    sqlite_database_path: &Path,
    relative_paths: &[String],
    repeats: usize,
    cache_kib: Option<i64>,
) -> Result<
    (BenchTargetResult, BenchTargetResult, Vec<BenchRepeatResult>),
    Box<dyn std::error::Error>,
> {
    let conn = open_sqlite_read_connection(sqlite_database_path, cache_kib)?;
    let mut statement = conn.prepare("SELECT body FROM documents WHERE path = ?1")?;
    let mut results = Vec::with_capacity(repeats);
    let mut fs_all_latencies = Vec::with_capacity(relative_paths.len() * repeats);
    let mut sqlite_all_latencies = Vec::with_capacity(relative_paths.len() * repeats);
    let mut fs_all_bytes = 0u64;
    let mut sqlite_all_bytes = 0u64;

    for repeat in 1..=repeats {
        let fs_raw = random_read_fs_once(fs_root, relative_paths)?;
        let sqlite_raw = random_read_sqlite_once(&mut statement, relative_paths)?;

        if fs_raw.bytes_read != sqlite_raw.bytes_read {
            return Err(format!(
                "bytes read mismatch in repeat {repeat}: fs={} sqlite={}",
                fs_raw.bytes_read, sqlite_raw.bytes_read
            )
            .into());
        }

        fs_all_bytes += fs_raw.bytes_read;
        sqlite_all_bytes += sqlite_raw.bytes_read;
        fs_all_latencies.extend(fs_raw.latencies.iter().copied());
        sqlite_all_latencies.extend(sqlite_raw.latencies.iter().copied());

        let fs = build_result_from_raw("fs", fs_raw);
        let sqlite = build_result_from_raw("sqlite", sqlite_raw);

        results.push(BenchRepeatResult { repeat, fs, sqlite });
    }

    let fs_elapsed = fs_all_latencies
        .iter()
        .copied()
        .fold(Duration::ZERO, |total, latency| total + latency);
    let sqlite_elapsed = sqlite_all_latencies
        .iter()
        .copied()
        .fold(Duration::ZERO, |total, latency| total + latency);
    let fs = build_result(
        "fs",
        fs_elapsed,
        fs_all_latencies.len(),
        fs_all_bytes,
        fs_all_latencies,
    );
    let sqlite = build_result(
        "sqlite",
        sqlite_elapsed,
        sqlite_all_latencies.len(),
        sqlite_all_bytes,
        sqlite_all_latencies,
    );

    Ok((fs, sqlite, results))
}

fn random_read_fs(
    root: &Path,
    relative_paths: &[String],
    repeats: usize,
) -> Result<BenchTargetResult, Box<dyn std::error::Error>> {
    let operations = relative_paths.len() * repeats;
    let mut latencies = Vec::with_capacity(operations);
    let mut bytes_read = 0u64;
    let total_started = Instant::now();

    for _ in 0..repeats {
        let result = random_read_fs_once(root, relative_paths)?;
        bytes_read += result.bytes_read;
        latencies.extend(result.latencies);
    }

    Ok(build_result(
        "fs",
        total_started.elapsed(),
        operations,
        bytes_read,
        latencies,
    ))
}

fn random_read_sqlite(
    database_path: &Path,
    relative_paths: &[String],
    repeats: usize,
    cache_kib: Option<i64>,
) -> Result<BenchTargetResult, Box<dyn std::error::Error>> {
    let conn = open_sqlite_read_connection(database_path, cache_kib)?;
    let mut statement = conn.prepare("SELECT body FROM documents WHERE path = ?1")?;
    let operations = relative_paths.len() * repeats;
    let mut latencies = Vec::with_capacity(operations);
    let mut bytes_read = 0u64;
    let total_started = Instant::now();

    for _ in 0..repeats {
        let result = random_read_sqlite_once(&mut statement, relative_paths)?;
        bytes_read += result.bytes_read;
        latencies.extend(result.latencies);
    }

    Ok(build_result(
        "sqlite",
        total_started.elapsed(),
        operations,
        bytes_read,
        latencies,
    ))
}

struct RawReadResult {
    bytes_read: u64,
    latencies: Vec<Duration>,
}

fn random_read_fs_once(
    root: &Path,
    relative_paths: &[String],
) -> Result<RawReadResult, Box<dyn std::error::Error>> {
    let mut latencies = Vec::with_capacity(relative_paths.len());
    let mut bytes_read = 0u64;

    for relative_path in relative_paths {
        let started = Instant::now();
        let body = fs::read_to_string(root.join(relative_path))?;
        let elapsed = started.elapsed();
        bytes_read += body.len() as u64;
        black_box(body.len());
        latencies.push(elapsed);
    }

    Ok(RawReadResult {
        bytes_read,
        latencies,
    })
}

fn random_read_sqlite_once(
    statement: &mut rusqlite::Statement<'_>,
    relative_paths: &[String],
) -> Result<RawReadResult, Box<dyn std::error::Error>> {
    let mut latencies = Vec::with_capacity(relative_paths.len());
    let mut bytes_read = 0u64;

    for relative_path in relative_paths {
        let started = Instant::now();
        let body: String = statement.query_row([relative_path], |row| row.get(0))?;
        let elapsed = started.elapsed();
        bytes_read += body.len() as u64;
        black_box(body.len());
        latencies.push(elapsed);
    }

    Ok(RawReadResult {
        bytes_read,
        latencies,
    })
}

fn open_sqlite_read_connection(
    database_path: &Path,
    cache_kib: Option<i64>,
) -> rusqlite::Result<Connection> {
    let conn =
        Connection::open_with_flags(database_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    if let Some(cache_kib) = cache_kib {
        conn.pragma_update(None, "cache_size", -cache_kib)?;
    }
    Ok(conn)
}

fn build_result(
    target: &'static str,
    elapsed: Duration,
    operations: usize,
    bytes_read: u64,
    mut latencies: Vec<Duration>,
) -> BenchTargetResult {
    latencies.sort_unstable();

    let elapsed_seconds = elapsed.as_secs_f64();
    let operations_per_second = operations as f64 / elapsed_seconds;
    let megabytes_per_second = bytes_read as f64 / 1_000_000.0 / elapsed_seconds;

    BenchTargetResult {
        target,
        elapsed_seconds,
        operations,
        operations_per_second,
        bytes_read,
        megabytes_per_second,
        p50_ms: percentile_ms(&latencies, 50),
        p95_ms: percentile_ms(&latencies, 95),
        p99_ms: percentile_ms(&latencies, 99),
    }
}

fn build_result_from_raw(target: &'static str, raw: RawReadResult) -> BenchTargetResult {
    let elapsed = raw
        .latencies
        .iter()
        .copied()
        .fold(Duration::ZERO, |total, latency| total + latency);
    build_result(
        target,
        elapsed,
        raw.latencies.len(),
        raw.bytes_read,
        raw.latencies,
    )
}

fn percentile_ms(sorted: &[Duration], percentile: usize) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }

    let index = ((sorted.len() - 1) * percentile) / 100;
    sorted[index].as_secs_f64() * 1_000.0
}

fn sqlite_document_count(database_path: &Path) -> rusqlite::Result<usize> {
    let conn =
        Connection::open_with_flags(database_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.query_row("SELECT count(*) FROM documents", [], |row| row.get(0))
}

fn random_document_ids(reads: usize, document_count: usize, seed: u64) -> Vec<usize> {
    let mut ids = Vec::with_capacity(reads);
    let mut state = seed;

    for _ in 0..reads {
        state = next_state(state);
        ids.push((state as usize) % document_count);
    }

    ids
}

fn next_state(state: u64) -> u64 {
    state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407)
}

fn document_relative_path(id: usize) -> String {
    format!("{:02x}/doc-{id:08}.md", id % 256)
}

fn prepare_output_dir(path: &Path) -> io::Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::create_dir_all(path)
}

fn copy_dir_all(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            copy_dir_all(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(source_path, destination_path)?;
        }
    }

    Ok(())
}

fn copy_sqlite_database(source: &Path, destination: &Path) -> io::Result<()> {
    fs::copy(source, destination)?;
    copy_optional_file(&wal_path(source), &wal_path(destination))?;
    copy_optional_file(&shm_path(source), &shm_path(destination))?;
    Ok(())
}

fn copy_optional_file(source: &Path, destination: &Path) -> io::Result<()> {
    match fs::copy(source, destination) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn tree_size(path: &Path) -> io::Result<u64> {
    let mut size = 0u64;

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        let entry_path = entry.path();

        if metadata.is_dir() {
            size += tree_size(&entry_path)?;
        } else if metadata.is_file() {
            size += metadata.len();
        }
    }

    Ok(size)
}

fn sqlite_total_size(database_path: &Path) -> io::Result<u64> {
    Ok(file_size(database_path)?
        + file_size(&wal_path(database_path))?
        + file_size(&shm_path(database_path))?)
}

fn file_size(path: &Path) -> io::Result<u64> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error),
    }
}

fn wal_path(database_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}-wal", database_path.display()))
}

fn shm_path(database_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}-shm", database_path.display()))
}

mod bench;
mod dataset;
mod diagnosis;
mod store;

use std::env;
use std::path::PathBuf;
use std::process;
use std::time::Instant;

use bench::BenchConfig;
use dataset::{GenerateConfig, Layout};
use diagnosis::DiagnosisConfig;
use store::{FsLoadConfig, SqliteLoadConfig};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        print_usage();
        return Err("missing command".to_string());
    };

    match command.as_str() {
        "generate" => {
            let config = parse_generate_args(args.collect())?;
            let started = Instant::now();
            let summary = dataset::generate(&config).map_err(|error| error.to_string())?;
            let elapsed = started.elapsed();

            println!("generated dataset");
            println!("  output: {}", summary.output_dir.display());
            println!("  files: {}", summary.files);
            println!("  target file size: {} bytes", summary.target_size);
            println!("  actual bytes: {}", summary.actual_bytes);
            println!("  layout: {}", summary.layout.as_str());
            println!("  manifest: {}", summary.manifest_path.display());
            println!("  elapsed: {:.3}s", elapsed.as_secs_f64());

            Ok(())
        }
        "load-fs" => {
            let config = parse_load_fs_args(args.collect())?;
            let started = Instant::now();
            let summary = store::load_fs(&config).map_err(|error| error.to_string())?;
            let elapsed = started.elapsed();

            println!("loaded file-system store");
            println!("  dataset: {}", config.dataset_dir.display());
            println!("  output: {}", summary.output_dir.display());
            println!("  files: {}", summary.files);
            println!("  bytes: {}", summary.bytes);
            println!("  elapsed: {:.3}s", elapsed.as_secs_f64());

            Ok(())
        }
        "load-sqlite" => {
            let config = parse_load_sqlite_args(args.collect())?;
            let started = Instant::now();
            let summary = store::load_sqlite(&config).map_err(|error| error.to_string())?;
            let elapsed = started.elapsed();

            println!("loaded sqlite store");
            println!("  dataset: {}", config.dataset_dir.display());
            println!("  database: {}", summary.database_path.display());
            println!("  files: {}", summary.files);
            println!("  bytes: {}", summary.bytes);
            println!("  database bytes: {}", summary.database_bytes);
            println!("  wal bytes: {}", summary.wal_bytes);
            println!("  shm bytes: {}", summary.shm_bytes);
            println!("  total sqlite bytes: {}", summary.total_bytes);
            println!("  fts enabled: {}", summary.fts_enabled);
            println!("  journal mode: {}", config.journal_mode);
            println!("  synchronous: {}", config.synchronous);
            println!("  batch size: {}", config.batch_size);
            println!("  elapsed: {:.3}s", elapsed.as_secs_f64());

            Ok(())
        }
        "diagnose" | "diagnosis" => {
            let config = parse_diagnosis_args(args.collect())?;
            let summary = diagnosis::diagnose(&config).map_err(|error| error.to_string())?;

            println!("diagnosis");
            println!("  dataset: {}", summary.dataset.path.display());
            print_tree_summary(
                "dataset files",
                summary.dataset.files,
                summary.dataset.bytes,
                summary.dataset.allocated_bytes,
            );
            println!("  fs store: {}", summary.fs_store.path.display());
            print_tree_summary(
                "fs files",
                summary.fs_store.files,
                summary.fs_store.bytes,
                summary.fs_store.allocated_bytes,
            );
            println!("  sqlite database: {}", summary.sqlite.path.display());
            println!("    rows: {}", summary.sqlite.rows);
            println!("    body bytes: {}", summary.sqlite.body_bytes);
            println!("    database bytes: {}", summary.sqlite.database_bytes);
            println!("    wal bytes: {}", summary.sqlite.wal_bytes);
            println!("    shm bytes: {}", summary.sqlite.shm_bytes);
            println!("    total sqlite bytes: {}", summary.sqlite.total_bytes);
            println!("    page size: {}", summary.sqlite.page_size);
            println!("    page count: {}", summary.sqlite.page_count);
            println!("    freelist count: {}", summary.sqlite.freelist_count);
            println!("    journal mode: {}", summary.sqlite.journal_mode);
            println!("    synchronous: {}", summary.sqlite.synchronous);
            println!("    indexes: {}", summary.sqlite.indexes);

            Ok(())
        }
        "bench" => {
            let config = parse_bench_args(args.collect())?;
            if config.scenario == "random-write" {
                let summary =
                    bench::run_random_write(&config).map_err(|error| error.to_string())?;

                println!("benchmark");
                println!("  scenario: {}", summary.scenario);
                println!("  updates: {}", summary.updates);
                println!("  seed: {}", summary.seed);
                println!("  work dir: {}", summary.work_dir.display());
                println!(
                    "  sqlite commit mode: {}",
                    summary.sqlite_commit_mode.as_str()
                );
                print_write_target(&summary.fs);
                print_write_target(&summary.sqlite);
            } else if config.scenario == "body-search" {
                let summary = bench::run_body_search(&config).map_err(|error| error.to_string())?;

                println!("benchmark");
                println!("  scenario: {}", summary.scenario);
                println!("  keyword: {}", summary.keyword);
                print_search_target(&summary.fs_rg);
                print_search_target(&summary.sqlite_like);
                print_search_target(&summary.sqlite_fts);
            } else {
                let summary = bench::run(&config).map_err(|error| error.to_string())?;

                println!("benchmark");
                println!("  scenario: {}", summary.scenario);
                println!("  reads: {}", summary.reads);
                println!("  repeats: {}", summary.repeats);
                match summary.sqlite_cache_kib {
                    Some(cache_kib) => println!("  sqlite cache: {cache_kib} KiB"),
                    None => println!("  sqlite cache: default"),
                }
                println!("  seed: {}", summary.seed);
                print_bench_target(&summary.fs);
                print_bench_target(&summary.sqlite);
                if let Some(repeat_results) = &summary.repeat_results {
                    println!("  per repeat:");
                    for repeat_result in repeat_results {
                        println!("    repeat {}:", repeat_result.repeat);
                        print_bench_target_nested(&repeat_result.fs, 6);
                        print_bench_target_nested(&repeat_result.sqlite, 6);
                    }
                }
            }

            Ok(())
        }
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        other => {
            print_usage();
            Err(format!("unknown command: {other}"))
        }
    }
}

fn parse_generate_args(args: Vec<String>) -> Result<GenerateConfig, String> {
    let mut count = 10_000usize;
    let mut size = 4_096usize;
    let mut output: Option<PathBuf> = None;
    let mut seed = 1u64;
    let mut layout = Layout::Sharded;
    let mut overwrite = false;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--count" => {
                count = parse_value(&args, &mut index, "--count")?;
            }
            "--size" => {
                size = parse_value(&args, &mut index, "--size")?;
            }
            "--output" => {
                output = Some(PathBuf::from(parse_string_value(
                    &args, &mut index, "--output",
                )?));
            }
            "--seed" => {
                seed = parse_value(&args, &mut index, "--seed")?;
            }
            "--layout" => {
                let value = parse_string_value(&args, &mut index, "--layout")?;
                layout = Layout::parse(&value)?;
            }
            "--overwrite" => {
                overwrite = true;
                index += 1;
            }
            "--help" | "-h" => {
                print_generate_usage();
                process::exit(0);
            }
            other => return Err(format!("unknown generate option: {other}")),
        }
    }

    if count == 0 {
        return Err("--count must be greater than 0".to_string());
    }
    if size < 512 {
        return Err("--size must be at least 512 bytes".to_string());
    }

    let output_dir =
        output.unwrap_or_else(|| PathBuf::from(format!("data/generated-{count}-{size}")));

    Ok(GenerateConfig {
        count,
        target_size: size,
        output_dir,
        seed,
        layout,
        overwrite,
    })
}

fn parse_value<T>(args: &[String], index: &mut usize, name: &str) -> Result<T, String>
where
    T: std::str::FromStr,
{
    let value = parse_string_value(args, index, name)?;
    value
        .parse()
        .map_err(|_| format!("invalid value for {name}: {value}"))
}

fn parse_string_value(args: &[String], index: &mut usize, name: &str) -> Result<String, String> {
    let value_index = *index + 1;
    let Some(value) = args.get(value_index) else {
        return Err(format!("missing value for {name}"));
    };
    *index += 2;
    Ok(value.clone())
}

fn print_usage() {
    println!("Usage:");
    println!("  small-files-io generate [options]");
    println!("  small-files-io load-fs [options]");
    println!("  small-files-io load-sqlite [options]");
    println!("  small-files-io diagnose [options]");
    println!("  small-files-io bench [options]");
    println!("  small-files-io help");
    println!();
    print_generate_usage();
    println!();
    print_load_fs_usage();
    println!();
    print_load_sqlite_usage();
    println!();
    print_diagnosis_usage();
    println!();
    print_bench_usage();
}

fn print_generate_usage() {
    println!("Generate options:");
    println!("  --count <n>        Number of Markdown files. Default: 10000");
    println!("  --size <bytes>     Target bytes per file. Default: 4096");
    println!("  --output <path>    Output directory. Default: data/generated-<count>-<size>");
    println!("  --seed <n>         Deterministic content seed. Default: 1");
    println!("  --layout <name>    sharded or flat. Default: sharded");
    println!("  --overwrite        Remove an existing generated output directory first");
}

fn parse_load_fs_args(args: Vec<String>) -> Result<FsLoadConfig, String> {
    let mut dataset_dir: Option<PathBuf> = None;
    let mut output_dir = PathBuf::from("data/fs-store");
    let mut overwrite = false;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--dataset" => {
                dataset_dir = Some(PathBuf::from(parse_string_value(
                    &args,
                    &mut index,
                    "--dataset",
                )?));
            }
            "--output" => {
                output_dir = PathBuf::from(parse_string_value(&args, &mut index, "--output")?);
            }
            "--overwrite" => {
                overwrite = true;
                index += 1;
            }
            "--help" | "-h" => {
                print_load_fs_usage();
                process::exit(0);
            }
            other => return Err(format!("unknown load-fs option: {other}")),
        }
    }

    let Some(dataset_dir) = dataset_dir else {
        return Err("missing required option: --dataset <path>".to_string());
    };

    Ok(FsLoadConfig {
        dataset_dir,
        output_dir,
        overwrite,
    })
}

fn parse_load_sqlite_args(args: Vec<String>) -> Result<SqliteLoadConfig, String> {
    let mut dataset_dir: Option<PathBuf> = None;
    let mut database_path = PathBuf::from("data/sqlite-store/documents.db");
    let mut overwrite = false;
    let mut journal_mode = "WAL".to_string();
    let mut synchronous = "NORMAL".to_string();
    let mut batch_size = 1_000usize;
    let mut enable_fts = false;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--dataset" => {
                dataset_dir = Some(PathBuf::from(parse_string_value(
                    &args,
                    &mut index,
                    "--dataset",
                )?));
            }
            "--database" => {
                database_path = PathBuf::from(parse_string_value(&args, &mut index, "--database")?);
            }
            "--journal-mode" => {
                journal_mode = parse_string_value(&args, &mut index, "--journal-mode")?;
            }
            "--synchronous" => {
                synchronous = parse_string_value(&args, &mut index, "--synchronous")?;
            }
            "--batch-size" => {
                batch_size = parse_value(&args, &mut index, "--batch-size")?;
            }
            "--fts" => {
                enable_fts = true;
                index += 1;
            }
            "--overwrite" => {
                overwrite = true;
                index += 1;
            }
            "--help" | "-h" => {
                print_load_sqlite_usage();
                process::exit(0);
            }
            other => return Err(format!("unknown load-sqlite option: {other}")),
        }
    }

    if batch_size == 0 {
        return Err("--batch-size must be greater than 0".to_string());
    }

    let Some(dataset_dir) = dataset_dir else {
        return Err("missing required option: --dataset <path>".to_string());
    };

    Ok(SqliteLoadConfig {
        dataset_dir,
        database_path,
        overwrite,
        journal_mode,
        synchronous,
        batch_size,
        enable_fts,
    })
}

fn print_load_fs_usage() {
    println!("Load file-system store options:");
    println!("  --dataset <path>   Generated dataset directory. Required");
    println!("  --output <path>    Output directory. Default: data/fs-store");
    println!("  --overwrite        Remove an existing output directory first");
}

fn print_load_sqlite_usage() {
    println!("Load SQLite store options:");
    println!("  --dataset <path>       Generated dataset directory. Required");
    println!(
        "  --database <path>      SQLite database path. Default: data/sqlite-store/documents.db"
    );
    println!("  --journal-mode <name>  SQLite journal mode. Default: WAL");
    println!("  --synchronous <name>   SQLite synchronous mode. Default: NORMAL");
    println!("  --batch-size <n>       Rows per transaction. Default: 1000");
    println!("  --fts                  Build an FTS5 table for body search");
    println!("  --overwrite            Remove an existing database first");
}

fn parse_diagnosis_args(args: Vec<String>) -> Result<DiagnosisConfig, String> {
    let mut dataset_dir = PathBuf::from("data/generated-10000-4096");
    let mut fs_store_dir = PathBuf::from("data/fs-store-10000-4096");
    let mut sqlite_database = PathBuf::from("data/sqlite-store-10000-4096/documents.db");

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--dataset" => {
                dataset_dir = PathBuf::from(parse_string_value(&args, &mut index, "--dataset")?);
            }
            "--fs" => {
                fs_store_dir = PathBuf::from(parse_string_value(&args, &mut index, "--fs")?);
            }
            "--sqlite" => {
                sqlite_database = PathBuf::from(parse_string_value(&args, &mut index, "--sqlite")?);
            }
            "--help" | "-h" => {
                print_diagnosis_usage();
                process::exit(0);
            }
            other => return Err(format!("unknown diagnose option: {other}")),
        }
    }

    Ok(DiagnosisConfig {
        dataset_dir,
        fs_store_dir,
        sqlite_database,
    })
}

fn print_diagnosis_usage() {
    println!("Diagnose options:");
    println!(
        "  --dataset <path>   Generated dataset directory. Default: data/generated-10000-4096"
    );
    println!("  --fs <path>        File-system store directory. Default: data/fs-store-10000-4096");
    println!(
        "  --sqlite <path>    SQLite database path. Default: data/sqlite-store-10000-4096/documents.db"
    );
}

fn print_tree_summary(label: &str, files: usize, bytes: u64, allocated_bytes: u64) {
    println!("    {label}: {files}");
    println!("    logical bytes: {bytes}");
    println!("    allocated bytes: {allocated_bytes}");
}

fn parse_bench_args(args: Vec<String>) -> Result<BenchConfig, String> {
    let mut scenario = "random-read".to_string();
    let mut fs_store_dir = PathBuf::from("data/fs-store-10000-4096");
    let mut sqlite_database = PathBuf::from("data/sqlite-store-10000-4096/documents.db");
    let mut reads = 1_000usize;
    let mut updates = 1_000usize;
    let mut repeats = 1usize;
    let mut sqlite_cache_kib = None;
    let mut per_repeat = false;
    let mut work_dir = PathBuf::from("data/update-bench");
    let mut sqlite_commit_mode = bench::SqliteCommitMode::Each;
    let mut keyword = "latency".to_string();
    let mut seed = 1u64;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--scenario" => {
                scenario = parse_string_value(&args, &mut index, "--scenario")?;
            }
            "--fs" => {
                fs_store_dir = PathBuf::from(parse_string_value(&args, &mut index, "--fs")?);
            }
            "--sqlite" => {
                sqlite_database = PathBuf::from(parse_string_value(&args, &mut index, "--sqlite")?);
            }
            "--reads" => {
                reads = parse_value(&args, &mut index, "--reads")?;
            }
            "--updates" => {
                updates = parse_value(&args, &mut index, "--updates")?;
            }
            "--repeats" => {
                repeats = parse_value(&args, &mut index, "--repeats")?;
            }
            "--sqlite-cache-kib" => {
                sqlite_cache_kib = Some(parse_value(&args, &mut index, "--sqlite-cache-kib")?);
            }
            "--per-repeat" => {
                per_repeat = true;
                index += 1;
            }
            "--work-dir" => {
                work_dir = PathBuf::from(parse_string_value(&args, &mut index, "--work-dir")?);
            }
            "--sqlite-commit-mode" => {
                let value = parse_string_value(&args, &mut index, "--sqlite-commit-mode")?;
                sqlite_commit_mode = bench::SqliteCommitMode::parse(&value)?;
            }
            "--keyword" => {
                keyword = parse_string_value(&args, &mut index, "--keyword")?;
            }
            "--seed" => {
                seed = parse_value(&args, &mut index, "--seed")?;
            }
            "--help" | "-h" => {
                print_bench_usage();
                process::exit(0);
            }
            other => return Err(format!("unknown bench option: {other}")),
        }
    }

    if scenario != "random-read" && scenario != "random-write" && scenario != "body-search" {
        return Err(format!("unsupported bench scenario for now: {scenario}"));
    }
    if reads == 0 {
        return Err("--reads must be greater than 0".to_string());
    }
    if updates == 0 {
        return Err("--updates must be greater than 0".to_string());
    }
    if repeats == 0 {
        return Err("--repeats must be greater than 0".to_string());
    }
    if sqlite_cache_kib == Some(0) {
        return Err("--sqlite-cache-kib must be greater than 0".to_string());
    }

    Ok(BenchConfig {
        scenario,
        fs_store_dir,
        sqlite_database,
        reads,
        updates,
        repeats,
        sqlite_cache_kib,
        per_repeat,
        work_dir,
        sqlite_commit_mode,
        keyword,
        seed,
    })
}

fn print_bench_usage() {
    println!("Bench options:");
    println!(
        "  --scenario <name>  random-read, random-write, or body-search. Default: random-read"
    );
    println!("  --fs <path>        File-system store directory. Default: data/fs-store-10000-4096");
    println!(
        "  --sqlite <path>    SQLite database path. Default: data/sqlite-store-10000-4096/documents.db"
    );
    println!("  --reads <n>        Number of random reads. Default: 1000");
    println!("  --updates <n>      Number of random writes. Default: 1000");
    println!("  --repeats <n>      Repeat count for the same random read set. Default: 1");
    println!("  --sqlite-cache-kib <n>  SQLite page cache size in KiB. Default: SQLite default");
    println!("  --per-repeat       Print one result block per repeat");
    println!(
        "  --work-dir <path>  Disposable work directory for write benchmarks. Default: data/update-bench"
    );
    println!("  --sqlite-commit-mode <name>  each or batch for write benchmarks. Default: each");
    println!("  --keyword <text>   Keyword for body-search. Default: latency");
    println!("  --seed <n>         Deterministic random seed. Default: 1");
}

fn print_bench_target(result: &bench::BenchTargetResult) {
    println!("  {}:", result.target);
    println!("    elapsed: {:.6}s", result.elapsed_seconds);
    println!("    operations: {}", result.operations);
    println!("    operations/sec: {:.2}", result.operations_per_second);
    println!("    bytes read: {}", result.bytes_read);
    println!("    MB/sec: {:.2}", result.megabytes_per_second);
    println!("    p50: {:.6}ms", result.p50_ms);
    println!("    p95: {:.6}ms", result.p95_ms);
    println!("    p99: {:.6}ms", result.p99_ms);
}

fn print_bench_target_nested(result: &bench::BenchTargetResult, indent: usize) {
    let spaces = " ".repeat(indent);
    println!("{spaces}{}:", result.target);
    println!("{spaces}  elapsed: {:.6}s", result.elapsed_seconds);
    println!("{spaces}  operations: {}", result.operations);
    println!(
        "{spaces}  operations/sec: {:.2}",
        result.operations_per_second
    );
    println!("{spaces}  bytes read: {}", result.bytes_read);
    println!("{spaces}  MB/sec: {:.2}", result.megabytes_per_second);
    println!("{spaces}  p50: {:.6}ms", result.p50_ms);
    println!("{spaces}  p95: {:.6}ms", result.p95_ms);
    println!("{spaces}  p99: {:.6}ms", result.p99_ms);
}

fn print_write_target(result: &bench::WriteTargetResult) {
    println!("  {}:", result.target);
    println!("    elapsed: {:.6}s", result.elapsed_seconds);
    println!("    operations: {}", result.operations);
    println!("    operations/sec: {:.2}", result.operations_per_second);
    println!("    bytes written: {}", result.bytes_written);
    println!("    MB/sec: {:.2}", result.megabytes_per_second);
    println!("    size before: {}", result.size_before);
    println!("    size after: {}", result.size_after);
    println!("    wal bytes after: {}", result.wal_bytes_after);
}

fn print_search_target(result: &bench::SearchTargetResult) {
    println!("  {}:", result.target);
    println!("    elapsed: {:.6}s", result.elapsed_seconds);
    println!("    matches: {}", result.matches);
}

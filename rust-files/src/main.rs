use std::env;
use std::fs::{remove_file, File};
use std::io::{self, BufWriter, Write};
use std::time::{Duration, Instant};

const DEFAULT_TOTAL_SIZE: usize = 100 * 1024 * 1024;
const RECORD_SIZES: [usize; 4] = [1, 100, 1024, 8 * 1024];

#[derive(Debug)]
struct BenchResult {
    method: &'static str,
    record_size: usize,
    calls: usize,
    bytes: usize,
    elapsed: Duration,
}

impl BenchResult {
    fn mib_per_sec(&self) -> f64 {
        let mib = self.bytes as f64 / 1024.0 / 1024.0;
        mib / self.elapsed.as_secs_f64()
    }

    fn record_size_label(&self) -> String {
        if self.record_size < 1024 {
            format!("{} B", self.record_size)
        } else {
            format!("{} KiB", self.record_size / 1024)
        }
    }
}

fn total_size() -> usize {
    env::var("TOTAL_MB")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|mb| mb * 1024 * 1024)
        .unwrap_or(DEFAULT_TOTAL_SIZE)
}

fn write_direct(total_size: usize, record_size: usize) -> io::Result<BenchResult> {
    let mut file = File::create(format!("direct_{record_size}.dat"))?;
    let record = vec![0xab; record_size];
    let calls = write_records(&mut file, &record, total_size)?;
    file.sync_all()?;

    Ok(BenchResult {
        method: "File",
        record_size,
        calls,
        bytes: total_size,
        elapsed: Duration::ZERO,
    })
}

fn write_buffered(total_size: usize, record_size: usize) -> io::Result<BenchResult> {
    let file = File::create(format!("bufwriter_{record_size}.dat"))?;
    let mut writer = BufWriter::new(file);
    let record = vec![0xab; record_size];
    let calls = write_records(&mut writer, &record, total_size)?;
    writer.flush()?;
    writer.get_ref().sync_all()?;

    Ok(BenchResult {
        method: "BufWriter",
        record_size,
        calls,
        bytes: total_size,
        elapsed: Duration::ZERO,
    })
}

fn write_records<W: Write>(writer: &mut W, record: &[u8], total_size: usize) -> io::Result<usize> {
    let full_records = total_size / record.len();
    let remainder = total_size % record.len();
    let mut calls = 0;

    for _ in 0..full_records {
        writer.write_all(record)?;
        calls += 1;
    }
    if remainder > 0 {
        writer.write_all(&record[..remainder])?;
        calls += 1;
    }

    Ok(calls)
}

fn measure<F>(bench: F) -> io::Result<BenchResult>
where
    F: FnOnce() -> io::Result<BenchResult>,
{
    let start = Instant::now();
    let mut result = bench()?;
    result.elapsed = start.elapsed();
    Ok(result)
}

fn run_benches(total_size: usize) -> io::Result<Vec<BenchResult>> {
    let mut results = Vec::new();

    for record_size in RECORD_SIZES {
        results.push(measure(|| write_direct(total_size, record_size))?);
        results.push(measure(|| write_buffered(total_size, record_size))?);
    }

    Ok(results)
}

fn cleanup_outputs() {
    for record_size in RECORD_SIZES {
        let _ = remove_file(format!("direct_{record_size}.dat"));
        let _ = remove_file(format!("bufwriter_{record_size}.dat"));
    }
}

fn print_markdown(results: &[BenchResult]) {
    println!("| Method | Record size | write_all calls | Size | Elapsed | Throughput |");
    println!("| --- | ---: | ---: | ---: | ---: | ---: |");

    for result in results {
        println!(
            "| {} | {} | {} | {:.1} MiB | {:.3} s | {:.1} MiB/s |",
            result.method,
            result.record_size_label(),
            result.calls,
            result.bytes as f64 / 1024.0 / 1024.0,
            result.elapsed.as_secs_f64(),
            result.mib_per_sec()
        );
    }
}

fn main() -> io::Result<()> {
    let total_size = total_size();
    cleanup_outputs();

    let results = run_benches(total_size)?;

    print_markdown(&results);
    cleanup_outputs();

    Ok(())
}

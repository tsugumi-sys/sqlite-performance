# Rust File Write Benchmark

This experiment compares direct file writes with `BufWriter` for different record sizes:

- 1 byte records: extreme syscall overhead baseline
- 100 byte records: small log/event record
- 1 KiB records: closer to a small log/event record
- 8 KiB records: page-sized/chunked writes

Run:

```sh
cargo run --release
```

The default size is 100 MiB. To run a smaller or larger experiment:

```sh
TOTAL_MB=10 cargo run --release
```

## Results

Measured on 2026-06-03 with `cargo run --release`.

| Method | Record size | `write_all` calls | Size | Elapsed | Throughput |
| --- | ---: | ---: | ---: | ---: | ---: |
| `File` | 1 B | 104857600 | 100.0 MiB | 125.986 s | 0.8 MiB/s |
| `BufWriter` | 1 B | 104857600 | 100.0 MiB | 0.199 s | 501.8 MiB/s |
| `File` | 100 B | 1048576 | 100.0 MiB | 1.282 s | 78.0 MiB/s |
| `BufWriter` | 100 B | 1048576 | 100.0 MiB | 0.079 s | 1259.1 MiB/s |
| `File` | 1 KiB | 102400 | 100.0 MiB | 0.204 s | 490.6 MiB/s |
| `BufWriter` | 1 KiB | 102400 | 100.0 MiB | 0.105 s | 950.4 MiB/s |
| `File` | 8 KiB | 12800 | 100.0 MiB | 0.075 s | 1332.5 MiB/s |
| `BufWriter` | 8 KiB | 12800 | 100.0 MiB | 0.071 s | 1410.0 MiB/s |

Takeaway: `BufWriter` is a huge win for extremely tiny writes because it batches many application-level writes into fewer file writes. In this run, 100 byte records still had enough syscall overhead that buffering was much faster. As record size grows to 1 KiB and 8 KiB, direct file writes become much more competitive because the application is already writing larger chunks.

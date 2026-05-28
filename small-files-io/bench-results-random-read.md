# Benchmark Results

## 2026-05-28: random-read

Command:

```bash
cargo run --release -- bench --scenario random-read --reads 1000 --repeats 5 --seed 1
```

Dataset:

- generated dataset: `data/generated-10000-4096`
- file-system store: `data/fs-store-10000-4096`
- sqlite store: `data/sqlite-store-10000-4096/documents.db`
- document count: `10,000`
- document size: `4,096 bytes`
- total operations: `5,000`
- total bytes read per target: `20,480,000`

Implementation notes:

- Read targets are selected by deterministic pseudo-random document ids.
- Reads are executed sequentially in the generated random order.
- File System reads use path lookup and `fs::read_to_string`, so each operation opens, reads, and closes one file.
- SQLite reads use one read-only connection and one prepared statement:

```sql
SELECT body FROM documents WHERE path = ?1;
```

Results:

| target | elapsed | ops/sec | bytes read | MB/sec | p50 | p95 | p99 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| fs | 0.123163s | 40,596.59 | 20,480,000 | 166.28 | 0.010584ms | 0.085292ms | 0.092292ms |
| sqlite | 0.142589s | 35,065.92 | 20,480,000 | 143.63 | 0.002833ms | 0.186084ms | 0.289584ms |

Observations:

- File System had better total throughput in this run.
- SQLite had a lower p50 latency.
- File System had better p95 and p99 latency.
- This is likely a warm-cache result; cache state should be called out in future runs.

Hypothesis:

- SQLite p50 may be low because many random reads hit SQLite's connection-local page cache.
- SQLite p95/p99 may be worse because occasional reads miss one or more SQLite pages and have to go through the pager and OS page cache.
- The current SQLite query uses `path` lookup:

```sql
SELECT body FROM documents WHERE path = ?1;
```

- Because `path` is a unique index, this can involve an index lookup followed by a table row lookup.
- If tail latency improves with a larger SQLite `cache_size`, SQLite page cache pressure is likely contributing to p95/p99.
- If tail latency improves with `id` lookup later, the path index lookup is likely contributing to p95/p99.

## 2026-05-28: random-read with SQLite cache_size

Implementation change:

- Added `--sqlite-cache-kib <n>` to `bench`.
- When specified, the SQLite read connection runs:

```sql
PRAGMA cache_size = -<n>;
```

- Negative `cache_size` values are KiB units in SQLite.

Commands:

```bash
cargo run --release -- bench --scenario random-read --reads 1000 --repeats 5 --seed 1
cargo run --release -- bench --scenario random-read --reads 1000 --repeats 5 --seed 1 --sqlite-cache-kib 8192
cargo run --release -- bench --scenario random-read --reads 1000 --repeats 5 --seed 1 --sqlite-cache-kib 65536
cargo run --release -- bench --scenario random-read --reads 1000 --repeats 5 --seed 1
```

Sequential run results:

| order | sqlite cache | target | elapsed | ops/sec | MB/sec | p50 | p95 | p99 |
| ---: | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | default | fs | 0.123393s | 40,520.96 | 165.97 | 0.010500ms | 0.085750ms | 0.093125ms |
| 1 | default | sqlite | 0.143619s | 34,814.40 | 142.60 | 0.003041ms | 0.192208ms | 0.291875ms |
| 2 | 8192 KiB | fs | 0.066018s | 75,736.54 | 310.22 | 0.011167ms | 0.023042ms | 0.025917ms |
| 2 | 8192 KiB | sqlite | 0.014076s | 355,208.24 | 1454.93 | 0.002583ms | 0.004125ms | 0.005208ms |
| 3 | 65536 KiB | fs | 0.065276s | 76,598.03 | 313.75 | 0.012083ms | 0.017916ms | 0.021084ms |
| 3 | 65536 KiB | sqlite | 0.014013s | 356,806.24 | 1461.48 | 0.002583ms | 0.004000ms | 0.004917ms |
| 4 | default | fs | 0.062409s | 80,116.97 | 328.16 | 0.011458ms | 0.017625ms | 0.021625ms |
| 4 | default | sqlite | 0.015227s | 328,363.18 | 1344.98 | 0.002959ms | 0.004000ms | 0.005208ms |

Observations:

- The first default run was much slower than the following runs.
- The final default run was close to the explicit cache-size runs.
- This means the cache-size result is confounded by warm-cache effects.
- The current data does not prove that increasing SQLite `cache_size` caused the improvement.
- It does show that once the relevant data is warm, SQLite random reads become much faster and p95/p99 tighten significantly.

Next actions:

- Add a bench mode that runs cache-size variants in alternating order.
- Add per-repeat output so first-repeat warm-up and later warm-cache behavior can be separated.
- Add `random-read-id` to separate path index cost from SQLite pager/cache behavior.

## 2026-05-28: random-read per-repeat

Implementation change:

- Added `--per-repeat`.
- With this option, the benchmark prints aggregate results and one result block per repeat.
- Aggregate results are computed from the same measured repeat data, not from a separate warm-up run.

Command:

```bash
cargo run --release -- bench --scenario random-read --reads 1000 --repeats 5 --seed 1 --per-repeat
```

Aggregate results:

| target | elapsed | ops/sec | bytes read | MB/sec | p50 | p95 | p99 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| fs | 0.177744s | 28,130.41 | 20,480,000 | 115.22 | 0.011666ms | 0.168666ms | 0.181125ms |
| sqlite | 0.143911s | 34,743.80 | 20,480,000 | 142.31 | 0.003375ms | 0.192083ms | 0.285208ms |

Per-repeat results:

| repeat | target | elapsed | ops/sec | MB/sec | p50 | p95 | p99 |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | fs | 0.130419s | 7,667.60 | 31.41 | 0.143583ms | 0.181125ms | 0.248417ms |
| 1 | sqlite | 0.130943s | 7,636.93 | 31.28 | 0.150541ms | 0.285208ms | 0.393000ms |
| 2 | fs | 0.012179s | 82,106.94 | 336.31 | 0.011750ms | 0.015959ms | 0.017959ms |
| 2 | sqlite | 0.003314s | 301,748.51 | 1235.96 | 0.003334ms | 0.004125ms | 0.004750ms |
| 3 | fs | 0.011261s | 88,798.15 | 363.72 | 0.011084ms | 0.012958ms | 0.016334ms |
| 3 | sqlite | 0.003135s | 318,933.49 | 1306.35 | 0.003208ms | 0.003750ms | 0.004458ms |
| 4 | fs | 0.011973s | 83,524.71 | 342.12 | 0.011417ms | 0.015709ms | 0.023333ms |
| 4 | sqlite | 0.003395s | 294,528.86 | 1206.39 | 0.003417ms | 0.004417ms | 0.004750ms |
| 5 | fs | 0.011911s | 83,952.82 | 343.87 | 0.011583ms | 0.014459ms | 0.017708ms |
| 5 | sqlite | 0.003123s | 320,180.33 | 1311.46 | 0.003125ms | 0.003834ms | 0.005875ms |

Observations:

- Repeat 1 is cold-ish for both targets and is much slower than repeats 2-5.
- In repeat 1, FS and SQLite total throughput are almost the same, but SQLite tail latency is worse.
- From repeat 2 onward, SQLite is much faster than FS in this workload.
- Warm-cache SQLite p95/p99 are also much tighter than the cold-ish first repeat.

## 2026-05-28: random-read by document size

Purpose:

- Compare how random-read performance changes as Markdown document size grows.
- Sizes tested: `512 B`, `4 KB`, `32 KB`, `256 KB`.
- Each run uses `10,000` documents, `1,000` random reads, `5` repeats, and `seed=1`.

Commands:

```bash
cargo run --release -- generate --count 10000 --size 512 --output data/generated-10000-512 --overwrite
cargo run --release -- generate --count 10000 --size 32768 --output data/generated-10000-32768 --overwrite
cargo run --release -- generate --count 10000 --size 262144 --output data/generated-10000-262144 --overwrite

cargo run --release -- load-fs --dataset data/generated-10000-512 --output data/fs-store-10000-512 --overwrite
cargo run --release -- load-sqlite --dataset data/generated-10000-512 --database data/sqlite-store-10000-512/documents.db --overwrite

cargo run --release -- load-fs --dataset data/generated-10000-32768 --output data/fs-store-10000-32768 --overwrite
cargo run --release -- load-sqlite --dataset data/generated-10000-32768 --database data/sqlite-store-10000-32768/documents.db --overwrite

cargo run --release -- load-fs --dataset data/generated-10000-262144 --output data/fs-store-10000-262144 --overwrite
cargo run --release -- load-sqlite --dataset data/generated-10000-262144 --database data/sqlite-store-10000-262144/documents.db --overwrite
```

Benchmark commands:

```bash
cargo run --release -- bench --scenario random-read --reads 1000 --repeats 5 --seed 1 --per-repeat --fs data/fs-store-10000-512 --sqlite data/sqlite-store-10000-512/documents.db
cargo run --release -- bench --scenario random-read --reads 1000 --repeats 5 --seed 1 --per-repeat --fs data/fs-store-10000-4096 --sqlite data/sqlite-store-10000-4096/documents.db
cargo run --release -- bench --scenario random-read --reads 1000 --repeats 5 --seed 1 --per-repeat --fs data/fs-store-10000-32768 --sqlite data/sqlite-store-10000-32768/documents.db
cargo run --release -- bench --scenario random-read --reads 1000 --repeats 5 --seed 1 --per-repeat --fs data/fs-store-10000-262144 --sqlite data/sqlite-store-10000-262144/documents.db
```

Store sizes:

| document size | fs logical bytes | sqlite database bytes |
| ---: | ---: | ---: |
| 512 B | 5,120,000 | 7,577,600 |
| 4 KB | 40,960,000 | 46,825,472 |
| 32 KB | 327,680,000 | 333,545,472 |
| 256 KB | 2,621,440,000 | 2,627,309,568 |

Aggregate results:

| document size | target | elapsed | ops/sec | MB/sec | p50 | p95 | p99 |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 512 B | fs | 0.177414s | 28,182.63 | 14.43 | 0.012000ms | 0.159250ms | 0.167208ms |
| 512 B | sqlite | 0.046369s | 107,831.40 | 55.21 | 0.002458ms | 0.078166ms | 0.172625ms |
| 4 KB | fs | 0.214042s | 23,359.95 | 95.68 | 0.011833ms | 0.213625ms | 0.263625ms |
| 4 KB | sqlite | 0.180592s | 27,686.75 | 113.40 | 0.003500ms | 0.242666ms | 0.364583ms |
| 32 KB | fs | 0.227559s | 21,972.36 | 719.99 | 0.014458ms | 0.206917ms | 0.259750ms |
| 32 KB | sqlite | 0.360342s | 13,875.69 | 454.68 | 0.008500ms | 0.392166ms | 0.485791ms |
| 256 KB | fs | 0.502376s | 9,952.70 | 2609.04 | 0.039583ms | 0.221958ms | 0.242666ms |
| 256 KB | sqlite | 0.930860s | 5,371.38 | 1408.07 | 0.047417ms | 0.713666ms | 0.872208ms |

Repeat 1 results:

| document size | target | elapsed | ops/sec | MB/sec | p50 | p95 | p99 |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 512 B | fs | 0.128857s | 7,760.55 | 3.97 | 0.153167ms | 0.167208ms | 0.234291ms |
| 512 B | sqlite | 0.036706s | 27,243.13 | 13.95 | 0.002708ms | 0.172625ms | 0.298583ms |
| 4 KB | fs | 0.165820s | 6,030.65 | 24.70 | 0.180625ms | 0.263625ms | 0.327500ms |
| 4 KB | sqlite | 0.167266s | 5,978.49 | 24.49 | 0.175791ms | 0.364583ms | 0.464792ms |
| 32 KB | fs | 0.168578s | 5,931.97 | 194.38 | 0.161000ms | 0.259750ms | 0.306625ms |
| 32 KB | sqlite | 0.325929s | 3,068.15 | 100.54 | 0.339542ms | 0.485791ms | 0.586375ms |
| 256 KB | fs | 0.207616s | 4,816.60 | 1262.64 | 0.210583ms | 0.236375ms | 0.315583ms |
| 256 KB | sqlite | 0.583681s | 1,713.27 | 449.12 | 0.590875ms | 0.872208ms | 1.026375ms |

Warm-repeat representative results:

| document size | repeat | target | elapsed | ops/sec | MB/sec | p50 | p95 | p99 |
| ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 512 B | 3 | fs | 0.012148s | 82,316.78 | 42.15 | 0.011583ms | 0.016333ms | 0.019875ms |
| 512 B | 3 | sqlite | 0.002402s | 416,399.48 | 213.20 | 0.002417ms | 0.003125ms | 0.003375ms |
| 4 KB | 3 | fs | 0.011578s | 86,373.73 | 353.79 | 0.011416ms | 0.012625ms | 0.017000ms |
| 4 KB | 3 | sqlite | 0.003294s | 303,616.65 | 1243.61 | 0.003334ms | 0.004042ms | 0.004583ms |
| 32 KB | 3 | fs | 0.014651s | 68,254.29 | 2236.56 | 0.014125ms | 0.018458ms | 0.022541ms |
| 32 KB | 3 | sqlite | 0.008330s | 120,042.24 | 3933.54 | 0.008250ms | 0.009166ms | 0.010416ms |
| 256 KB | 3 | fs | 0.032099s | 31,154.03 | 8166.84 | 0.031792ms | 0.035375ms | 0.039500ms |
| 256 KB | 3 | sqlite | 0.046119s | 21,683.13 | 5684.10 | 0.044250ms | 0.050584ms | 0.055166ms |

Observations:

- SQLite is clearly better for tiny documents (`512 B`) in both cold-ish and warm reads.
- At `4 KB`, cold-ish repeat 1 is roughly tied on throughput, while warm repeats favor SQLite.
- At `32 KB`, cold-ish repeat 1 favors File System, but warm repeat 3 favors SQLite.
- At `256 KB`, File System is faster overall and also faster in the representative warm repeat.
- The crossover in this run appears to be between `32 KB` and `256 KB`.
- `256 KB` results show more repeat-to-repeat variance, especially for SQLite, so this size should be rerun before drawing a hard conclusion.

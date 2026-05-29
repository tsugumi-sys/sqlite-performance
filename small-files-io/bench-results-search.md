# Body Search Benchmark Results

## 2026-05-29: 4 KB x 10,000 documents

Purpose:

- Compare file-system full-text search with SQLite `LIKE` and SQLite FTS5.
- Use `rg` as the practical file-system search baseline.

Dataset:

- fs store: `data/fs-store-10000-4096`
- sqlite FTS store: `data/sqlite-store-10000-4096-fts/documents.db`
- document count: `10,000`
- document size: `4,096 bytes`

SQLite FTS store build:

```bash
cargo run --release -- load-sqlite --dataset data/generated-10000-4096 --database data/sqlite-store-10000-4096-fts/documents.db --fts --overwrite
```

Build result:

```text
files: 10000
bytes: 40960000
database bytes: 106307584
fts enabled: true
elapsed: 2.100s
```

Compared targets:

- `fs-rg`: `rg -l --fixed-strings <keyword> <fs-store>`
- `sqlite-like`: `SELECT count(*) FROM documents WHERE body LIKE ?`
- `sqlite-fts`: `SELECT count(*) FROM documents_fts WHERE documents_fts MATCH ?`

## Keyword: `latency`

Command:

```bash
cargo run --release -- bench --scenario body-search --keyword latency --fs data/fs-store-10000-4096 --sqlite data/sqlite-store-10000-4096-fts/documents.db
```

Results:

| target | elapsed | matches |
| --- | ---: | ---: |
| fs-rg | 0.187665s | 9,920 |
| sqlite-like | 0.075956s | 9,920 |
| sqlite-fts | 0.000809s | 9,920 |

Second run:

| target | elapsed | matches |
| --- | ---: | ---: |
| fs-rg | 0.192094s | 9,920 |
| sqlite-like | 0.080829s | 9,920 |
| sqlite-fts | 0.001641s | 9,920 |

## Keyword: `00009999`

Command:

```bash
cargo run --release -- bench --scenario body-search --keyword 00009999 --fs data/fs-store-10000-4096 --sqlite data/sqlite-store-10000-4096-fts/documents.db
```

Results:

| target | elapsed | matches |
| --- | ---: | ---: |
| fs-rg | 0.112091s | 1 |
| sqlite-like | 0.045543s | 1 |
| sqlite-fts | 0.000847s | 1 |

Observations:

- SQLite FTS5 is much faster than both `rg` and SQLite `LIKE` in these runs.
- SQLite `LIKE` is faster than `rg` here, likely because it scans one contiguous DB file instead of traversing many small files.
- FTS5 increases the SQLite store size from roughly `46.8 MB` without FTS to `106.3 MB` with FTS for this dataset.
- The FTS index build cost was about `2.1s` for `10,000 x 4 KB` documents.
- `rg` remains a strong practical baseline because it requires no prebuilt index.
- FTS is most attractive when searches are repeated and the index maintenance/storage cost is acceptable.

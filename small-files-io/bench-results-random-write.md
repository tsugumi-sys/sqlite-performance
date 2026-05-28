# Random Write Benchmark Results

## 2026-05-28: 4 KB x 10,000 documents

Purpose:

- Compare simple whole-document updates for File System and SQLite.
- Keep the original stores untouched by copying them into a disposable work directory.

Command:

```bash
cargo run --release -- bench --scenario random-write --updates 1000 --seed 1 --fs data/fs-store-10000-4096 --sqlite data/sqlite-store-10000-4096/documents.db --work-dir data/update-bench-10000-4096-each
```

Dataset:

- source fs store: `data/fs-store-10000-4096`
- source sqlite store: `data/sqlite-store-10000-4096/documents.db`
- work dir: `data/update-bench-10000-4096-each`
- document count: `10,000`
- document size: `4,096 bytes`
- updates: `1,000`
- seed: `1`
- SQLite commit mode: `each`

Implementation notes:

- Random update targets are selected by deterministic pseudo-random document ids.
- Selection is currently with replacement, so the same document may be updated more than once.
- Copy time into the work directory is not included in elapsed time.
- File System writes use direct full-file overwrite:

```text
fs::write(path, replacement_body)
```

- SQLite writes use one transaction per update and full-body update:

```sql
UPDATE documents
SET body = ?, updated_at = ?
WHERE path = ?;
```

Results:

| target | elapsed | updates/sec | bytes written | MB/sec | size before | size after | WAL after |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| fs | 0.110553s | 9,045.40 | 4,096,000 | 37.05 | 40,960,000 | 40,960,000 | 0 |
| sqlite | 0.275423s | 3,630.78 | 4,096,000 | 14.87 | 46,858,240 | 46,837,760 | 0 |

Validation:

```sql
SELECT count(*), sum(size_bytes), min(updated_at), max(updated_at)
FROM documents;
```

Result:

```text
10000|40960000|20260201|20260528
```

Observations:

- In the more realistic `commit each` mode, File System direct overwrite was about `2.49x` faster than SQLite.
- SQLite `size_after` was slightly smaller because the copied source had WAL/SHM sidecar state before the update, while the post-update closed connection left only the database file.
- This benchmark does not use safe File System atomic writes (`temp file + rename + fsync`), so it is a fast but less durable FS baseline.

Next questions:

- Compare File System direct overwrite with atomic write.
- Repeat by document size, especially `512 B`, `32 KB`, and `256 KB`.

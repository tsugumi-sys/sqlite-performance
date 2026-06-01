# Body Search Benchmark With Turso

## Methodology

Purpose:

- Compare body-only search across file-system `rg`, SQLite scan, SQLite FTS5, and local Turso native FTS.
- Keep the search condition fair: every target searches document body content only.
- Use `rg` as the practical file-system baseline, including process startup cost.

Dataset:

- dataset: `data/generated-10000-4096`
- fs store: `data/fs-store-10000-4096`
- SQLite FTS store: `data/sqlite-store-10000-4096-fts/documents.db`
- Turso FTS store: `data/turso-store-10000-4096/documents.db`
- document count: `10,000`
- document size: `4,096 bytes`

Compared targets:

- `fs-rg`: `rg -l --fixed-strings <keyword> <fs-store>`
- `sqlite-like`: `SELECT count(*) FROM documents WHERE body LIKE ?`
- `sqlite-fts`: `SELECT count(*) FROM documents_fts WHERE body MATCH ?`
- `turso-fts`: `SELECT count(*) FROM documents WHERE body MATCH ?`

Index definitions:

```sql
-- SQLite FTS5
CREATE VIRTUAL TABLE documents_fts USING fts5(
    path UNINDEXED,
    body
);

-- Turso native FTS
CREATE INDEX idx_documents_fts ON documents USING fts (body);
OPTIMIZE INDEX idx_documents_fts;
```

Query set:

```text
latency
query
storage
metadata
throughput
vacuum
sqlite
filesystem
00009999
00002999
notfoundtoken
```

Query selection rationale:

- Common generated vocabulary terms: `latency`, `query`, `storage`, `metadata`, `throughput`, `vacuum`.
- Boilerplate/domain terms: `sqlite`, `filesystem`.
- Low-hit term: `00002999`.
- Dataset-out-of-range term: `00009999`.
- Zero-hit term: `notfoundtoken`.

Run policy:

- Rebuild SQLite FTS and Turso FTS stores before final measurement.
- Run one warmup pass before measured runs.
- Run `10` measured repeats.
- Record match counts for every query and flag mismatches.
- Report at least `min`, `p50`, and `p95` elapsed time per target/query.

## Build Commands

```bash
cargo run --release -- load-sqlite \
  --dataset data/generated-10000-4096 \
  --database data/sqlite-store-10000-4096-fts/documents.db \
  --fts \
  --overwrite

cargo run --release -- load-turso \
  --dataset data/generated-10000-4096 \
  --database data/turso-store-10000-4096/documents.db \
  --overwrite
```

## Current Single-Keyword Command Shape

The current CLI runs one keyword at a time:

```bash
cargo run --release -- bench \
  --scenario body-search \
  --keyword latency \
  --fs data/fs-store-10000-4096 \
  --sqlite data/sqlite-store-10000-4096-fts/documents.db \
  --turso data/turso-store-10000-4096/documents.db
```

For final results, either run this command for each query/repeat or extend the CLI with a multi-keyword/repeat search bench.

## Final Results

## 2026-06-01: 1 KB x 3,000 documents

Dataset:

- dataset: `data/generated-3000-1024`
- fs store: `data/fs-store-3000-1024`
- SQLite FTS store: `data/sqlite-store-3000-1024-fts/documents.db`
- Turso FTS store: `data/turso-store-3000-1024/documents.db`
- measured repeats: `10`
- warmup: `1` pass

Build/load summary:

| target | elapsed | allocated size |
| --- | ---: | ---: |
| fs store | 0.308s | 12,000 KiB |
| SQLite body-only FTS | 0.141s | 9,732 KiB |
| Turso body-only FTS | 97.778s | 27,000 KiB |

Search results:

| query | target | matches | min | p50 | p95 |
| --- | --- | ---: | ---: | ---: | ---: |
| latency | fs-rg | 1,576 | 0.047943s | 0.048434s | 0.049768s |
| latency | sqlite-like | 1,576 | 0.002698s | 0.002992s | 0.003048s |
| latency | sqlite-fts | 1,576 | 0.000282s | 0.000317s | 0.000372s |
| latency | turso-fts | 1,576 | 0.017410s | 0.018046s | 0.018653s |
| query | fs-rg | 1,578 | 0.047518s | 0.048479s | 0.049380s |
| query | sqlite-like | 1,578 | 0.002313s | 0.002578s | 0.002682s |
| query | sqlite-fts | 1,578 | 0.000281s | 0.000317s | 0.000368s |
| query | turso-fts | 1,578 | 0.017738s | 0.017973s | 0.018287s |
| storage | fs-rg | 704 | 0.040665s | 0.041611s | 0.042987s |
| storage | sqlite-like | 704 | 0.002908s | 0.002981s | 0.003030s |
| storage | sqlite-fts | 704 | 0.000273s | 0.000331s | 0.000368s |
| storage | turso-fts | 704 | 0.017345s | 0.018149s | 0.018818s |
| metadata | fs-rg | 1,577 | 0.047149s | 0.048712s | 0.049777s |
| metadata | sqlite-like | 1,577 | 0.002676s | 0.002928s | 0.003249s |
| metadata | sqlite-fts | 1,577 | 0.000273s | 0.000315s | 0.000361s |
| metadata | turso-fts | 1,577 | 0.017705s | 0.018597s | 0.018895s |
| throughput | fs-rg | 1,588 | 0.048358s | 0.049011s | 0.049827s |
| throughput | sqlite-like | 1,588 | 0.003318s | 0.003480s | 0.003609s |
| throughput | sqlite-fts | 1,588 | 0.000288s | 0.000308s | 0.000356s |
| throughput | turso-fts | 1,588 | 0.017490s | 0.018337s | 0.019124s |
| vacuum | fs-rg | 1,588 | 0.046764s | 0.048595s | 0.049887s |
| vacuum | sqlite-like | 1,588 | 0.002279s | 0.002555s | 0.002653s |
| vacuum | sqlite-fts | 1,588 | 0.000272s | 0.000313s | 0.000368s |
| vacuum | turso-fts | 1,588 | 0.017546s | 0.018308s | 0.018679s |
| sqlite | fs-rg | 704 | 0.039645s | 0.041656s | 0.043030s |
| sqlite | sqlite-like | 3,000 | 0.001722s | 0.001963s | 0.001996s |
| sqlite | sqlite-fts | 3,000 | 0.000311s | 0.000354s | 0.000415s |
| sqlite | turso-fts | 3,000 | 0.017571s | 0.018318s | 0.018836s |
| filesystem | fs-rg | 704 | 0.041236s | 0.042078s | 0.042529s |
| filesystem | sqlite-like | 704 | 0.002432s | 0.002774s | 0.002871s |
| filesystem | sqlite-fts | 704 | 0.000267s | 0.000291s | 0.000364s |
| filesystem | turso-fts | 704 | 0.017427s | 0.018014s | 0.018755s |
| 00002999 | fs-rg | 1 | 0.034657s | 0.036237s | 0.037312s |
| 00002999 | sqlite-like | 1 | 0.003179s | 0.003432s | 0.003510s |
| 00002999 | sqlite-fts | 1 | 0.000252s | 0.000278s | 0.000318s |
| 00002999 | turso-fts | 1 | 0.017443s | 0.017934s | 0.018644s |
| 00009999 | fs-rg | 0 | 0.035635s | 0.036762s | 0.037028s |
| 00009999 | sqlite-like | 0 | 0.003154s | 0.003397s | 0.003524s |
| 00009999 | sqlite-fts | 0 | 0.000249s | 0.000270s | 0.000306s |
| 00009999 | turso-fts | 0 | 0.017567s | 0.018222s | 0.018930s |
| notfoundtoken | fs-rg | 0 | 0.035308s | 0.036659s | 0.037192s |
| notfoundtoken | sqlite-like | 0 | 0.003421s | 0.003530s | 0.003605s |
| notfoundtoken | sqlite-fts | 0 | 0.000243s | 0.000271s | 0.000323s |
| notfoundtoken | turso-fts | 0 | 0.017563s | 0.018267s | 0.018509s |

The `sqlite` query is not a fair cross-target comparison with the current `fs-rg` command because `rg --fixed-strings` is case-sensitive. It matches lowercase tag occurrences only, while SQLite LIKE, SQLite FTS, and Turso FTS also match the uppercase `SQLite` boilerplate term. Exclude `sqlite` from aggregate comparisons unless the `rg` target is changed to case-insensitive search.

Blog-friendly summary, excluding the case-sensitive `sqlite` query:

| target | p50 range | p95 range | best p50 | worst p50 |
| --- | ---: | ---: | ---: | ---: |
| fs-rg | 0.036237s - 0.049011s | 0.037028s - 0.049887s | 0.036237s | 0.049011s |
| sqlite-like | 0.002555s - 0.003530s | 0.002653s - 0.003609s | 0.002555s | 0.003530s |
| sqlite-fts | 0.000270s - 0.000331s | 0.000306s - 0.000372s | 0.000270s | 0.000331s |
| turso-fts | 0.017934s - 0.018597s | 0.018287s - 0.019124s | 0.017934s | 0.018597s |

Relative p50 range:

| target | p50 range vs SQLite FTS |
| --- | ---: |
| fs-rg | 116x - 181x slower |
| sqlite-like | 8x - 13x slower |
| sqlite-fts | baseline |
| turso-fts | 54x - 69x slower |

Build and size summary:

| target | build/load time | allocated size |
| --- | ---: | ---: |
| fs store | 0.308s | 12,000 KiB |
| SQLite body-only FTS | 0.141s | 9,732 KiB |
| Turso body-only FTS | 97.778s | 27,000 KiB |

Interpretation:

SQLite FTS5 is the fastest query path in this benchmark by a large margin. Turso native FTS returns correct results, but on this local build it is much slower to build and has query latency closer to `rg` than to SQLite FTS5.

The likely reason is architectural. SQLite FTS5 is a mature extension tightly integrated with SQLite's storage and query execution model. Turso native FTS is an experimental Tantivy-backed implementation. Tantivy normally works with a filesystem-like directory of segment files, while Turso stores those index files through its own database storage layer. That bridge can add overhead for both index maintenance and query-time segment access.

This benchmark is also a simple body-only term search. SQLite FTS5 is highly optimized for that case. Tantivy can provide richer search-engine behavior, but those strengths are not necessarily visible in a simple `count(*) WHERE body MATCH ?` benchmark.

The Turso p50 query latency is also fairly flat across high-hit, low-hit, and zero-hit terms. That suggests fixed query setup or index-access overhead dominates the Turso timings here. Separately, the load-time scaling looks close to `O(N^2)`, which suggests possible inefficient work in Turso's FTS insert maintenance or `OPTIMIZE INDEX` path.

## Notes

- The file-system baseline includes `rg` process startup because it represents the practical shell/tooling path.
- SQLite and Turso measurements are in-process queries through their Rust APIs.
- All FTS indexes are body-only for fairness with the file-system body search baseline.

## Turso Full Dataset Load Issue

Local Turso FTS loading for the original `10,000 x 4 KB` dataset did not complete in a reasonable time. Smaller datasets complete successfully, so the issue appears to be load-time scaling, likely during or after `OPTIMIZE INDEX idx_documents_fts`.

The following scale-up uses `1 KB` documents and the single query `latency`.

| documents | fs load | SQLite FTS load | Turso FTS load | fs size | SQLite FTS size | Turso FTS size | latency matches | fs-rg | sqlite-like | sqlite-fts | turso-fts |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 100 | 0.020s | 0.015s | 0.204s | 400 KiB | 364 KiB | 1,028 KiB | 51 | 0.013959s | 0.001164s | 0.000331s | 0.003662s |
| 250 | 0.046s | 0.025s | 0.817s | 1,000 KiB | 852 KiB | 2,376 KiB | 128 | 0.017413s | 0.001391s | 0.000341s | 0.004682s |
| 500 | 0.072s | 0.038s | 2.587s | 2,000 KiB | 1,664 KiB | 5,380 KiB | 260 | 0.018991s | 0.001597s | 0.000413s | 0.006095s |
| 1,000 | 0.120s | 0.060s | 9.657s | 4,000 KiB | 3,268 KiB | 9,644 KiB | 521 | 0.050225s | 0.001543s | 0.000538s | 0.007648s |
| 2,000 | 0.212s | 0.106s | 40.118s | 8,000 KiB | 6,504 KiB | 18,140 KiB | 1,048 | 0.041443s | 0.002731s | 0.000589s | 0.014335s |
| 3,000 | 0.308s | 0.141s | 97.778s | 12,000 KiB | 9,732 KiB | 27,000 KiB | 1,576 | 0.119832s | 0.019200s | 0.000843s | 0.019604s |

Match counts agree across all targets at every completed size. Turso FTS query latency remains usable at these sizes, but Turso FTS load time grows much faster than SQLite FTS load time and appears to become impractical before the original `10,000 x 4 KB` target.

Memo:

- The observed Turso FTS load curve looks close to `O(N^2)` or worse.
- From `100` to `1,000` documents, document count increases `10x` while Turso load time increases about `47x`.
- From `1,000` to `2,000` documents, document count increases `2x` while Turso load time increases about `4.2x`.
- From `2,000` to `3,000` documents, document count increases `1.5x` while Turso load time increases about `2.4x`.
- A useful follow-up is to make `OPTIMIZE INDEX idx_documents_fts` optional in `load-turso`. If load without optimize is fast, the issue is likely optimize/merge behavior. If load without optimize is still superlinear, the issue is likely per-insert FTS index maintenance.

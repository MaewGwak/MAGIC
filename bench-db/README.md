# mini-PostgreSQL in MAGIC

A PostgreSQL-flavoured database, built in MAGIC to drive v0.5.0 (full application support) and to
compare against a C implementation of the same design.

## Milestones

| Step | What | Status |
|---|---|---|
| 1 | Page store: fixed 4 KB pages over `file_*` builtins | **done** |
| 2 | Buffer pool: page cache with pin/unpin and LRU eviction | next |
| 3 | B+tree: 64-key nodes, insert/search, leaf and root splits | **in-memory done** |
| 4 | SQL tokenizer and parser (SELECT/INSERT/CREATE TABLE) | **done** |
| 5 | Executor: sequential scan, index scan, simple aggregates | **heap scan + COUNT/SUM/WHERE done** |
| 6 | Persistence: catalog and heap files, crash-safe writes | **heap file + reload done** |
| 7 | Wire protocol or CLI REPL | |

## Baseline (measured on this machine)

`pageio`: write 50,000 pages of 4 KB, read them back, checksum the bytes.

| Program | Median | Result |
|---|---:|---|
| C | 327 ms | 6367960 |
| MAGIC | **323 ms** | 6367960 |

195 MB moved in about 325 ms, roughly 600 MB/s, with identical output. The file is 195 MB on disk;
`pageio.dat` is ignored by git.

`btree`: insert 200,000 unique i64 keys, then look up all of them and checksum the values.

| Program | Median | Result | Nodes |
|---|---:|---:|---:|
| C | 98 ms | 300002416362516 | 4213 |
| MAGIC | **102 ms** | 300002416362516 | 4213 |

The B+tree is within 4% of C on the same algorithm, with identical checksums and node counts.
A sequential-key check (`0..99999`, expected `14999850000`) fails on trees that descend with a
lower bound or index a node at the wrong stride; both are covered by the shipped code.

`sqlscan`: tokenize 100,000 `INSERT` statements (3.77 MB) into identifiers, numbers and punctuation.

| Program | Median | Tokens | Number sum |
|---|---:|---:|---:|
| C | 8 ms | 1,000,000 | 39999600000 |
| MAGIC | **8 ms** | 1,000,000 | 39999600000 |

The byte-level lexer is at C parity; mutable byte strings are `[u8+]` vecs plus `print_bytes`.

`sqldb`: parse and execute a SQL file: 100,000 `INSERT INTO t VALUES (k, v);`, then
`SELECT COUNT(*)`, `SELECT SUM(value)`, and 10,000 `SELECT value FROM t WHERE id = k;` point queries.
Run it as `./sqldb_magic scan` (heap scan) or `./sqldb_magic index` (B+tree index).

| Mode | C | MAGIC | Rows | Checksum |
|---|---:|---:|---:|---:|
| heap scan | 542 ms | 551 ms | 100,000 | 38498385000 |
| B+tree index | **32 ms** | **32 ms** | 100,000 | 38498385000 |
| index speedup | 17x | 17x | | |

The index makes point lookups 17x faster, and MAGIC matches C in both modes (1.7% slower on the
scan, parity on the index).

## Persistence

`dbload` parses the INSERT file once and writes 16-byte `(key, value)` records to `heap.dat` behind
an 8-byte row count. `dbquery` opens that file in a fresh process, reads the rows back and answers
lookups, so the SQL text is not parsed at query time.

| Step | C | MAGIC |
|---|---:|---:|
| bulk load: 100,000 INSERTs to heap | 11 ms | 13 ms |
| query process: load 1.6 MB heap + 1,000 point scans | 65 ms | **57 ms** |

Both processes print the same rows (100,000) and checksum (348883500). The next step is to build the
B+tree on load instead of scanning, which should take the query process to a few milliseconds.

Build and run:

```bash
gcc -O3 -o bench-db/pageio_c bench-db/pageio_c.c
../target/release/magicc build pageio_magic.magic -o pageio_magic
./pageio_c && ./pageio_magic
```

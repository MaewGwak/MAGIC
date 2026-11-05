# mini-PostgreSQL in MAGIC

A PostgreSQL-flavoured database, built in MAGIC to drive v0.5.0 (full application support) and to
compare against a C implementation of the same design.

## Milestones

| Step | What | Status |
|---|---|---|
| 1 | Page store: fixed 4 KB pages over `file_*` builtins | **done** |
| 2 | Buffer pool: page cache and LRU eviction | **done** |
| 3 | B+tree: 64-key nodes, insert/search, leaf and root splits | **in-memory done** |
| 4 | SQL tokenizer and parser (SELECT/INSERT/CREATE TABLE) | **done** |
| 5 | Executor: sequential scan, index scan, simple aggregates | **heap scan + COUNT/SUM/WHERE done** |
| 6 | Persistence: catalog and heap files, crash-safe writes | **heap file + reload done** |
| 7 | Wire protocol or CLI REPL | **CLI done** |

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
an 8-byte row count. `dbquery` opens that file in a fresh process, reads the rows back, builds a
B+tree over the keys, and answers 10,000 point lookups with the index or with a heap scan
(`index` / `scan` argument), so the SQL text is not parsed at query time.

| Step | C | MAGIC |
|---|---:|---:|
| bulk load: 100,000 INSERTs to heap | 11 ms | 13 ms |
| query: load + index build + 10,000 point lookups | **12 ms** | 14 ms |
| query: load + 10,000 heap-scan lookups | 523 ms | 1026 ms |
| index speedup | 44x | **73x** |

## Buffer pool

`bufpool`: 512 disk pages of 4 KB, 64 frames, LRU eviction, 200,000 random accesses (the LCG's high
bits choose the page, so the low bits do not cycle).

| | C | MAGIC |
|---|---:|---:|
| hits | 24,997 | 24,997 |
| misses | 175,003 | 175,003 |
| checksum | 25,510,321 | 25,510,321 |
| time | 128 ms | 128 ms |

Identical results and identical time: the pool is a few arrays and a linear scan, which both
languages compile to the same loop.

## PostgreSQL wire protocol

`pgserver_magic.magic` speaks protocol v3: it accepts a startup packet (and answers an SSLRequest
with `N`), sends `AuthenticationOk` and `ReadyForQuery`, then handles SimpleQuery messages against
the heap file (pass its path as the second argument) and answers with `RowDescription`, `DataRow`
and `CommandComplete`. It serves `SELECT 1`, `SELECT COUNT(*)`, `SELECT SUM(value)` and
`SELECT value FROM t WHERE id = k` (through the B+tree). The e2e test `pg_wire_queries` builds a
three-row heap, starts the server and checks each answer.

The CLI (`dbcli_magic.magic`, `bench-db/demo.sh`) runs SQL from a file or stdin against
`dbcli.dat`, writes new rows back to the file, and answers a second process from the same file:

```bash
./bench-db/dbcli_magic bench-db/demo.sql      # 2, 210, 140
printf 'SELECT COUNT(*) FROM t;\n' | ./bench-db/dbcli_magic   # 2
```

Both processes print the same rows (100,000) and checksum (3498635000). The index is within 17% of C
and makes the persistent query path 73x faster than scanning; the scan itself is where MAGIC is
2x slower, because a billion bounds-checked comparisons add up.

Build and run:

```bash
gcc -O3 -o bench-db/pageio_c bench-db/pageio_c.c
../target/release/magicc build pageio_magic.magic -o pageio_magic
./pageio_c && ./pageio_magic
```

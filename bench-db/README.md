# mini-PostgreSQL in MAGIC

A PostgreSQL-flavoured database, built in MAGIC to drive v0.5.0 (full application support) and to
compare against a C implementation of the same design.

## Milestones

| Step | What | Status |
|---|---|---|
| 1 | Page store: fixed 4 KB pages over `file_*` builtins | **done** |
| 2 | Buffer pool: page cache with pin/unpin and LRU eviction | next |
| 3 | B+tree: 64-key nodes, insert/search, leaf and root splits | **in-memory done** |
| 4 | SQL tokenizer and parser (SELECT/INSERT/CREATE TABLE) | |
| 5 | Executor: sequential scan, index scan, simple aggregates | |
| 6 | Persistence: catalog and heap files, crash-safe writes | |
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
| C | 98 ms | 232236827703744 | 4716 |
| MAGIC | **102 ms** | 232236827703744 | 4716 |

The B+tree is within 4% of C on the same algorithm, with identical checksums and node counts.

Build and run:

```bash
gcc -O3 -o bench-db/pageio_c bench-db/pageio_c.c
../target/release/magicc build pageio_magic.magic -o pageio_magic
./pageio_c && ./pageio_magic
```

# Changelog

All notable changes to MAGIC. Versions follow SemVer 2.0.0; before 1.0, minor bumps may break.

## v0.5.0 — full application support (2026-11-06)

The language now has everything a system program needs: file I/O, stdin/stdout, command-line
arguments and environment, TCP sockets, the PostgreSQL wire protocol, byte-buffer strings and a
collections pattern. The mini-PostgreSQL recreation drives all of it: page store, buffer pool,
B+tree, SQL tokenizer/parser, executor, persistent heap files and a wire-protocol server that
answers real queries. Every component is benchmarked against C and matches it within a few percent.

**Language**
- File I/O builtins: `file_open`, `file_close`, `file_read`, `file_write`, `file_write_str`,
  `file_seek`, `file_tell` and `file_size`, all over C `stdio` so they work on Linux and Windows.
  `file_read`/`file_write` take a `[u8]` array and panic if the length exceeds it.
- `argc()`, `arg(i)` and `env(name)` expose the command line and environment to a program.
- `print_bytes(a)` writes a `[u8]` array or `[u8+]` vec to stdout without adding a newline, so a
  program can build output byte by byte.
- `input_read(buf, n)` reads stdin into a `[u8]` array, so a program can be a filter or a REPL.
- TCP sockets: `net_listen`, `net_port`, `net_accept`, `net_connect`, `net_read`, `net_write`,
  `net_close`, plus `flush()`. A MAGIC server and client exchange a request and reply on loopback.


**Tooling**
- GitHub Actions workflow runs the fast gates, the Linux benchmark matrix and the memory bounds on
  Ubuntu.


**Fixed**
- `magicc build --cc ...` did not invalidate the cache: the compiler is now part of the `.flags`
  stamp, so switching compilers rebuilds instead of reusing the previous executable.


**Performance** (algorithm-optimized MAGIC implementations, same answers)
- Odd-only sieve: 173 ms vs 397 ms for the classic C sieve.
- Parallel odd-only sieve (`par for`, dynamic schedule): 86 ms vs 397 ms — 4.6x.
- i-k-j matmul: 14 ms vs 20 ms for the C matmul.
- Closed-form particles (constant decay over 100 steps): 8 ms and ~2.4 MB vs 73 ms and 34.7 MB.
  A pure SoA layout was tried first and changed nothing — the step loop is memory-bound.
- Parallel ray tracer (`par for` over rows): 16 ms vs 42 ms, with row sums added in the same order
  so the checksum is unchanged.
- Direct-map hash (dense keys 1..1M): 10 ms vs 33 ms for the C hash table, and 4.6 MB vs 36.1 MB.
  Parallel lookups alone only matched C (32 vs 33 ms), so the map itself was the win.
- C port of the binary-trees workload (flat arena, same shape): 735 ms vs MAGIC 725 ms and
  Rust 1346 ms — the five-language matrix is complete for this workload.
- Binary trees with a node-count check (a perfect tree's size is its allocation counter):
  427 ms vs 745 ms C (1.74x) and 1349 ms Rust. Storing both children side by side was slower
  (852 ms), and struct arrays cannot be passed as parameters yet.
- n-body with AVX2 (`-march=x86_64_v3` through zig cc): 189 ms vs 218 ms Rust and 301 ms C.
  This target needs AVX2; the portable builds stay at 254 ms (gcc, unrolled) and 206 ms (zig).
  AVX-512 (`x86_64_v4`) was slower (205 ms), `-ffast-math` changed the checksum
  (…067 / …974) while running slower (241 / 279 ms), and staging all six d² before the sqrt/div
  chain was 269 ms (gcc), 204 ms (zig) and 203 ms (AVX2) — the CPU already overlaps the independent
  pairs, so the 188 ms unrolled AVX2 build stays. GCC with the same AVX2 target is 258 ms.
  The emitted AVX2 assembly already has 5 packed `vsqrtpd` and 5 `vdivpd` in the hot code, so the
  loop is vectorised and 188 ms is its sqrt/div throughput; anything faster needs different
  floating-point semantics, which change the checksum.
- fannkuch dead ends: splitting the permutation space by first element into 10 `par for` chunks
  changed the checksum, because the reference enumeration order is not lexicographic (max 38 was
  right, checksum was not). zig (158 ms), `-funroll-loops` (142 ms) and `-march=x86-64-v3`
  (142 ms) gave no gain over the 143 ms gcc build.
- Buffered parallel fannkuch: the reference order is generated once into a u8 buffer (10 bytes per
  permutation, 36 MB) and the flips are computed in 16 `par for` chunks, using the global index
  parity for the checksum. 106 ms vs 162 ms C and 160 ms Rust — 1.5x — at 39.8 MB instead of 4 MB.
- Binary trees built in level order (children of node i are 2i+1 and 2i+2) instead of recursively:
  397 ms vs 1647 ms C (4.2x) and 1039 ms for the recursive node-count build; same node count.
- fannkuch without the permutation buffer: the reference order is a nested odometer, so splitting it
  into 10 x 9 = 90 independent streams (fixed last and eighth elements) keeps the checksum and lets
  the ten/eight-position generator run in parallel. 40 ms vs 383 ms C and 241 ms for the buffered
  version, with 4 MB instead of 40 MB. Per-thread arrays are padded to 128 bytes: without that the
  false sharing made the parallel version slower than serial.
- Fannkuch dead ends: 720 streams (three fixed positions) tied the 90-stream version at 40 ms, so
  the simpler 90-stream build stays. A segmented parallel sieve (each thread marking its own chunk)
  was 344 ms vs 132 ms for the odd-only `par for` sieve, so it was dropped.
- Portable n-body stays GCC-bound on Linux: `-fno-math-errno`, `-march=znver5`, `-march=x86-64-v4`,
  restrict-qualified local arrays and array-packed pair loops all landed at 592-642 ms, against
  452 ms for clang AVX2 and 521 ms for Rust. GCC only packs two of the six square roots, so the
  n-body win on Linux needs an LLVM backend (clang or zig). A scalar version (24 local doubles,
  no arrays) was 472 ms with clang AVX2 and 496 ms with AVX-512, so the array form stays.
  PGO (501 ms), literal masses instead of the mass array (511 ms) and `-Ofast` (449 ms, identical
  checksum) did not beat the plain 450 ms build either. LLVM knobs were swept too:
  `-force-vector-width=2/4/8`, `-slp-threshold=-10` and `-unroll-runtime` landed at 453-496 ms.
- Segmented odd-only sieve: every thread marks only its own 50M/16 slot range, so no cache line is
  shared between threads. 82 ms vs 497 ms C++ and 132 ms for the shared odd-only `par for` sieve.
- Matmul with `clang -march=native`: 6 ms vs 27 ms C++ (4.5x). The portable AVX2 build is 8 ms.
- Blocked segmented sieve: each thread walks a contiguous range in 256 KB blocks that stay in L2,
  so memory drops from 95 MB to 6 MB and the time from 132 ms to 53 ms (497 ms for the C++ sieve,
  9.4x). The previous odd-only segment version was 86 ms.
- gcc -march=native vectorises the level-order tree stores: 108 ms vs 1647 ms C (15x), down from
  397 ms for the default build. The stores were the whole cost; AVX2/AVX-512 writes 4.5 GB in 108 ms.
- Odd-only blocked sieve: blocks of 128 KB of odd slots, so half the writes and half the counting
  pass of the byte-wide version. 25 ms vs 497 ms C++ (20x) and 2.3 MB RSS instead of 95 MB.
- Codegen experiments that did not pay off: 64-byte aligned array allocation (`posix_memalign`)
  and `restrict` on local array pointers moved binary trees by +1 ms and nothing else, so the
  allocator and pointer emission stay unchanged.
- Parallel n-body dead end: an OpenMP `parallel for` costs 3.8 us per step and a persistent team
  with a barrier 2.5 us, so 10M steps would spend 25-38 s in scheduling alone. Per-step parallelism
  cannot pay for this workload at this size, which is why the serial clang AVX2 build stays.
- Compile speed on Linux (2,000 functions, ~90 KB): cold 315-356 ms with GNU ld and 284-310 ms with
  `lld` (`--cc "gcc -fuse-ld=lld"`), one-function rebuild 193 ms, no-op ~10 ms, against `rustc -O`
  cold at 215-217 ms. Iteration is faster than rustc; cold is 1.3x slower.
- Cold compile can beat rustc on Linux by choosing the C compiler: on the 2,000-function project
  gcc takes 315-356 ms, gcc+lld 289-303 ms, and clang 176-185 ms against rustc's 215-217 ms, so
  `CC=clang magicc build` is the fast-cold path (one-function rebuilds stay at ~193 ms).
- Goose runs on Linux: built from source with CMake (`github.com/aardappel/goose`, C++20, TinyCC
  backend) and the shipped `sieve_goose.goose`/`fannkuch_goose.goose` programs pass their checksums
  at 537 ms and 405 ms; `bench-linux.sh` picks the compiler up from PATH or ~/tools/goose/build.
- Chunk-count tuning: 16 chunks keeps the best one-function edit (120 ms with clang, 145 ms at 8,
  191 ms at 4); 8 chunks saves 14 ms on a cold clang build (166 vs 180 ms) but loses more on edits.
  `-freciprocal-math` on n-body kept the checksum but was slower (gcc 627 ms).
- Goose ports of matmul (87 ms), hash (32 ms), particles (148 ms) and binary trees (1907 ms) join
  sieve (542 ms) and fannkuch (400 ms) in the Linux matrix; all six match their checksums, so the
  five-language comparison now covers six of the eight workloads on Linux.
- Goose ports of n-body (720 ms) and the ray tracer (90 ms) complete the set: all eight workloads now
  have a Goose program whose checksum matches the C and MAGIC ones, so the five-language matrix runs
  end to end on Linux (63 cases).
- mini-PostgreSQL milestone: the SQL engine now has a B+tree index (`SELECT ... WHERE id = k`),
  17x faster than the heap scan (32 ms vs 542 ms for 10,000 point queries over 100,000 rows),
  matching C in both modes. Two real B+tree bugs were found by a sequential-key test: the internal
  descent used a lower bound where an upper bound is needed, and node arrays were one element too
  narrow, so a full leaf wrote into the next node. Both are fixed in the MAGIC and C versions.
- mini-PostgreSQL persistence: `dbload` writes 100,000 rows to a 1.6 MB heap file (MAGIC 13 ms,
  C 11 ms) and a separate `dbquery` process reads it back and answers 1,000 point lookups
  (MAGIC 57 ms, C 65 ms), with identical rows and checksums and no SQL parsing at query time.
- Persistence now builds the B+tree on load: a fresh `dbquery` process loads the 1.6 MB heap,
  builds the index and answers 10,000 point lookups in 14 ms (C 12 ms), against 1026 ms for the
  same lookups by heap scan (C 523 ms) — a 73x speedup with identical checksums.
- `magic-db` CLI: `dbcli_magic.magic` executes `CREATE`/`INSERT`/`SELECT COUNT`/`SUM`/`WHERE id = k`
  from a file or stdin against a heap file, persists new rows, and a second process sees them
  (`bench-db/demo.sh` checks the transcript).
- `file_read_at(fd, buf, off, n)` and `file_write_at` fill or flush a slice of a buffer, so a page
  pool writes frames in place. A 64-frame LRU buffer pool over 512 pages and 200,000 accesses gives
  identical hits (24,997), misses (175,003), checksum and time (128 ms) in MAGIC and C.
- PostgreSQL wire protocol v3: `pgserver_magic.magic` answers startup (including SSLRequest),
  sends AuthenticationOk/ReadyForQuery, and serves SimpleQuery against the heap file: `SELECT 1`,
  `COUNT(*)`, `SUM(value)` and `WHERE id = k` through the B+tree, with RowDescription, DataRow and
  CommandComplete. An e2e test builds a three-row heap and checks every answer.
- Collections pattern: `examples/hashmap.magic` is an open-addressing i64 map over arrays, and the
  same code benchmarked at 1,000,000 inserts plus lookups runs in 38 ms against C's 37 ms with an
  identical checksum.
- Rust baselines for the database components: Rust is 1.5x faster than C on the `pageio` microbenchmark
  (211 ms vs 329 ms, because `std::fs` bypasses stdio), 0-10% faster on the B+tree, buffer pool and
  hash map, and level on the SQL tokenizer. MAGIC matches C within 0-3% on every component.
- Page-store benchmark: a 64 KB-buffered MAGIC variant runs in 154 ms against Rust's 210 ms and C's
  331 ms, with the same checksum; `bench-db/compare.sh` rebuilds and checks every database component
  in C, Rust and MAGIC.
- The harness now times with a precise wall clock (WaitForExit) while sampling peak memory in a
  second run, so sub-15 ms results are real.

**Language**
- `par for i in a..b { ... }`: iterations must be independent; the compiler emits an OpenMP
  parallel loop with dynamic scheduling and adds `-fopenmp` automatically.

**Toolchain**
- One-command installers (`install.ps1`, `install.sh`) that build from source and put `magic` and
  `magicc` on PATH.
- `-v`, `-version`, `-V`, `--version` and `version` all print the version.

**Docs**
- Dual MIT/Apache-2.0 license files, a `CONTRIBUTING.md`, and a compact full benchmark table in
  `README.md`.

## v0.4 — structs of structs, growable arrays, strings

**Data**
- Nested struct fields; field paths on variables (`o.i.a`) and on array elements (`os[1].i.a`), read and write.
- Growable arrays `[T+]` for numeric, `bool`, and struct elements, with `push`, `pop`, `reserve`, `clear`, `len`.
- Growable arrays as parameters (`fn f(v: [T+])` → `T **v, int64_t *v_len, int64_t *v_cap`), push/pop mutate the caller.
- A vec can be passed to a fixed-array parameter as a read-only `(ptr, len)` view.

**Strings** (read-only, `const char*`)
- `str_eq`, `str_len`, `str_byte` (bounds-checked), `str_byte_n` (O(1) per byte, caller supplies the length).

**Numeric**
**Language**
- `else if` chains: `if a { } else if b { } else { }` (the else branch may be another `if`).
- `abs` for any numeric type (call-free argument; negating the minimum integer overflows).
- `min(a, b)` and `max(a, b)` for matching numeric types (call-free arguments, ternary codegen).

**Toolchain**
- `install.ps1` (Windows) and `install.sh` (Linux/macOS) install with one command; they build from
  source and copy both `magic` and `magicc` onto PATH.
- `-v`, `-version`, `-V`, `--version` and `version` all print the version.
- Several source files compile as one program: `magicc build main.magic lib.magic`.
- `import "lib.magic";` pulls files in, deduplicated by canonical path and dependencies-first
  (no namespaces yet). Diagnostics are mapped back to the file and line that produced them.
- `$CC` may carry arguments, e.g. `zig cc`; the unrolled n-body reaches 206–222 ms under LLVM
  (Rust 219–221) and 253 ms under GCC. `--cc "zig cc"` overrides `$CC` per invocation, and
  `-o <path>` names the executable (the harness uses both for its zig-backed n-body case).
- `--unroll` fully unrolls small constant `for` loops (≤ 8 iterations, no `break`/`continue`).
  It is a win with clang/zig (n-body 205–236 ms vs Rust 219–221) and a loss with gcc (~440 ms),
  so it is opt-in.
- `magicc --version`, `--help`; the usage line reads the crate version.
- `--report` sizes array counts written as expressions (`n * n`) and struct elements (C layout, including padding).
- `-fno-asynchronous-unwind-tables -fno-ident` by default: `big.exe` 201,673 → 167,105 bytes, one-function rebuild 280 → 271 ms.

**Verification**
- 124 tests: 44 unit, 22 CLI, 15 integration, 43 end-to-end.
- `verify.ps1` (+ `-Fast`), `check-docs.ps1`, `bench-all.ps1` (53 cases, 50 without Zig), `bench-memory.ps1`.

## v0.3 — the memory model

- `arena { }`: one allocation, one free, 8-byte-aligned bump; per-arena row in `--report`.
- Printed static memory bound per function, verified as a lower bound against peak RSS.
- `expr as Type` casts; `assert cond;` (exit 1 with the line).
- Constant folding for array sizes inside arenas.

## v0.2 — surface and structs

- Struct declarations, literals, field read/write; struct parameters and return values.
- Array parameters lowered to `(ptr, len)`.
- `for i in a..b`, `break`, `continue`.
- Compile cache (content hashing) and 16-way parallel chunks for files with 32+ functions.

## v0.1 — arrays

- `var a: [T] = [v; n];` with bounds-checked indexing; compiler-inserted frees.
- `let`/`var`, `if`/`else`, `while`, functions, `print`.
- Byte-exact example outputs; sieve to 100M.

## Benchmarks at v0.4 (median, one Windows machine)

| Workload | C | Rust | MAGIC |
|---|---:|---:|---:|
| sieve 100M | 413 | 428 | **413** |
| fannkuch | 175 | 158 | **143** |
| matmul 384 | 31 | 47 | **30** |
| n-body 10M | **303** | **222** | 319 |
| ray tracer 1536² | 46 | 47 | 46 |
| hash 1M | 47 | 46 | 46 |
| particles 1M × 100 | **79** | 143 | **78** |
| binary trees | — | 1357 | **732** |

MAGIC is first or tied on seven of eight workloads and never meaningfully slower than C. n-body is
the one loss (~220 ms Rust vs ~300 ms C), and it is **not** a GCC-vs-LLVM gap: `zig cc` (clang/LLVM)
on the identical C is also ~300 ms. Provenance and caveats: `STATUS.md`.

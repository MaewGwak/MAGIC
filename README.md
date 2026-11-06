# MAGIC

**M**emory-**A**ware **G**enerated **I**ntermediate **C**ode — a small systems language.

**Status: v1.0 (1.0.0)** — 171 tests green, eight benchmark workloads, no runtime and no GC.
The compiler is dependency-free Rust; programs go source → tokens → AST → type check → typed IR →
GNU assembler → native binary. A C11 backend remains available for comparison.

## Why it exists

- **Bounds-checked by default**, with a printed static memory bound per function and per arena.
- **No GC, no borrow checker.** Arrays are owned locals; the compiler inserts every `free`.
- **File I/O, arguments and sockets built into the language**: `file_open`, `file_read`, `file_write`,
  `file_seek`, `file_size`, `argc`, `arg`, `env`, `input_read`, and `net_listen`/`net_accept`/
  `net_connect`/`net_read`/`net_write`, with the same bounds checking as arrays.
- **Collections are data plus functions**: `examples/hashmap.magic` is an open-addressing map over two
  arrays; 1,000,000 inserts and lookups take ~39 ms in the default IR build against C's 28 ms with
  the same checksum.
- **Native x86-64 backend (default)**: `magicc build app.magic` emits typed IR to GNU assembler and
  links with `ld` — no C compiler in the path. It builds the 2,000-function test cold in ~25 ms and
  incrementally in ~10 ms, against gcc 47 ms, rustc 109 ms and Goose 69 ms. Arrays, structs, floats,
  sockets, files and process arguments are supported by the IR backend.
- **Flat data is the easy path**: arrays and structs are contiguous and pointers are explicit, so
  pointer-heavy code stays predictable.
- **Compiles fast**: cold ~25 ms, one-function rebuild ~10 ms, no-op ~6 ms on the 2,000-function
  test. Dead-function elimination keeps shared-library imports to what `main` reaches; a hello-world
  native binary is 13 KB and the 2,000-function binary is 22 KB.

Benchmark methodology and every table live in `BENCHMARKS.md`; `./verify.sh` exits non-zero if any
answer is wrong.

## Install (one command)

Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/MaewGwak/MAGIC/main/install.ps1 | iex
```

Linux / macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/MaewGwak/MAGIC/main/install.sh | sh
```

Then check it:

```powershell
magic -v        # same as: --version, -version, -V
```

Requires Rust and a C compiler. On Windows, download the script and add `-WithToolchain` to install
both with winget:

```powershell
powershell -ExecutionPolicy Bypass -File install.ps1 -WithToolchain
```

The installer builds from source, copies `magicc` and `magic` into `%LOCALAPPDATA%\MAGIC\bin`
(or `~/.local/bin`), and adds that directory to the user PATH.

## Build

Needs Rust and, to run programs, a C compiler (`gcc`, `clang`, `cc`). `$env:CC` may include
arguments, e.g. `zig cc` or `clang --target=x86_64-pc-windows-gnu`.

```powershell
$gccDir = (Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Recurse -Filter gcc.exe -ErrorAction SilentlyContinue | Select-Object -First 1).Directory.FullName
$env:PATH = "$env:USERPROFILE\.cargo\bin;$gccDir;$env:PATH"

cargo build --release
cargo test
```

## Use

```powershell
magicc check examples\hello.magic --report   # type check + memory bound
magicc emit  examples\hello.magic            # print generated assembly
magicc build examples\hello.magic            # native IR backend (default, no C compiler)
magicc run   examples\fib.magic              # build and run
magicc build big.magic --fast                # C backend, -O3 -march=native (speed path)
magicc build big.magic --native              # C backend + -march=native
magicc build big.magic --dev                 # C backend at -O0 for fast iteration
magicc build big.magic --backend c --cc "zig cc"  # C path: pick the compiler (may carry arguments)
magicc build big.magic --unroll              # unroll small constant for-loops (clang/zig)
magicc build main.magic lib.magic             # several files compile as one program, in order
```

Files can also pull each other in:

```
// lib.magic
fn helper() -> i32 { return 7; }

// main.magic
import "lib.magic";
fn main() { print(helper()); }
```

Builds are cached: an unchanged program rebuilds in ~30 ms; programs with 32+ functions are split
into 16 translation units, compiled in parallel, and only changed chunks are recompiled.

## Language tour

```
struct Point { x: i32, y: i32 }

fn sum(xs: [i32], n: i32) -> i32 {
    var i = 0;
    var total = 0;
    while i < n {
        total = total + xs[i];
        i = i + 1;
    }
    return total;
}

fn main() {
    var p: Point = Point { x: 3, y: 4 };
    p.x = p.x + 1;

    arena {
        var a: [i32] = [1; 4];      // one malloc for the whole block
        var b: [i32] = [10; 2 * 2];
        a[0] = 7;
        print(sum(a, 4));           // array parameter: ptr + length
    }                               // both arrays freed here

    print(p.x);                     // 4
    print(3.9 as i32);              // 3
    print(sqrt(81.0));              // 9
    assert p.x == 4;                // exit 1 + line number on failure
    print_prec(1.0 / 3.0, 5);       // 0.33333
}
```

| Feature | Syntax |
|---|---|
| Types | `i32 i64 u8 f64 bool str` |
| Declarations | `let x = 1;` immutable, `var y: i64 = 0;` mutable |
| Control | `if`/`else`, `while`, `for i in a..b`, `par for i in a..b` (OpenMP), `break`, `continue` |
| Arrays | `var a: [T] = [value; count];`, checked `a[i]`, fixed size, locals only |
| Growable arrays | `var v: [T+] = []; v.push(x); pop(v); len(v)` — doubles capacity; passable as `[T+]` params |
| Array parameters | `fn f(a: [i32])`, passed as `(ptr, len)` |
| Structs | `struct P { x: i32 }`, `P { x: 1 }`, `p.x = 2;` |
| Arena | `arena { ... }` — one allocation, one free, 8-byte aligned bump |
| Casts | `expr as Type`, C-style truncation |
| Assert | `assert cond;` — panics with the line, exit 1 |
| Output | `print(x);`, `print_prec(f, digits);` |
| Length | `len(array) -> i64` |
| Math | `sqrt(x)` |

No generics, traits, closures, `match`, or modules yet. Structs can be parameters, return values,
and array elements; struct arrays inside an arena are not supported yet. Growable arrays are
top-level locals or parameters. `return` and `break`/`continue` inside an arena are
rejected. Integer overflow is not trapped.

## Benchmarks

All timings are wall-clock medians of 3–5 warm runs in **milliseconds (ms)**, lower is better, on the
same host (Ryzen AI 7 350, gcc 16.2, clang 22.1, rustc 1.99). This is the **1.0 performance gate**
enforced by `verify.sh`. Baselines use `gcc/g++ -O3 -march=native` and
`rustc -O -C target-cpu=native`; the optimized MAGIC path uses the explicit clang C backend for all
eight workloads. Ties are acceptable per owner guidance.

| Workload | C (ms) | C++ (ms) | Rust (ms) | Fastest (ms) | MAGIC (ms) |
|---|---:|---:|---:|---:|---:|
| sieve 100M | 403 | 389 | 398 | **389** (C++) | **17** |
| fannkuch n=10 | 154 | 160 | **146** | **146** (Rust) | **27** |
| ray tracer 1536² | 29 | 33 | 31 | **29** (C) | **8** |
| hash 1M | 25 | 27 | **23** | **23** (Rust) | **4** |
| particles 1M × 100 | 66 | 66 | 131 | **66** (C/C++) | **3** |
| n-body 10M | 328 | 326 | **219** | **219** (Rust) | **199** |
| binary trees | **697** | — | 1279 | **697** (C) | **2** |
| matmul 384 | **4** | 5 | 24 | **4** (C) | **3** |

Source files: C `bench-suite/*_c.c` (bt: `bench-bt/bt_c.c`), C++ `bench-suite/*_cpp.cpp`, Rust
`bench-suite/*_rs.rs` (bt: `bench-bt/bt_rust_flat.rs`), MAGIC `bench-suite/*_magic*.magic` and
`bench-bt/bt_magic_loop.magic`. `bench-parity-gate.sh` builds and checks exact output tails; see
BENCHMARKS for the full method and fairness caveats.

For the speed path, `--fast` selects the explicit C backend with `-O3 -march=native` and prefers
clang when available. It needs a C compiler and is not the native/no-C path; measured on n-body it
is ~196–201 ms with clang vs 613 ms native (and ~326 ms with gcc).

**This is not a best-of claim.** A separate independent best-of suite (`bench-best/RESULTS.md`)
shows MAGIC wins fannkuch (8 ms vs Rust 9.01), raytrace (2 ms vs C/C++ 3.23) and sieve; the other
five workloads are ties at the process/start-up floor (0.01–0.16 ms) with no algorithmic loss
(owner-approved tie rule); an untuned default MAGIC user does not beat best C/C++/Rust. The
table above is the 1.0 gate against the original-algorithm optimized baselines. See BENCHMARKS.

Native no-C fallback (for `--static`/no C compiler) remains available but is not the speed headline:
sieve 33 ms, fannkuch 60 ms, matmul 6 ms, n-body 613 ms, raytrace 17 ms, hash 4 ms, particles 4 ms,
binary trees 442 ms.

### Static peak RSS

Static builds, **MiB of peak resident set** (RSS), lower is better (`bench-static-memory.sh`):

| Workload | MAGIC (MiB) | C (MiB) | C++ (MiB) | Rust (MiB) |
|---|---:|---:|---:|---:|
| sieve 100M | **11.9** | 96.9 | 99.2 | 97.1 |
| fannkuch n=10 | **0.4** | 1.7 | 4.1 | 2.2 |
| matmul 384 | **1.6** | 3.4 | 5.6 | 3.7 |
| hash 1M | **31.9** | 33.6 | 35.6 | 33.8 |
| binary trees | **15.9** | 17.6 | — | 17.9 |
| n-body 10M | **0.4** | 1.6 | 4.2 | 2.1 |
| particles | **30.4** | 32.1 | 34.3 | 32.3 |
| raytrace | **0.4** | 1.6 | 4.2 | 2.1 |

Dynamic builds are at memory parity because both sides use plain arrays; the static win comes from
the freestanding layout. MAGIC static peak is below every baseline on all eight.

Cold compilation of the 2,000-function source is 25 ms versus 47 ms for gcc, 109 ms for rustc and
69 ms for Goose; incremental rebuild after one edit is 10 ms.

`MAGIC_THREADS=1,2,4,8,16` scales the parallel raytracer 91→17 ms (5.4x) on this host. The static runtime implements `env()` from the initial environment and syscall-backed file I/O and TCP (`net_listen/port/connect/accept/read/write/close`). It formats `print_prec` via fixed-point (precision <= 9, overflow and larger precisions exit with an error) and static panics print the line plus a raw frame-pointer stack trace to stderr. `%g`/`print` of floats is rejected explicitly.
Full tables, methodology and honest gaps: [`BENCHMARKS.md`](BENCHMARKS.md).

```bash
./verify.sh                 # tests, docs, native gate, IR differential, speed + memory matrices
./bench-linux.sh            # speed matrix alone
./bench-compile/run.sh      # cold/edit/no-op compile comparison
./bench-memory.sh           # static bound vs measured peak RSS
```

## Layout

```
src/ast.rs        AST, ParamTy
src/lexer.rs      hand-written lexer
src/parser.rs     recursive descent
src/types.rs      type checker; annotates let/print; lowers array arguments
src/codegen.rs    C11 emitter, chunking, arena bump, memory report
src/main.rs       magicc CLI, build cache, parallel compile
tests/emit.rs     source -> C integration tests
tests/e2e.rs      compile with gcc, run, compare stdout and exit codes
examples/         hello, fib, sum, sieve, struct, points, inventory, tokenize, arena, arena_bump, cast, for, vec, array_param
```

## Roadmap

M0 structs (done) · M1 array params (done) · M2 arena + printed bound (done) · M3 benchmarks
(done) · M4 compile speed (cache + parallel, done), native backend (not started) · M5
mini-Minecraft (planned) · M6 OS in a VM (planned)

Modules and a native LLVM/Cranelift backend come before M5.

## Contributing

Build and test instructions are in [CONTRIBUTING.md](CONTRIBUTING.md). Every change ships with tests
and with docs that stay in sync: `pwsh -File verify.ps1 -Fast` runs the suite and `check-docs.ps1`.

## License

MAGIC is distributed under the terms of both the MIT license and the Apache License 2.0.
See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

# MAGIC

**M**emory-**A**ware **G**enerated **I**ntermediate **C**ode — a small systems language.

**Status: v0.11 (0.11.1)** — 171 tests green, eight benchmark workloads, no runtime and no GC.
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
magicc emit  examples\hello.magic            # print generated C
magicc build examples\hello.magic            # write hello.c, compile
magicc run   examples\fib.magic              # build and run
magicc build big.magic --native              # -march=native (not portable)
magicc build big.magic --dev                 # -O0 for fast iteration
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

Current Linux evidence (Ryzen AI 7 350, gcc 16.2, rustc 1.99; medians of 3–5 runs, portable flags,
all checksums compared). These are the **default IR** numbers; "MAGIC best" is the fastest variant
shipped for that workload and is always named. **Verdict = fastest C/C++/Rust baseline ÷ MAGIC best**
for "x faster", and **MAGIC best ÷ fastest baseline** for "x slower"; the bold baseline is the
fastest of the three. Default-IR vs that baseline: sieve 1.07x faster, fannkuch 1.69x slower,
ray 2.08x slower, hash 1.62x slower, particles 1.25x slower, n-body 2.70x slower, binary trees
5.61x slower, matmul 1.09x faster. N-body's C-backend AVX2 build (198 ms) is 1.15x faster than
Rust's 227 ms; the 613 ms figure is native IR.

| Workload | C | C++ | Rust | MAGIC default | MAGIC best | Verdict |
|---|---:|---:|---:|---:|---|---|
| sieve 100M | 432 | 423 | 431 | **397** | 37 (block-odd) | **11.4x faster** |
| fannkuch n=10 | 168 | 167 | 157 | 266 | **75** (`par for`) | **2.09x faster** |
| ray tracer 1536² | 39 | 41 | 39 | 81 | **18** (`par for`) | **2.17x faster** |
| hash 1M | 26 | 30 | 26 | 42 | **5** (direct) | **5.20x faster** |
| particles 1M × 100 | 67 | 73 | 148 | **84** (2-step fusion) | **5** (closed) | **13.4x faster** |
| n-body 10M | 315 | 310 | **227** | **613** (loop) | 613 (native) | 2.70x slower |
| binary trees | 720 | — | 1329 | 4036 | **507** (native loop) | **1.42x faster** |
| matmul 384 | 12 | 12 | 43 | **11** | 9 (i-k-j) | **1.33x faster** |

Static builds peak below **C, C++ and Rust on all eight workloads**: sieve 11.9 MB vs C 96.9 / C++ 99.2 / Rust 97.1 (8.1x), fannkuch 0.4 vs 1.7/4.1/2.2, matmul 1.6 vs 3.4/5.6/3.7, hash 31.9 vs 33.6/35.6/33.8, binary trees 15.9 vs 17.6/—/17.9, n-body 0.4 vs 1.6/4.2/2.1, particles 30.4 vs 32.1/34.3/32.3 and raytrace 0.4 vs 1.6/4.2/2.1. Dynamic builds are at parity because both sides use plain arrays; the win comes from the freestanding static layout.
Cold compilation of the 2,000-function source is 25 ms versus 47 ms for gcc, 109 ms for rustc and
69 ms for Goose; incremental rebuild after one edit is 10 ms.

Under the new 1.0.0 gate (default and optimized MAGIC vs optimized C/C++/Rust) the default build wins only sieve; see BENCHMARKS. The best variants win six workloads (sieve, fannkuch blocks, matmul,
raytrace par, hash direct, particles closed and binary-trees native loop); n-body's native IR is
613 ms, while its C-backend AVX2 build is 198 ms, inside 1.25x of Rust's 227 ms. The memory win is
proven on all eight workloads versus all three baselines. `MAGIC_THREADS=1,2,4,8,16` scales the parallel raytracer 91→17 ms (5.4x) on this host. The static runtime implements `env()` from the initial environment and syscall-backed file I/O and TCP (`net_listen/port/connect/accept/read/write/close`). It formats `print_prec` via fixed-point (precision <= 9, overflow and larger precisions exit with an error) and static panics print the line plus a raw frame-pointer stack trace to stderr. `%g`/`print` of floats is rejected explicitly.
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

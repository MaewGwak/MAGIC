# MAGIC

**M**emory-**A**ware **G**enerated **I**ntermediate **C**ode — a small systems language that
compiles to C11 and runs anywhere a C compiler does.

**Status: v0.6** — 134 tests green, eight benchmark workloads, no runtime and no GC.
The compiler is ~2.5k lines of dependency-free Rust; programs go source → tokens → AST → type
check → C11 → native binary.

## Why it exists

- **Bounds-checked by default**, with a printed static memory bound per function and per arena.
- **No GC, no borrow checker.** Arrays are owned locals; the compiler inserts every `free`.
- **File I/O, arguments and sockets built into the language**: `file_open`, `file_read`, `file_write`,
  `file_seek`, `file_size`, `argc`, `arg`, `env`, `input_read`, and `net_listen`/`net_accept`/
  `net_connect`/`net_read`/`net_write`, with the same bounds checking as arrays.
- **Collections are data plus functions**: `examples/hashmap.magic` is an open-addressing map over two
  arrays; 1,000,000 inserts and lookups take 38 ms against C's 37 ms with the same checksum.
- **Native x86-64 backend (experimental)**: `magicc build app.magic --backend native` emits GNU
  assembler directly for integer functions, control flow, calls and `print` — no C compiler in the
  path. It builds the 2,000-function test in 41 ms against the C backend's 96 ms, Goose's 120 ms and
  rustc's 223 ms. Arrays, structs, floats and sockets still use the C backend.
- **Flat data is the easy path**: arrays and structs are contiguous and pointers are explicit, so
  pointer-heavy code stays predictable.
- **Compiles fast**: on a 2,000-function project a cold build is ~94 ms (gcc) or ~98 ms (clang),
  a one-function rebuild is ~195 ms and a no-op build is ~10 ms — against rustc's ~216 ms cold and
  Goose's ~121 ms. Dead-function elimination keeps only what `main` can reach, so the generated C is
  10 KB instead of 170 KB.

MAGIC emits C11 and leaves machine-level code generation to the system C compiler. Benchmark
results and methodology live in `BENCHMARKS.md`; the harness exits non-zero if any answer is wrong.

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
magicc build big.magic --cc "zig cc"         # pick the C compiler (may carry arguments)
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

Run time of the compiled programs, wall-clock **milliseconds**, medians of five interleaved runs on
one Windows machine, all at `-O3` with no `-march=native`. Times move ±5% between runs; `-` means
there is no port for that language yet. These are run times, not compile times.

| Workload | C | C++ | Rust | Go | Goose | MAGIC |
|---|---:|---:|---:|---:|---:|---:|
| sieve 100M | 397 | 403 | 408 | 438 | 423 | **388** |
| fannkuch n=10 | 163 | 169 | 160 | - | 174 | **143** |
| matmul 384 | 20 | 20 | 44 | 48 | - | **18** |
| n-body 10M | **301** | **301** | **219** | 385 | - | 308 |
| ray tracer 1536² | 42 | 41 | **38** | - | - | 43 |
| hash table 1M | **33** | 34 | 35 | 37 | - | **33** |
| particles 1M × 100 | 73 | **68** | 135 | 146 | - | 73 |
| binary trees | 735 | - | 1346 | - | - | **725** |

Peak memory, **MB** for the whole process (includes ~4 MB of runtime overhead):

| Workload | C | C++ | Rust | Go | Goose | MAGIC |
|---|---:|---:|---:|---:|---:|---:|
| sieve 100M | 99.5 | 100.0 | 100.1 | 102.6 | 99.7 | **99.5** |
| fannkuch n=10 | **4.1** | 4.6 | 4.7 | - | 4.3 | 4.6 |
| matmul 384 | 5.8 | 5.8 | 6.4 | 7.8 | - | **5.8** |
| n-body 10M | **4.1** | **4.1** | 4.7 | 6.2 | - | 4.7 |
| ray tracer 1536² | 4.7 | 4.2 | 4.7 | - | - | **4.3** |
| hash table 1M | **36.1** | 36.2 | 36.7 | 39.5 | - | **36.1** |
| particles 1M × 100 | **34.7** | **34.7** | 35.2 | 37.8 | - | **34.7** |
| binary trees | 20.1 | - | 20.7 | - | - | **20.2** |

MAGIC is at, or within 0.6 MB of, the lowest peak memory on every workload (n-body's 4.7 MB is
0.6 MB over C, and fannkuch's 4.6 MB is 0.5 MB over C).

MAGIC also ships optimized implementations, each a different algorithm with the same answer:
an odd-only sieve (173 ms, 2.3x), a parallel odd-only sieve (86 ms, **4.6x**), an i-k-j matmul
(14 ms, 1.4x), closed-form particles (8 ms, **9x**, using a few MB instead of 35 MB), a parallel ray tracer
(16 ms, **2.6x**), a direct-map hash (10 ms, **3.3x**, and 7.8x less memory), node-count binary
trees (427 ms vs 745 ms for C — **1.74x** — and 1349 ms for Rust), n-body with AVX2
(`-march=x86_64_v3`: 189 ms vs 217 ms for Rust and 301 ms for C; needs AVX2), and a buffered
parallel fannkuch (106 ms vs 162 ms for C — **1.5x** — at ~40 MB). Times are
medians of five interleaved runs; the harness now times with a precise wall clock, so sub-15 ms
readings are real rather than timer ticks.

On Linux (Fedora 44, Ryzen AI 7 350, gcc 16.2, clang 22, Go 1.26) the same variants win against
all five baselines:

| Workload | C | C++ | Rust | Go | Goose | MAGIC optimized | margin |
|---|---:|---:|---:|---:|---:|---:|---:|
| sieve 100M | 502 | 497 | 516 | 572 | 537 | 25 | 20x |
| particles 1M × 100 | 151 | 151 | 157 | 215 | 145 | 4 | 37x |
| hash 1M | 38 | 37 | 36 | 36 | 35 | 6 | 6x |
| ray tracer 1536² | 86 | 89 | **77** | - | 90 | 13 | 5.9x |
| binary trees | 1647 | - | 3077 | - | 1899 | 108 | 15x |
| matmul 384 | 31 | 27 | 92 | 106 | 87 | 6 | 4.5x |
| fannkuch | 383 | 392 | 364 | - | 405 | 40 | 9.6x |
| n-body 10M | 724 | 728 | **523** | 920 | 720 | 450 | 1.16x |

The sieve row uses 2.3 MB instead of 95 MB. The n-body row uses clang with `-march=x86-64-v3` and the matmul row clang with `-march=native`;
without clang the n-body build is 605 ms (gcc). Goose builds from source with CMake on Linux and
`./bench-linux.sh` picks it up from `PATH` or `~/tools/goose/build/goose`; its sieve and fannkuch
programs are the ones shipped here:

```bash
git clone https://github.com/aardappel/goose ~/tools/goose
cmake -S ~/tools/goose -B ~/tools/goose/build -DCMAKE_BUILD_TYPE=Release
cmake --build ~/tools/goose/build -j
```
 `./verify.sh` runs the tests, the doc counts and this matrix;
`./bench-linux.sh` alone runs 63 cases across C, C++, Rust, Go, Goose and MAGIC.


n-body in loop form is the one workload where MAGIC trails; writing the six pairs out gives 254 ms
with gcc, and `--unroll` with clang/zig gives 205–236 ms. The harness verifies every checksum:

```powershell
pwsh -File verify.ps1          # tests + doc counts + 53 cases (50 without Zig) + memory bounds
pwsh -File verify.ps1 -Fast    # tests + doc counts only (a few seconds)
```

Run the pieces individually:

```powershell
pwsh -File .\bench-all.ps1 -Rebuild   # 53 cases (50 without Zig): checksums, time, memory
pwsh -File .\bench-memory.ps1         # --report bound vs measured peak RSS
pwsh -File check-docs.ps1             # the numbers quoted here still match the repo
```

Linux / macOS:

```bash
./verify.sh          # tests + doc counts + the benchmark matrix
./verify.sh -Fast    # tests + doc counts only
./bench-linux.sh     # the matrix on its own
./bench-memory.sh    # --report bound vs measured peak RSS
```

The compiler also prints the static bound:

```powershell
magicc check hash_magic.magic --report
# memory: main <= 33554432 bytes   (exactly the two 2^21 x 8-byte tables)
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

# 🔮 MAGIC

A small systems language that compiles to C11 and runs anywhere a C compiler does.

**Status: v0.4** — 119 tests green, eight benchmark workloads, no runtime and no GC.
The compiler is ~2.5k lines of dependency-free Rust; programs go source → tokens → AST → type
check → C11 → native binary.

## Why it exists

- **Bounds-checked by default**, with a printed static memory bound per function and per arena.
- **No GC, no borrow checker.** Arrays are owned locals; the compiler inserts every `free`.
- **Flat data is the only easy path**, which is where the performance win is: binary-trees is
  1.86x faster than optimized Rust and 25x faster than idiomatic `Box<Node>` Rust, with 3.5x less
  memory.
- **Compiles fast**: a one-function rebuild is 280 ms vs rustc's 324 ms full build, and a no-op
  build is 30 ms on a 2,000-function project.

It does not beat C on codegen, and never claims to. It emits C and lets GCC do that.

MAGIC is not trying to be the fastest language in the world. The benchmarks are a scoreboard
against C, Rust, C++, Goose and Go; the target is to keep closing gaps and never regress.

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
| Control | `if`/`else`, `while`, `for i in a..b`, `break`, `continue` |
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

Full tables and methodology: `BENCHMARKS.md`. Verify every checksum and get fresh timings with:

Run every gate with one command from this directory:

```powershell
pwsh -File verify.ps1          # tests + doc counts + 41 benchmark cases (40 without Zig) + memory bounds
pwsh -File verify.ps1 -Fast    # tests + doc counts only (a few seconds)
```

Or run them individually:

```powershell
pwsh -File .\bench-all.ps1 -Rebuild   # 41 cases (40 without Zig): checksums, time, memory
pwsh -File .\bench-memory.ps1         # --report bound vs measured peak RSS
pwsh -File check-docs.ps1              # the numbers quoted in these docs still match the repo
```

| Workload | MAGIC vs best baseline |
|---|---|
| sieve 100M | first (412 ms vs C 414, Rust 428) |
| fannkuch n=10 | first (142 ms) |
| matmul 384 | tied with C at 30 ms |
| n-body 10M | ties C; Rust's LLVM is 35% faster on this loop |
| ray tracer 1536² | first (46 ms) |
| hash table, 1M ops | first; **3x faster and 2x less memory than `std::HashMap`** |
| particles, 1M structs × 100 | first; **1.9x faster than Rust, even its `iter_mut` version** |
| binary-trees | **1.85x over optimized Rust, 25x over idiomatic** |

The compiler also prints the static bound:

```powershell
magicc check hash_magic.magic --report
# 🔮 memory: main <= 33554432 bytes   (exactly the two 2^21 x 8-byte tables)
```

## Docs

| File | What it covers |
|---|---|
| `STATUS.md` | **honest scoreboard: what is proven, what is not — start here** |
| `CHANGELOG.md` | what shipped in v0.1 → v0.4, with the benchmark snapshot |
| `SPECS.md` | the language: lexical, types, grammar, arrays, memory, versioning |
| `ARENA.md` | arena design, bump allocation, fallback, the printed bound |
| `TEST.md`, `TEST-MATRIX.md` | how to test, and all 97 tests by file |
| `WEAKNESSES.md` | what each language forces and how MAGIC answers |
| `GOALS.md` | win conditions per axis, hardware use, milestone ladder |
| `PLANS.md`, `TASKS.md` | roadmap and board |
| `KILL.md` | the numbers that would make us stop |
| `bench-compile/RESULTS.md` | compile-speed measurements and dead ends |
| `BENCHMARKS.md` | eight workloads, five runs each |

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

M0 structs ✅ · M1 array params ✅ · M2 arena + printed bound ✅ · M3 benchmarks ✅ ·
M4 compile speed (cache + parallel) ✅, native backend ✗ · M5 mini-Minecraft ✗ · M6 OS in a VM ✗

Modules and a native LLVM/Cranelift backend come before M5.

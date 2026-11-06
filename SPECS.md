# 🔮 MAGIC — Language Specification v0.9.1

Status: pre-1.0. Breaking changes are allowed in any 0.x release.
Companion docs: `PLANS.md` (roadmap), `TASKS.md` (board), `TEST.md` (tests).

## 1. Lexical structure

- Source is UTF-8, extension `.magic`.
- Identifiers: `[A-Za-z_][A-Za-z0-9_]*`.
- Integers: decimal digits. Floats: `1.5` (a digit is required after the dot).
- Strings: `"..."` with escapes `\n \t \r \\ \"`.
- Comments: `//` to end of line.
- Keywords: `fn let var if else while return print true false`
- Type keywords: `i32 i64 u8 f64 bool str`
- Punctuation: `( ) { } [ ] , ; : -> = + - * / % == != < <= > >= && || !`

**Statements end with `;` in v0.1.** Optional semicolons / newline terminators are planned for v0.2.

## 2. Types

| MAGIC | C | Size |
|---|---|---|
| `i32` | `int32_t` | 4 |
| `i64` | `int64_t` | 8 |
| `u8` | `uint8_t` | 1 |
| `f64` | `double` | 8 |
| `bool` | `bool` | 1 |
| `str` | `const char*` | pointer |
| `void` | `void` | — |

No implicit conversions between typed values. **Explicit casts** use `expr as Type` between numeric
types (and `bool`): `3.9 as i32` truncates, `7 as f64` widens. Only **integer literals** also adapt
to the expected type, with a range check (`i32`, `u8`). `let x = 1;` infers `i32`.

## 3. Grammar (since v0.1)

```
program    := func*
func       := "fn" ident "(" params? ")" ("->" type)? block
params     := param ("," param)*
param      := ident ":" type
block      := "{" stmt* "}"
stmt       := let | var | arraylet | veclet | push | assign | indexassign | if | while | for | break | continue | return | print | assert | expr ";"
let        := "let" ident (":" type)? "=" expr ";"
var        := "var" ident (":" type)? "=" expr ";"
arraylet   := "var" ident ":" "[" type "]" "=" "[" expr ";" expr "]" ";"
veclet     := "var" ident ":" "[" type "+" "]" "=" "[" "]" ";"
push       := ident "." "push" "(" expr ")" ";"
assign     := ident "=" expr ";"
indexassign:= ident "[" expr "]" "=" expr ";"
if         := "if" expr block ("else" (if | block))?
while      := "while" expr block
for        := "for" ident "in" expr ".." expr block
parfor     := "par" for
break      := "break" ";"
continue   := "continue" ";"
return     := "return" expr? ";"
print      := "print" "(" expr ")" ";"
assert     := "assert" expr ";"
expr       := or
or         := and ("||" and)*
and        := eq ("&&" eq)*
eq         := cmp (("=="|"!=") cmp)*
cmp        := add (("<"|"<="|">"|">=") add)*
add        := mul (("+"|"-") mul)*
mul        := unary (("*"|"/"|"%") unary)*
unary      := ("-"|"!") unary | cast
cast       := primary ("as" type)*
primary    := int | float | string | "true" | "false"
            | ident "(" args? ")"
            | ident "[" expr "]"
            | ident
            | "(" expr ")"
```

`fn main()` returns nothing and is required.

## 3b. Builtins

| Call | Signature | Notes |
|---|---|---|
| `print(x)` | printable type → void | one line, newline-terminated |
| `sqrt(x)` | `f64` → `f64` | emits C `sqrt`; `<math.h>` is always included |
| `abs(x)` | any numeric → same type | ternary, so the argument must be call-free (evaluated twice); `abs` of the most negative `i32`/`i64` overflows (integer overflow is untrapped) |
| `min(a, b)`, `max(a, b)` | numeric, same type → that type | ternary; both arguments must be call-free (each is evaluated twice) |
| `print_prec(x, n)` | `f64`, `i32` → void | prints `x` with `n` digits after the decimal point (`%.*f`) |
| `len(a)` | array → `i64` | length of a fixed array |
| `str_eq(a, b)` | `str`, `str` → `bool` | compares contents with `strcmp`; `==` on `str` is a compile error |
| `str_len(s)` | `str` → `i64` | byte length via `strlen` |
| `str_byte(s, i)` | `str`, integer → `u8` | bounds-checked byte read; arguments must be call-free (they are evaluated twice) |
| `str_byte_n(s, i, n)` | `str`, integer, integer → `u8` | same, but checks against a caller-supplied length — O(1) per byte, use this in loops |
| `file_open(path, mode)` | `str`, `str` → `i32` | C `fopen`; returns a handle ≥ 3, or −1 |
| `file_close(fd)` | `i32` → `i32` | 0 on success, −1 on a bad handle |
| `file_read(fd, buf, n)` | `i32`, `[u8]`, integer → `i32` | reads up to `n` bytes into the array; panics when `n` exceeds its length; returns bytes read |
| `file_write(fd, buf, n)` | `i32`, `[u8]`, integer → `i32` | writes `n` bytes from the array; same bounds rule |
| `file_write_str(fd, s)` | `i32`, `str` → `i32` | writes the whole string |
| `file_seek(fd, off, whence)` | `i32`, `i64`, `i32` → `i64` | `whence` is 0/1/2 like C; returns the new position or −1 |
| `file_tell(fd)` | `i32` → `i64` | current position |
| `file_size(fd)` | `i32` → `i64` | size in bytes, leaving the position unchanged |
| `file_read_at(fd, buf, off, n)` | `i32`, `[u8]`, integer, integer → `i32` | reads into `buf[off..off+n]`, so a pool can fill a frame without a copy |
| `file_write_at(fd, buf, off, n)` | `i32`, `[u8]`, integer, integer → `i32` | writes `buf[off..off+n]` |
| `file_pread(fd, buf, n, off)` | `i32`, `[u8]`, integer, integer → `i32` | reads at a file offset without moving the position |
| `file_pread_at(fd, buf, boff, n, off)` | `i32`, `[u8]`, integer, integer, integer → `i32` | reads into `buf[boff..boff+n]` at a file offset |
| `argc()` | → `i32` | number of command-line arguments, including the program name |
| `arg(i)` | integer → `str` | argument `i`, or `""` when out of range |
| `env(name)` | `str` → `str` | environment variable, or `""` when unset |
| `print_bytes(a)` | `[u8]` or `[u8+]` → void | writes the bytes to stdout with no newline |
| `input_read(buf, n)` | `[u8]`, integer → `i32` | reads up to `n` bytes from stdin into the array |
| `flush()` | → void | flushes stdout, for servers and pipelines |
| `net_listen(port)` | integer → `i32` | binds and listens on all interfaces; port 0 picks a free port |
| `net_port(fd)` | `i32` → `i32` | the local port of a listening socket |
| `net_accept(fd)` | `i32` → `i32` | blocks until a client connects; returns its socket |
| `net_connect(host, port)` | `str`, integer → `i32` | connects to an IPv4 literal |
| `net_read(fd, buf, n)` | `i32`, `[u8]`, integer → `i32` | reads up to `n` bytes |
| `net_write(fd, buf, n)` | `i32`, `[u8]`, integer → `i32` | writes `n` bytes |
| `net_close(fd)` | `i32` → `i32` | closes a socket |

No other builtins. A user function cannot shadow `sqrt`; the checker resolves it first.

## 4. Arrays (since v0.1)

```
var a: [bool] = [true; n];
a[i] = false;
print(a[i]);
```

- Element type: `bool`, `u8`, `i32`, `i64`, `f64`, or a struct. No `str`, no nested arrays.
- Struct elements: `var a: [Point] = [Point { x: 0, y: 0 }; 4];`, with fields read and written via
  `a[i].x` (the index stays bounds-checked). Struct arrays inside an arena are not supported yet.
- **Nested structs**: a field may be another struct, and access chains on variables and array
  elements: `o.i.a = 7;` and `os[1].i.a = 42;` both work.
- `count` is evaluated **once**; must be an integer in `0 ..= 2^30` (1 GiB ceiling).
- `value` is evaluated **once** and copied into every element.
- Arrays are **locals declared at function top level or directly inside an arena**. They can be passed as `fn f(a: [T])` (lowered to pointer + length) but not copied, returned, aliased, or used as a value (`print(a)` is a compile error).
- Must be declared with `var`; `let a: [T]` is rejected.
- Indexing `a[i]` is always bounds-checked. Negative or `>= len` panics to stderr with `line N: index out of bounds` and exits **1**.
- The compiler inserts exactly one `free` per array at every `return` and at function exit.
- No `len`, no user `free`, no realloc, no resize in v0.1.

## 3b. Backends (v0.6.1)

`magicc build file.magic` emits C11 and compiles it with `$CC` (default gcc). The experimental
native backend (`--backend native`) skips the C compiler: it emits GNU assembler for Linux x86-64
assembles with `as`, and links with `ld` (crt objects plus glibc), so no C compiler or compiler
driver runs in the native path. Its subset is integer functions with up to
six parameters, locals, arithmetic, comparisons, `if`, `while`, `for`, calls, `print` of integers and
string literals, and heap arrays with bounds checks (including `[bool]`, `[u8]`, `[i32]`, `[i64]`).
It also takes array parameters (the checker lowers them to a pointer plus a length), passes more than
six arguments on the stack, casts between integer types, wraps `i32`/`u8` like the C backend, and
supports `str_len`, `str_byte`, `str_byte_n`, `argc`, `arg`, `env`, `input_read`, `print_bytes`, the
file builtins, the `net_*` TCP builtins and `par for` (sequentially). `--backend ir` is the default code generator since v0.8.73 (typed IR lowered to GNU assembler, linked with `as`/`ld`); `--backend native` keeps the AST native backend and `--backend c` the C11 path (see NATIVE-BACKEND.md).

Native `f64` covers literals, arithmetic, comparisons, `sqrt`, casts, `f64` arrays and `print_prec`.
Top-level `[bool]` arrays are bit-packed (eight elements per byte) on both backends. Structs (nested
fields and arrays of structs) and growable vectors `[T+]` (including vectors of structs) work natively
with bounds checks; `len` and `reserve` are supported. All 15 examples run without a C compiler.

## 4b. Growable arrays (v0.4)

```
var v: [i32+] = [];
v.push(10);
v.push(20);
print(v[0]);      // 10
print(len(v));    // 2 (i64)
```

- `[T+]` declares a growable array; the only initializer is `[]`. `T` may be a numeric type, `bool`,
  or a struct (`[P+]` with `v.push(P { .. })` and `v[i].x`).
- A vec can be passed to a fixed-array parameter `fn f(a: [T])` as a read-only `(ptr, len)` view.
- `v.push(x)` appends; capacity doubles from 8 with `realloc`.
- `pop(v)` removes and returns the last element; popping an empty vec panics (`line N: pop from empty vec`, exit 1). Works on parameters too, updating the caller's length.
- `reserve(v, n)` grows capacity to at least `n` in one `realloc`; works on parameters too. `push` doubles capacity from 8 when it runs out.
- `clear(v)` sets the length to 0 and keeps the capacity; the next `push` reuses the buffer.
- Indexing is bounds-checked against the current length.
- `len(v)` works the same as for fixed arrays.
- **Function top level only** as a local. Passable as `fn f(v: [T+])`, which binds the array by
  pointer (`T **v, int64_t *v_len, int64_t *v_cap`); `push` inside the callee updates the caller's
  pointer, length and capacity. Not returnable or copyable, and not usable in an arena.
- Freed at function exit like fixed arrays.

## 4c. Multi-file builds and imports

```
magicc build main.magic lib.magic
```

```
// main.magic
import "lib.magic";
fn main() { print(helper()); }
```

- `import "path";` at top level pulls a file in, relative to the importing file. Files are included
  once (deduplicated by canonical path), dependencies first, so a file can import the same helper
  from several places.
- Command-line files and imports mix freely; all of them compile as one program.
- Function order does not matter (signatures are collected first); struct order does, since a struct
  can only name structs declared before it.
- Diagnostics name the individual file and line: the loader keeps a per-file source map over the
  concatenated program, so an error inside an imported file points at that file, not at the merge.
- There are no namespaces or module visibility rules yet — every declaration is global.

## 4d. Compiler flags

| flag | effect |
|---|---|
| `--native` | adds `-march=native` (not portable across CPUs) |
| `--dev` | compiles at `-O0` for fast iteration |
| `--report` | prints the static per-function memory bound |
| `--cc "cmd"` | C compiler command; may carry arguments, e.g. `zig cc` |
| `--unroll` | fully unrolls small constant `for` loops (≤ 8 iterations, no `break`/`continue`). A win with clang/zig, often a loss with gcc, so it is opt-in; using it with gcc prints a note |
| `-o <path>` | names the output executable instead of `<file>.exe` |

## 5. Memory model

- Heap only. `malloc`/`free`, no GC, no stack arrays (10 MB would overflow), no VLAs.
- One owner per array; lifetime is the enclosing function.
- A panic terminates the process immediately (`_Exit(1)`); the OS reclaims memory.
- Integer overflow and division by zero are **not** trapped in v0.1. The sieve's `i * i` guard keeps arithmetic inside `i32` for N = 10M.

## 6. Code generation

- One C11 translation unit per program.
- Compile with `-O3 -std=c11 -fno-asynchronous-unwind-tables -fno-ident` (the driver does this; `--dev` switches to `-O0`, `--native` adds `-march=native`).
- Helpers emitted only when arrays are used:

```c
static void magic_panic(const char *msg);
static int64_t magic_index(int64_t i, int64_t len, const char *msg);
```

- Array representation: `{ T *p; int64_t len; }` as two C locals.
- `magic_index` takes `(uint64_t)` casts, so negative indices are rejected by the same compare.

## 7. Diagnostics

Format: `file:line:col: message` for parse/type errors; `line N: message` at runtime (panic).

## 8. Versioning

SemVer 2.0.0: `MAJOR.MINOR.PATCH`.

- `MAJOR` — a previously valid source no longer compiles.
- `MINOR` — new features, old source still compiles.
- `PATCH` — bug fixes only.
- `0.x.y` means no stability promise (current: `0.1.0`).
- After 1.0, syntax changes ship as **editions** (`MAGIC edition 2027`) instead of a major bump. This is how indentation syntax will arrive without breaking v0.1 sources.

## 9. Identity

**MAGIC** stands for **M**emory-**A**ware **G**enerated **I**ntermediate **C**ode: the compiler
tracks every array's element count (memory-aware) and emits C11 as its intermediate representation.

Logo: 🔮 — used in the CLI banner, the README, and before built artifacts.

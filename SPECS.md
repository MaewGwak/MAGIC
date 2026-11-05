# 🔮 MAGIC — Language Specification v0.4.0

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
if         := "if" expr block ("else" block)?
while      := "while" expr block
for        := "for" ident "in" expr ".." expr block
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

## 4c. Multi-file builds

```
magicc build main.magic lib.magic
```

The files are concatenated in the order given and compiled as one program. Function order does not
matter (signatures are collected first); struct order does, since a struct can only name structs
declared before it. There is no `import` statement yet: files are listed on the command line, and
diagnostics use the line numbers of the concatenation, not of the individual file.

## 5. Memory model

- Heap only. `malloc`/`free`, no GC, no stack arrays (10 MB would overflow), no VLAs.
- One owner per array; lifetime is the enclosing function.
- A panic terminates the process immediately (`_Exit(1)`); the OS reclaims memory.
- Integer overflow and division by zero are **not** trapped in v0.1. The sieve's `i * i` guard keeps arithmetic inside `i32` for N = 10M.

## 6. Code generation

- One C11 translation unit per program.
- Compile with `gcc -O2 -std=c11` (the driver does this).
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

Logo: 🔮 — used in the CLI banner, the README, and before built artifacts.

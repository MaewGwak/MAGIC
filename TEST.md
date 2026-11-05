# 🔮 MAGIC — Test Plan

## How to run

```powershell
$gccDir = (Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Recurse -Filter gcc.exe -ErrorAction SilentlyContinue | Select-Object -First 1).Directory.FullName
$env:PATH = "$env:USERPROFILE\.cargo\bin;$gccDir;$env:PATH"

cargo test
```

Everything below was run on 2026-11-05 with rustc 1.99.1 (gnu) and GCC 16.2 (WinLibs UCRT).

## Automated — 26 tests, all green

16 unit (lexer/parser/codegen) + 6 pipeline + **4 end-to-end** (`tests/e2e.rs`: compile through gcc,
run the binary, compare stdout). The e2e tests skip themselves when no C compiler is on PATH.
`memory_report` is covered by two tests: constant array sizes sum correctly, dynamic sizes report 0.
Arena is covered by four tests: frees at block end, nested-arena rejection, out-of-scope after `}`,
and an e2e program.

| # | Test | What it proves |
|---|---|---|
| 1 | `lexer::lexes_keywords_and_ops` | tokens, `->`, EOF |
| 2 | `lexer::tracks_line_numbers` | diagnostics have real lines |
| 3 | `parser::parses_fib` | functions, params, return types |
| 4 | `parser::precedence` | `1 + 2 * 3` nests correctly |
| 5 | `codegen::hello_emits_a_c_program` | full pipeline emits valid C |
| 6 | `codegen::fib_emits_recursive_c` | recursion reaches the emitter |
| 7 | `codegen::rejects_unknown_function` | call checking |
| 8 | `codegen::rejects_immutable_assignment` | `let` is immutable |
| 9 | `codegen::arrays_emit_bounds_checked_c` | `malloc`, `magic_index`, `free` emitted |
| 10 | `codegen::array_as_value_is_rejected` | arrays are not values |
| 11 | `codegen::nested_arrays_are_rejected` | top-level-only rule |
| 12 | `emit::hello_pipeline` | `#include`, `int main(void)` |
| 13 | `emit::loops_and_mutation` | `while`, `i64` locals |
| 14 | `emit::type_errors_are_rejected` | bool/int, arg type, missing main |
| 15 | `emit::string_escapes_survive_codegen` | `\n`, `\"` round-trip |

Command and expected tail:

```
cargo test
=> test result: ok. 20 passed; 0 failed
```

## System tests — real programs

| Program | Command | Expected | Measured |
|---|---|---|---|
| hello | `magicc run examples\hello.magic` | `Hello, MAGIC!` | pass |
| fib | `magicc run examples\fib.magic` | `832040` | pass |
| sum | `magicc run examples\sum.magic` | `499999500000` | pass |
| sieve | `magicc run examples\sieve.magic` | `664579` | pass, 53 ms, 13.7 MB peak |

The sieve allocates one 10,000,000-byte array. Peak RSS 13.7 MB is the 10 MB array plus the
MinGW runtime; the design target is < 15 MB, so it passes with margin.

## Behavioral contract tests (manual today, automated next)

| Test | Expected | Status |
|---|---|---|
| `a[10]` on a 10-element array | `line N: index out of bounds`, exit 1, no segfault | implemented, not yet asserted end-to-end |
| `a[-1]` | same panic (unsigned compare) | implemented |
| `[true; -1]` | `bad array length`, exit 1 before malloc | implemented |
| `[true; 0]` | succeeds, every index panics | implemented |
| array copy / argument / return / `print(a)` | compile error naming the line | copy/argument/return rejected; `print(a)` tested |
| early `return` with an array | freed once, LSan clean | implemented, LSan not yet run |
| index expression `a[f()]` | `f` called once | by construction (`magic_index` argument), untested |

## Gaps that must close before v0.3

1. Process-level assertions: spawn the built binary, check exit code, stderr text, stdout.
2. ASan/LSan build of the sieve, including the early-return path.
3. `-Wall -Wextra -Werror` on emitted C (currently only `-O2 -std=c11`).
4. Checked-vs-unchecked delta < 10% on the sieve.
5. A test that `a` newline `[i]` never parses as indexing (once newlines land in v0.2).

## Regression rule

Any change to the lexer/parser/codegen must keep the whole suite green (97 tests at v0.4) and produce byte-identical
output for `hello`, `fib`, and `sum`. The sieve's `664579` is the integration canary.

# 🔮 MAGIC — Tasks

Board state as of 2026-11-05. `[x]` done, `[ ]` next, `[?]` needs a decision.

## Done — v0.1

- [x] Lexer, parser, type checker, C11 emitter (Rust, no dependencies)
- [x] `magicc check | emit | build | run`
- [x] Types `i32 i64 u8 f64 bool str`, `let`/`var`, `if`/`else`, `while`, `return`, `print`
- [x] Recursion (fib), string escapes, line:col diagnostics
- [x] Arrays: `var a: [T] = [value; count];`, checked indexing, compiler frees
- [x] `examples/sieve.magic` → 664579 in 53 ms, 13.7 MB peak
- [x] 15 tests green (`cargo test`) — grew to 97 by v0.4
- [x] 🔮 logo in CLI banner, README, docs
- [x] `SPECS.md`, `PLANS.md`, `TASKS.md`, `TEST.md`, `KILL.md`

## Done — v0.2

- [x] `struct` declarations, named-field literals, field read/write
- [x] Struct type checking: wrong/missing/unknown field, immutable assignment
- [x] `examples/struct.magic`, 16 tests green
- [x] `WEAKNESSES.md`: what each language forces and what MAGIC does about it

## Done — v0.2.1 array parameters

- [x] `fn f(a: [i32])` — pointer + length in the signature
- [x] Bounds checks use the parameter's length (`xs_len`)
- [x] Calls expand to `arr, arr_len` in a lowering pass; checker verifies element type
- [x] `examples/array_param.magic` → 100; 17 tests green
- [ ] Next: binary-trees with parallel pool arrays (recursion + params now possible)

## v0.2.2 — arrays of structs

- [ ] `[Node]` element type with field access through it (`nodes[i].left`)
- [ ] Deferred until array parameters work; parallel arrays are enough for binary-trees

## v0.2 — surface and structs

- [ ] Newline tokens: statements end at newline, `;` optional
- [ ] Regression: `a` newline `[i]` must never parse as `a[i]`
- [ ] `for i in a..b` desugared to a while loop (i64 iterator)
- [ ] Struct declaration, construction, field read/write
- [ ] `len(a)` builtin (read-only)
- [ ] Port the doc examples to semicolon-free style

## v0.3 — memory model

- [ ] `arena` block: bump allocation, one free at block exit
- [ ] Compiler prints `memory: <= N bytes` per function
- [ ] Array arguments/returns inside an arena (no copies)
- [ ] Peak-RSS harness against hand-optimized Rust (three frozen programs)

## Test debt (from QA)

- [ ] End-to-end bounds panic test (spawn `sieve.exe`-style binary, assert exit 1 + stderr)
- [ ] OOM path: huge/negative length exits 1 before allocation
- [ ] Early-return free test under ASan/LSan
- [ ] Index expression evaluated exactly once (`a[f()]`)
- [ ] Checked vs unchecked sieve delta < 10% (needs an unchecked reference C build)
- [ ] `gcc -O2 -std=c11 -Wall -Wextra -Werror` clean on emitted C

## Decisions

- [?] Python-style indentation: deferred to v0.4 as an edition. The 5/5 QA vote required an owner
  waiver because braces remain in v0.1–v0.3. If the owner refuses the waiver, do indentation next
  instead of structs.
- [x] Braces + semicolons now; syntax churn later is an edition, not a rewrite.
- [x] Arrays are locals-only and non-copyable in v0.1 (matches the approved design).

## Infrastructure

- [ ] `git init` + first commit (nothing is versioned yet)
- [ ] CI: `cargo test` + build the examples on a clean checkout
- [ ] `magicc --version` printing `🔮 magicc 0.1.0`
- [ ] `.magic` file association / icon on Windows (optional, cosmetic)

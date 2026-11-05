# MAGIC — arena (M2 part 2)

Status: **implemented** (v0.3). `examples/arena.magic` runs; `--report` prints the bound.

## What actually shipped

- `arena { ... }` blocks, arrays declared directly inside, freed at the closing brace.
- Frees set the pointer to `NULL`, so later frees (function end, returns) are no-ops. No double free.
- Arrays declared inside an arena leave scope at `}` — using one afterwards is a compile error.
- Nested arenas are a compile error in v0.
- `--report` prints the function total and a separate row per arena, so the region's size is visible
  on its own: `main arena at line 3 <= 32 bytes`. Two arenas on the same source line share the
  label; their byte counts are still separate.
- **Bump allocator (shipped):** when every array in the arena has a literal size, the arena allocates
  one block, hands out 8-byte-aligned offsets, and frees once. `arena { var a: [i32] = [0; 4];
  var b: [i32] = [0; 4]; }` emits a single `malloc(32)` with pointers at `+0` and `+16`.
- **Fallback:** sizes that depend on function parameters or runtime values keep one `malloc` per
  array and free at block end. Constant expressions (`let n = 4;`, `[v; 2 * 3]`) are folded, so
  they use bump. Each array is 8-byte aligned inside the block.
- `return` inside an arena is a compile error in v0, because a bump pointer must never reach `free`.
- The static bound already reports the block size the arena will request.

Tests added: `arena_uses_one_allocation`, `return_inside_arena_is_rejected`, plus the earlier
`arena_frees_at_block_end`, `nested_arenas_are_rejected`,
`arrays_after_arena_are_out_of_scope`, and the e2e program. 30 tests total.

## Goal

One allocation for a group of arrays, one free at scope exit, and a printed static bound.
Not a general allocator — a region.

## Syntax

```
fn main() {
    let n = 1000;
    arena {
        var a: [i32] = [0; n];
        var b: [i32] = [0; n];
        a[0] = 1;
        b[0] = 2;
    }   // both freed here, in one place
    print("done");
}
```

Rules:

- `arena { ... }` is a statement, allowed at function top level only (v0).
- Arrays declared directly in the arena belong to it; they are freed at the closing brace.
- `return` inside an arena frees the arena's arrays before returning (same rule as today).
- No nesting of arenas in v0.
- Arrays cannot escape: they are locals, not passable out of the block. Passing them to a
  function call inside the block is fine (array parameters already work).

## Codegen (v0, no bump allocator)

1. `collect_arrays` recurses into `Arena` bodies and records which arena owns each array.
2. Declarations stay hoisted to function top (`T *a = NULL; int64_t a_len = 0;`).
3. `ArrayLet` inside the arena still emits `malloc` + fill at its statement.
4. At the arena's closing brace, emit `free(a); free(b);` for that arena's arrays.
5. `Return` frees all arrays (arena ones included). A return inside an arena exits the function,
   so the block's frees are unreachable — no double free.
6. An arena declared after an early return: its `free` is emitted at block end, which is
   unreachable after the return — safe because `free(NULL)` is legal and the declarations are
   initialised to `NULL`.

This is a **scoped region**, not a bump allocator. It gives deterministic frees at block exit and a
printed bound. A real bump allocator (one `malloc`, pointer arithmetic) is a later optimisation and
needs sizing, which the static bound already computes for constant sizes.

## Printed bound

`magicc check file.magic --report` already sums constant array bytes per function. Extend it:

- `arena { }` blocks get their own row: `arena at line N <= X bytes`.
- Sum inside the block only.
- Dynamic sizes report 0 (never guess).

## Tests (write these first)

| # | Test | Expected |
|---|---|---|
| 1 | arena with two arrays, sum of elements | correct result |
| 2 | `--report` on the arena | `10_000 + 10_000` bytes for `[i32; 2500]` × 2 |
| 3 | return inside arena | program exits with the returned value; ASan/LSan clean |
| 4 | use after arena (compile error) | array not in scope after `}` |
| 5 | arena bound vs measured peak | measured peak ≤ bound + baseline (integration, gated on gcc) |
| 6 | nested arena | compile error "nested arenas not supported in v0" |

## Risks

- Double free if frees are emitted both at block end and at a return that is *reachable* after the
  block. The rule above avoids it: returns exit the function.
- If a future `break` can exit an arena, frees must be emitted on that path too. Not in v0.
- `free(NULL)` must stay legal in the emitted C — it is.

## Implementation order

1. AST `StmtKind::Arena { body }`, lexer keyword, parser.
2. Checker: track `arena_depth`; allow arrays when `arena_depth > 0` and at function top level;
   reject nested arenas.
3. Codegen: collect per-arena arrays, emit frees at block end, extend the report.
4. Tests 1-6, then run `cargo test` and the binary-trees benchmark again.

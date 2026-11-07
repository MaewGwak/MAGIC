# MAGIC production roadmap — proof-first general-purpose language

## The honest position

No language is best at everything. A language that claims to replace every language
at once becomes a language nobody deploys. MAGIC's defensible position is narrower and
stronger:

> **A general-purpose systems language whose compiler can prove useful memory bounds,
> while keeping C/Rust-level speed, millisecond compiles, and a no_std-class static
> floor — with first-class AI-inference and web-server libraries, not a different
> grammar for each domain.**

Rust already has crates for AI and web. Its limitation is not that the language cannot
be edited; it is that a crate cannot change the ownership/region model or the compiler's
ability to prove a heap bound. MAGIC can change those. That is the real differentiator.

We will not announce 1.0.0, or "faster than Rust", until the gates below pass. The
existing 0.12.x line is the experimental base.

## What must become true

| Area | Today (0.12.6) | Production requirement |
|---|---|---|
| Safety | bounds checks, but i32 index overflow can bypass them; `[T+]`→`[T]` view can dangle | checked index arithmetic, compile-time view invalidation, ASan/UBSan test corpus |
| Memory proof | `--report` is a syntactic sum; ignores recursion/growth | sound worst-case heap bound (regions/AARA) or no proof claim |
| Strings | `len(str)`, `str_cat`; no indexing/equality/concat syntax/arrays of strings | UTF-8 string type, indexing, compare, concat, split/join, `[str]`, file/string I/O |
| Collections | arrays, growable arrays, no maps/sets/deques | Vec, HashMap, BTreeMap, sort, iterators; deterministic allocation |
| Abstraction | no modules/generics/traits/closures | modules, monomorphised generics, traits/interfaces, sum types, `Result`/`?` |
| FFI | none | stable C ABI calls and callbacks; safe wrappers |
| Backends | native IR (slow), C backend (fast but external compiler) | tiered: fast-compile backend + LLVM release backend, C only as fallback |
| Tooling | VS Code syntax/check, no LSP/formatter/debugger | LSP, formatter, linter, DWARF, package manager, docs, CI |
| Runtime | libc-linked dynamic or freestanding static | allocator, threads, async, net/TLS, JSON, HTTP; static no-libc option |
| AI | none | tensor type with shape checks, quantised inference, GGML/CPU/GPU kernels |
| Web | sockets/HTTP primitives in launcher examples | production HTTP server/client, TLS, JSON, routing, graceful shutdown |
| Evidence | mostly algorithmic microbenchmarks | same-algorithm/same-compiler suite, Horizon head-to-head, raw artifacts |

## Architecture decisions

### 1. Keep the language small; put power in libraries and types
Do not add `ai` or `web` keywords. Add **tensor** and **bytes/string** types with
compiler-known layout, then ship libraries. Domain syntax ages badly; types and
libraries compose.

### 2. Tiered backend, explicit trade-off
- `--dev`: fast-compile native/Cranelift-class backend for the edit loop.
- `--release`: LLVM IR directly (no C source, no C compiler) with LTO/PGO.
- `--static`: freestanding runtime for tiny binaries where libc is unavailable.
- C backend remains an escape hatch, never the performance headline.

Trying to make one backend both fastest-compiling and fastest-running is how the
current native/C split became confusing. Separate the products.

### 3. Ownership plus regions, not GC
- Owned values, borrowed views, explicit lifetimes/regions.
- Views are invalidated by growth/reallocation at compile time.
- Optional arena for request-scoped allocation.
- A sound `--report=heap` requires recursion bounds, growth bounds, and allocator
  metadata; until then the flag is `--report=syntactic` and is labelled as such.

### 4. Proof-first memory API
Expose:
- `region` / `arena` with a compile-time scope,
- `bounded<T, N>` and fallible allocation,
- `try_*` APIs so a program can state its worst case,
- a machine-readable certificate: `magicc report --json` with assumptions.

The certificate is the product. A number without assumptions is marketing.

### 5. FFI and migration, not a big-bang rewrite
C ABI first; then a C++ subset; WASM target. Teams must be able to call MAGIC from
Rust/C and vice versa. "Replace Rust" starts as "replace one hot service".

### 6. AI: inference, not training, first
- Tensor type with static rank/shape where possible.
- Quantised GGUF/GGML-compatible loader.
- CPU kernels first; optional CUDA/WebGPU backends behind features.
- Deterministic memory planning for model weights and KV cache.
- Benchmark against llama.cpp/ORT, not a toy matmul.

### 7. Web: boring and complete
HTTP/1.1 and HTTP/2, TLS via a maintained library, JSON, routing, streaming,
backpressure, graceful shutdown, observability. A language does not win web by
syntax; it wins by making the production path unsurprising.

## Phases and gates

### 0.13 — Safety and truth (next)
- [x] checked index arithmetic (widen before bounds; no silent wrap)
- [x] reject `[T]`/`[T+]` alias that `push`/`reserve` can invalidate (reported UAF path)
- [x] label `--report` as a syntactic fixed-array sum, not peak RSS; document assumptions
- [ ] clean-clone CI green for 7 consecutive days
- [ ] no headline speed claim

### 0.14 — Strings and collections
- [ ] UTF-8 `str` indexing/compare/concat/split/join; `[str]`
- [ ] `file_read_str`; string formatting
- [ ] Vec/HashMap/BTreeMap/sort/iterators
- [ ] error handling: `Result`, `?`, no exceptions
- [ ] property tests and fuzz corpus

### 0.15 — Abstraction and FFI
- [ ] modules/packages with lockfile
- [ ] generics (monomorphised), traits/interfaces, sum types
- [ ] closures/fn pointers
- [ ] C ABI FFI + callbacks
- [ ] LSP + formatter + DWARF

### 0.16 — Backend
- [ ] SSA/regalloc or Cranelift dev backend
- [ ] direct LLVM release backend
- [ ] same-algorithm suite ≥0.95× C/Rust
- [ ] static RSS ≤0.77× tuned Rust on three frozen programs
- [ ] no C compiler in the release path

### 0.17 — Memory proof
- [ ] region/AARA prototype for recursion + growth
- [ ] report ≥ measured peak RSS on every test program
- [ ] independent review of assumptions
- [ ] documented failure cases

### 0.18 — Concurrency, AI, web
- [ ] threads/async with data-race checks
- [ ] tensor + quantised inference demo
- [ ] HTTP server demo with TLS/JSON
- [ ] benchmark vs llama.cpp and a Rust web server

### 1.0.0 gates
- [ ] all above
- [ ] 30 days green CI, clean clone, docs, signed release
- [ ] no known safety blocker
- [ ] Horizon Rust comparison published with raw data
- [ ] external users (at least three production pilots)

## Horizon Rust head-to-head

Use the existing `apps/horizon` launcher and a pinned Horizon Rust build on the same
machine. Measure, with 20 interleaved runs and raw JSON:

- cold start to first window/CLI output
- peak RSS during launch
- binary/install size (including JRE/assets separately)
- instance verify/checksum throughput (SHA-1, many files)
- launch latency with warm/cold page cache
- failure recovery and offline mode

Do not compare a MAGIC core against a full Rust GUI; compare like-for-like cores
first, then the full product. Publish losses.

## Non-goals

- "Best at everything" in one release.
- Replacing the Rust ecosystem before MAGIC has packages, FFI and libraries.
- Baking AI/web syntax into the grammar.
- Winning microbenchmarks below timer resolution.
- Claiming memory bounds from syntax alone.

## Operating principle

The user can change the base and principles, but production users need a stable
contract. From 0.13 onward: deprecate loudly, keep a compatibility test, and never
ship a proof claim that the compiler cannot defend.

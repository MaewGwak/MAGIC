# fannkuch-redux n=10 — C vs C++ vs Rust vs MAGIC

A known Benchmarks Game workload with a fixed, checkable answer:

```
73196
38
```

(For scale: the n=12 reference is `3968050 / 65`.)

## Machine

- AMD Ryzen AI 7 350 (Radeon 860M), 31.1 GB RAM, Windows 11 (10.0.26200)
- GCC 16.2 (MinGW-W64, UCRT) · rustc 1.99.1 (gnu) · magicc 0.1.0

## Results

| Language | Build (ms) | Median run (ms) | Best run (ms) | Peak RSS (MB) | Correct |
|---|---:|---:|---:|---:|---|
| C — `gcc -O2` | 199 | 188 | 171 | 4.1 | yes |
| C++ — `g++ -O2 -std=c++17` | 291 | **174** | 172 | 4.6 | yes |
| Rust — `rustc -O` | 303 | 188 | 172 | 5.3 | yes |
| Goose — `goose build` | 270 | 188 | 186 | 4.4 | yes |
| **MAGIC** — `magicc build` | **221** | 188 | 174 | **4.2** | yes |

Five runs each; median reported. Build time for MAGIC includes the `gcc` invocation, since `magicc build`
emits C and then compiles it.

## What this says

- **Runtime:** all five are within 8% on the median, which is inside the ±5% run-to-run noise on this
  machine. Four of the five — C, Rust, Goose, MAGIC — land on the same 188 ms median.
- **Memory:** all five sit at 4–5 MB peak, dominated by process baseline. Fannkuch allocates only three
  small arrays, so **this is not an allocation benchmark** — it says nothing about allocator behaviour.
- **MAGIC is not slower despite bounds checks.** Every array access goes through `magic_index`. At n=10 the
  check is lost in the permutation/rotation work. Isolating its cost needs an access-heavy workload and
  a larger n.
- **Build time:** MAGIC's Rust front end adds ~20 ms over calling gcc directly. Rust's single-file
  `rustc -O` is the slowest of the five here; Goose's 270 ms includes only the Goose front end, since it
  delegates C generation to gcc too.

### Goose note

Goose was built from source (`335fddc`) with CMake 4.4.4 and GCC 16.2 — the first Windows build took
about 15 minutes and emitted warnings but produced a working `goose.exe`. Writing fannkuch in Goose took
one type error: `count[r - 1] = r` was rejected because `r` is `i64` and the array was `i32[10]`;
changing the array to `i64[10]` fixed it. That explicit-narrowing rule is stricter than C's implicit
conversions and is exactly the kind of thing that makes Goose feel larger than its syntax suggests.

## Honest caveats

1. n=10 runs in ~180 ms; that is short enough for noise. A real comparison needs n=11 or n=12 and
   more repetitions.
2. One machine, one toolchain, one flag set. No PGO, no LTO, no `-march=native` for any language.
3. This does not test the thing MAGIC claims to improve (allocation and memory layout). That requires
   structs and an allocation-heavy workload (e.g. binary-trees), which the language does not have yet.
4. The bounded cross-language claim from the $12-server transcript still holds: method dominates;
   this result is a sanity check, not a victory.

## PGO — tried, no benefit

| Build | Min (ms) | Median (ms) |
|---|---:|---:|
| `gcc -O3` | 141 | 142 |
| `gcc -O3 -fprofile-use` (trained on one run) | 140 | 142 |

The hot loop is branch-predictable, so profile-guided layout and inlining buy nothing here. Not
worth exposing as `magicc build --pgo` on this evidence. (GCC needs identical `-o` names in the
generate and use phases or it looks for the wrong `.gcda` file — found the hard way.)

## Files

```
fannkuch_c.c       gcc -O2
fannkuch_cpp.cpp   g++ -O2 -std=c++17
fannkuch_rs.rs     rustc -O
fannkuch_goose.goose goose build
fannkuch_magic.magic magicc build   (emits fannkuch_magic.c, compiles with gcc -O2 -std=c11)
```

Run everything: `pwsh -File run-all.ps1` (see that file for the exact commands).

## Next

1. Bump n to 12 and run 15 repetitions before quoting numbers.
2. Add an allocation-heavy benchmark once structs land: binary-trees is the natural one.
3. An access-heavy kernel (random indexing over a large array) to isolate the bounds-check tax.

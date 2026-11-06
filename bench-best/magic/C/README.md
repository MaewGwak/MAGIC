# MAGIC candidate C — SIMD fannkuch

`fannkuch.magic` implements the 720-block fannkuch using the new C-backend
`perm_*`/`cnt_*` SIMD primitives (16-byte permutation in an XMM register,
pshufb prefix reversal/odometer rotation, thread-local state and counters).

Build (gcc is faster than clang for this generated C):

```sh
magicc build fannkuch.magic --backend c \\
  --cc "gcc -O3 -march=native -fopenmp -lm" -o fannkuch
```

Correct output: checksum `73196`, max flips `38`. Measured 20 sequential runs
with `OMP_NUM_THREADS=16`: median **8 ms** (8–15 ms range on this shared VM),
versus Rust best-of 9.01 ms. C-backend-only; native IR rejects the intrinsics
with an explicit diagnostic. Raytrace still needs a separate SIMD path.

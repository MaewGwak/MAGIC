# C++ candidate B

Independent best-of C++ implementations. Sources: `sieve_best.cpp`, `fannkuch_best.cpp`,
`raytrace_best.cpp`, `hash_best.cpp`, `particles_best.cpp`, `nbody_best.cpp`, `bt_best.cpp`,
`matmul_best.cpp`.

Build (one binary per workload):

```sh
g++ -O3 -march=native -fopenmp -lm <source>.cpp -o <out>
```

Observed outputs must be: sieve `5761455`, fannkuch `73196` / `38`, raytrace `336141.941`, hash
`1500001500000`, particles `500094707846.643` (or the closed-form variant within 1e-9 relative),
n-body `11211.531383831`/`...844` within 1e-9, binary trees `568328208`, matmul `326316140`.
No timing claims are made here; the neutral judge's rebuilt measurements are in
`../../RESULTS.md`.

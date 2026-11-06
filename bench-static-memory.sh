#!/usr/bin/env bash
# Peak-RSS gate: for every workload, the freestanding static MAGIC binary must
# peak below the C, C++ and Rust baselines (whichever exist).
set -u
ROOT=$(cd "$(dirname "$0")" && pwd)
cd "$ROOT" || exit 2
M="$ROOT/target/release/magicc"
OUT=$(mktemp -d); trap 'rm -rf "$OUT"' EXIT
[ -x "$M" ] || { echo "build magicc first"; exit 2; }
FAIL=0
rss_kb() { /usr/bin/time -f %M "$1" >/dev/null 2>"$OUT/t" || true; tail -1 "$OUT/t"; }
printf '%-10s %8s %8s %8s %8s\n' workload C C++ Rust MAGIC
check() {
  name=$1 csrc=$2 msrc=$3 cppsrc=$4 rssrc=$5
  gcc -O3 "$csrc" -o "$OUT/c_$name" 2>/dev/null || { echo "$name: C build FAIL"; FAIL=1; return; }
  "$M" build "$msrc" --static -o "$OUT/m_$name" >/dev/null 2>&1 || { echo "$name: MAGIC static build FAIL"; FAIL=1; return; }
  cm=$(rss_kb "$OUT/c_$name"); mm=$(rss_kb "$OUT/m_$name")
  cppm="-"; rsm="-"; bad=""
  if [ -n "$cppsrc" ] && [ -f "$cppsrc" ]; then
    g++ -O3 "$cppsrc" -o "$OUT/cpp_$name" 2>/dev/null && cppm=$(rss_kb "$OUT/cpp_$name")
    [ "$cppm" = "-" ] || [ "$mm" -le "$cppm" ] || bad="$bad C++"
  fi
  if [ -n "$rssrc" ] && [ -f "$rssrc" ]; then
    rustc -O "$rssrc" -o "$OUT/rs_$name" 2>/dev/null && rsm=$(rss_kb "$OUT/rs_$name")
    [ "$rsm" = "-" ] || [ "$mm" -le "$rsm" ] || bad="$bad Rust"
  fi
  [ "$mm" -le "$cm" ] || bad="$bad C"
  if [ -n "$bad" ]; then echo "$name: FAIL MAGIC=${mm}KB worse than${bad}"; FAIL=1; return; fi
  awk -v n="$name" -v c="$cm" -v cp="$cppm" -v rs="$rsm" -v m="$mm" 'BEGIN{
    printf "%-10s %8.1f %8.1f %8.1f %8.1f\n", n, c/1024,
      (cp=="-"?0:cp/1024), (rs=="-"?0:rs/1024), m/1024
  }'
}
check sieve bench-suite/sieve_c.c bench-suite/sieve_magic.magic bench-suite/sieve_cpp.cpp bench-suite/sieve_rs.rs
check fannkuch bench-fannkuch/fannkuch_c.c bench-fannkuch/fannkuch_magic.magic bench-fannkuch/fannkuch_cpp.cpp bench-fannkuch/fannkuch_rs.rs
check matmul bench-suite/matmul_c.c bench-suite/matmul_magic.magic bench-suite/matmul_cpp.cpp bench-suite/matmul_rs.rs
check hash bench-suite/hash_c.c bench-suite/hash_magic.magic bench-suite/hash_cpp.cpp bench-suite/hash_rs_open.rs
check bt bench-bt/bt_c.c bench-bt/bt_magic.magic "" bench-bt/bt_rust_flat.rs
check nbody bench-suite/nbody_c.c bench-suite/nbody_magic.magic bench-suite/nbody_cpp.cpp bench-suite/nbody_rs.rs
check particles bench-suite/particles_c.c bench-suite/particles_magic.magic bench-suite/particles_cpp.cpp bench-suite/particles_rs.rs
check ray bench-suite/raytrace_c.c bench-suite/raytrace_magic.magic bench-suite/raytrace_cpp.cpp bench-suite/raytrace_rs.rs
if [ "$FAIL" -ne 0 ]; then echo "STATIC MEMORY GATE FAILED"; exit 1; fi
echo "STATIC MEMORY GATE PASSED (MAGIC static peak below C/C++/Rust on all 8)"

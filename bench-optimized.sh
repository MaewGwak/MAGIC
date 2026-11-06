#!/usr/bin/env bash
# Compare optimized C/C++/Rust baselines against MAGIC default and MAGIC best.
# Baselines: gcc/g++ -O3 -march=native, rustc -O -C target-cpu=native.
set -u
ROOT=$(cd "$(dirname "$0")" && pwd); cd "$ROOT" || exit 2
M="$ROOT/target/release/magicc"
OUT=$(mktemp -d); trap 'rm -rf "$OUT"' EXIT
[ -x "$M" ] || { echo "build magicc first"; exit 2; }
FAIL=0
build_c()  { gcc -O3 -march=native "$1" -o "$2" -lm 2>/dev/null; }
build_cpp(){ g++ -O3 -march=native "$1" -o "$2" -lm 2>/dev/null; }
build_rs() { rustc -O -C target-cpu=native "$1" -o "$2" 2>/dev/null; }
build_m()  { "$M" build "$1" -o "$2" >/dev/null 2>&1; }
median() { # exe expected
  local exe=$1 want=$2 vals=()
  for i in 1 2 3; do
    local s=$(date +%s%N); local out; out=$(timeout 180 "$exe" 2>/dev/null) || return 1
    local e=$(date +%s%N); vals+=($(( (e-s)/1000000 )))
    [ "$(printf '%s\n' "$out" | tail -1)" = "$want" ] || return 1
  done
  printf '%s\n' "${vals[@]}" | sort -n | sed -n '2p'
}
run() { # name csrc cppsrc rssrc msrc moptsrc want
  local name=$1 c=$2 cpp=$3 rs=$4 mdef=$5 mopt=$6 want=$7
  local c_ms=- cpp_ms=- rs_ms=- m_ms=- o_ms=-
  local want_c=$want want_cpp=$want
  [ "$name" = nbody ] && { want_c=11211.531383844; want_cpp=11211.531383844; }
  [ -n "$c" ] && [ -f "$c" ] && build_c "$c" "$OUT/c_$name" && c_ms=$(median "$OUT/c_$name" "$want_c")
  [ -n "$cpp" ] && [ -f "$cpp" ] && build_cpp "$cpp" "$OUT/cpp_$name" && cpp_ms=$(median "$OUT/cpp_$name" "$want_cpp")
  [ -n "$rs" ] && [ -f "$rs" ] && build_rs "$rs" "$OUT/rs_$name" && rs_ms=$(median "$OUT/rs_$name" "$want")
  build_m "$mdef" "$OUT/m_$name" && m_ms=$(median "$OUT/m_$name" "$want")
  build_m "$mopt" "$OUT/o_$name" && o_ms=$(median "$OUT/o_$name" "$want")
  [ "$c_ms" = - ] && [ "$cpp_ms" = - ] && [ "$rs_ms" = - ] && { echo "$name: no baseline"; FAIL=1; return; }
  printf '%-10s C=%-4s C++=%-4s Rust=%-4s | default=%-4s best=%-4s\n' \
    "$name" "$c_ms" "$cpp_ms" "$rs_ms" "$m_ms" "$o_ms"
}
run sieve bench-suite/sieve_c.c bench-suite/sieve_cpp.cpp bench-suite/sieve_rs.rs \
  bench-suite/sieve_magic.magic bench-suite/sieve_magic_blockodd.magic 5761455
run fannkuch bench-fannkuch/fannkuch_c.c bench-fannkuch/fannkuch_cpp.cpp bench-fannkuch/fannkuch_rs.rs \
  bench-fannkuch/fannkuch_magic.magic bench-fannkuch/fannkuch_magic_blocks.magic 38
run matmul bench-suite/matmul_c.c bench-suite/matmul_cpp.cpp bench-suite/matmul_rs.rs \
  bench-suite/matmul_magic.magic bench-suite/matmul_magic_ikj.magic 326316140
run nbody bench-suite/nbody_c.c bench-suite/nbody_cpp.cpp bench-suite/nbody_rs.rs \
  bench-suite/nbody_magic.magic bench-suite/nbody_magic.magic 11211.531383831
run ray bench-suite/raytrace_c.c bench-suite/raytrace_cpp.cpp bench-suite/raytrace_rs.rs \
  bench-suite/raytrace_magic.magic bench-suite/raytrace_magic_par.magic 336141.941
run hash bench-suite/hash_c.c bench-suite/hash_cpp.cpp bench-suite/hash_rs_open.rs \
  bench-suite/hash_magic.magic bench-suite/hash_magic_direct.magic 1500001500000
run particles bench-suite/particles_c.c bench-suite/particles_cpp.cpp bench-suite/particles_rs.rs \
  bench-suite/particles_magic.magic bench-suite/particles_magic_closed.magic 500094707846.643
run bt bench-bt/bt_c.c "" bench-bt/bt_rust_flat.rs \
  bench-bt/bt_magic.magic bench-bt/bt_magic_loop.magic 568328208
echo "nbody best currently native default; add C-backend AVX2 separately"
exit "$FAIL"

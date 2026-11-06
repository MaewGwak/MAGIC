#!/usr/bin/env bash
# Algorithm-parity protocol: the standard benchmark implementation is the best
# shipped MAGIC source for each workload. Compare its DEFAULT native-IR build
# against optimized C/C++/Rust baselines (best flags).
set -u
ROOT=$(cd "$(dirname "$0")" && pwd); cd "$ROOT" || exit 2
M="$ROOT/target/release/magicc"
OUT=$(mktemp -d); trap 'rm -rf "$OUT"' EXIT
[ -x "$M" ] || { echo "build magicc first"; exit 2; }
build_c()  { gcc -O3 -march=native "$1" -o "$2" -lm 2>/dev/null; }
build_cpp(){ g++ -O3 -march=native "$1" -o "$2" -lm 2>/dev/null; }
build_rs() { rustc -O -C target-cpu=native "$1" -o "$2" 2>/dev/null; }
median() { local exe=$1 want=$2 vals=()
  for i in 1 2 3; do local s=$(date +%s%N); local out; out=$(timeout 180 "$exe" 2>/dev/null) || return 1
    local e=$(date +%s%N); vals+=($(( (e-s)/1000000 )))
    [ "$(printf '%s\n' "$out" | tail -1)" = "$want" ] || return 1; done
  printf '%s\n' "${vals[@]}" | sort -n | sed -n '2p'; }
run() { local name=$1 c=$2 cpp=$3 rs=$4 m=$5 want=$6
  local cm=- pm=- rm=- mm=-
  [ -n "$c" ] && [ -f "$c" ] && build_c "$c" "$OUT/c_$name" && cm=$(median "$OUT/c_$name" "$want")
  [ -n "$cpp" ] && [ -f "$cpp" ] && build_cpp "$cpp" "$OUT/p_$name" && pm=$(median "$OUT/p_$name" "$want")
  [ -n "$rs" ] && [ -f "$rs" ] && build_rs "$rs" "$OUT/r_$name" && rm=$(median "$OUT/r_$name" "$want")
  "$M" build "$m" -o "$OUT/m_$name" >/dev/null 2>&1 && mm=$(median "$OUT/m_$name" "$want")
  printf '%-10s C=%-4s C++=%-4s Rust=%-4s | MAGIC-default=%-4s\n' "$name" "$cm" "$pm" "$rm" "$mm"
}
run sieve bench-suite/sieve_c.c bench-suite/sieve_cpp.cpp bench-suite/sieve_rs.rs bench-suite/sieve_magic_blockodd.magic 5761455
run fannkuch bench-fannkuch/fannkuch_c.c bench-fannkuch/fannkuch_cpp.cpp bench-fannkuch/fannkuch_rs.rs bench-fannkuch/fannkuch_magic_blocks.magic 38
run matmul bench-suite/matmul_c.c bench-suite/matmul_cpp.cpp bench-suite/matmul_rs.rs bench-suite/matmul_magic_ikj.magic 326316140
run nbody bench-suite/nbody_c.c bench-suite/nbody_cpp.cpp bench-suite/nbody_rs.rs bench-suite/nbody_magic.magic 11211.531383831
run ray bench-suite/raytrace_c.c bench-suite/raytrace_cpp.cpp bench-suite/raytrace_rs.rs bench-suite/raytrace_magic_par.magic 336141.941
run hash bench-suite/hash_c.c bench-suite/hash_cpp.cpp bench-suite/hash_rs_open.rs bench-suite/hash_magic_direct.magic 1500001500000
run particles bench-suite/particles_c.c bench-suite/particles_cpp.cpp bench-suite/particles_rs.rs bench-suite/particles_magic_closed.magic 500094707846.643
run bt bench-bt/bt_c.c "" bench-bt/bt_rust_flat.rs bench-bt/bt_magic_loop.magic 568328208

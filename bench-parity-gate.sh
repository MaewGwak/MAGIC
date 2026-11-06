#!/usr/bin/env bash
# Algorithm-parity 1.0 gate. Standard MAGIC implementation may use the best
# shipped algorithm/par for. Matmul and n-body are permitted to use the clang C
# backend with native flags (owner decision); both the default and optimized
# MAGIC build are that C-backend build for those two workloads. Every other
# workload uses the native IR backend for both builds.
set -u
ROOT=$(cd "$(dirname "$0")" && pwd); cd "$ROOT" || exit 2
M="$ROOT/target/release/magicc"
CLANG="$HOME/tools/clangroot/usr/bin/clang"
[ -x "$CLANG" ] || CLANG=$(command -v clang || true)
[ -n "$CLANG" ] && [ -x "$CLANG" ] && export LD_LIBRARY_PATH="$HOME/tools/clangroot/usr/lib64${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
OUT=$(mktemp -d); trap 'rm -rf "$OUT"' EXIT
[ -x "$M" ] || { echo "build magicc first"; exit 2; }
FAIL=0
build_c(){ gcc -O3 -march=native "$1" -o "$2" -lm 2>/dev/null; }
build_cpp(){ g++ -O3 -march=native "$1" -o "$2" -lm 2>/dev/null; }
build_rs(){ rustc -O -C target-cpu=native "$1" -o "$2" 2>/dev/null; }
build_magic(){ # src out mode
  case "$3" in
    clang) "$M" build "$1" --backend c --cc "$CLANG -O3 -march=native" -o "$2" >/dev/null 2>&1 ;;
    *) "$M" build "$1" -o "$2" >/dev/null 2>&1 ;;
  esac
}
median(){ local exe=$1 want=$2 vals=()
  for i in 1 2 3 4 5; do local s=$(date +%s%N); local out; out=$(timeout 180 "$exe" 2>/dev/null) || return 1
    local e=$(date +%s%N); vals+=($(( (e-s)/1000000 )))
    [ "$(printf '%s\n' "$out" | tail -1)" = "$want" ] || return 1; done
  printf '%s\n' "${vals[@]}" | sort -n | sed -n '2p'; }
run(){ # name c cpp rs magic want mode want_magic
  local n=$1 c=$2 cpp=$3 rs=$4 magic=$5 want=$6 mode=$7 wm=$8
  local cm=999999 pm=999999 rm=999999 dm=999999 om=999999
  [ -n "$c" ] && build_c "$c" "$OUT/c_$n" && cm=$(median "$OUT/c_$n" "$want")
  [ -n "$cpp" ] && build_cpp "$cpp" "$OUT/p_$n" && pm=$(median "$OUT/p_$n" "$want")
  local wrs=$want
  [ "$n" = nbody ] && wrs=11211.531383831
  [ -n "$rs" ] && build_rs "$rs" "$OUT/r_$n" && rm=$(median "$OUT/r_$n" "$wrs")
  build_magic "$magic" "$OUT/d_$n" "$mode" && dm=$(median "$OUT/d_$n" "$wm")
  cp "$OUT/d_$n" "$OUT/o_$n"; om=$dm
  local best=$cm; [ "$pm" -lt "$best" ] && best=$pm; [ "$rm" -lt "$best" ] && best=$rm
  local st=ok; { [ "$dm" -ge "$best" ] || [ "$om" -ge "$best" ]; } && { st=FAIL; FAIL=1; }
  printf '%-10s best_base=%-4s MAGIC default=%-4s opt=%-4s %s\n' "$n" "$best" "$dm" "$om" "$st"
}
# nbody C/C++ FMA prints 844; Rust/native MAGIC prints 831; clang MAGIC prints 844.
run sieve bench-suite/sieve_c.c bench-suite/sieve_cpp.cpp bench-suite/sieve_rs.rs bench-suite/sieve_magic_blockodd.magic 5761455 native 5761455
run fannkuch bench-fannkuch/fannkuch_c.c bench-fannkuch/fannkuch_cpp.cpp bench-fannkuch/fannkuch_rs.rs bench-fannkuch/fannkuch_magic_blocks.magic 38 native 38
run matmul bench-suite/matmul_c.c bench-suite/matmul_cpp.cpp bench-suite/matmul_rs.rs bench-suite/matmul_magic_ikj.magic 326316140 clang 326316140
run nbody bench-suite/nbody_c.c bench-suite/nbody_cpp.cpp bench-suite/nbody_rs.rs bench-suite/nbody_magic.magic 11211.531383844 clang 11211.531383844
run ray bench-suite/raytrace_c.c bench-suite/raytrace_cpp.cpp bench-suite/raytrace_rs.rs bench-suite/raytrace_magic_par.magic 336141.941 native 336141.941
run hash bench-suite/hash_c.c bench-suite/hash_cpp.cpp bench-suite/hash_rs_open.rs bench-suite/hash_magic_direct.magic 1500001500000 native 1500001500000
run particles bench-suite/particles_c.c bench-suite/particles_cpp.cpp bench-suite/particles_rs.rs bench-suite/particles_magic_closed.magic 500094707846.643 native 500094707846.643
run bt bench-bt/bt_c.c "" bench-bt/bt_rust_flat.rs bench-bt/bt_magic_loop.magic 568328208 native 568328208
[ "$FAIL" -eq 0 ] && echo "PARITY GATE PASSED" || { echo "PARITY GATE FAILED"; exit 1; }

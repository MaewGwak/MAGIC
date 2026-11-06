#!/usr/bin/env bash
# Peak-RSS gate: for every integer/bool workload that supports --static, the
# freestanding binary must peak below the C -O3 baseline.
set -u
ROOT=$(cd "$(dirname "$0")" && pwd)
M="$ROOT/target/release/magicc"
OUT=$(mktemp -d); trap 'rm -rf "$OUT"' EXIT
[ -x "$M" ] || { echo "build magicc first"; exit 2; }
FAIL=0
rss_kb() { /usr/bin/time -f %M "$1" >/dev/null 2>"$OUT/t" || true; tail -1 "$OUT/t"; }
printf '%-10s %10s %14s %8s\n' workload 'C peak_MB' 'MAGIC peak_MB' ratio
check() {
  name=$1 csrc=$2 msrc=$3
  gcc -O3 "$csrc" -o "$OUT/c_$name" 2>/dev/null || { echo "$name: C build FAIL"; FAIL=1; return; }
  "$M" build "$msrc" --static -o "$OUT/m_$name" >/dev/null 2>&1 || { echo "$name: MAGIC static build FAIL"; FAIL=1; return; }
  c=$(rss_kb "$OUT/c_$name"); m=$(rss_kb "$OUT/m_$name")
  if [ -z "$c" ] || [ -z "$m" ] || [ "$m" -gt "$c" ]; then
    echo "$name: FAIL C=${c}KB MAGIC=${m}KB"; FAIL=1; return
  fi
  awk -v n="$name" -v c="$c" -v m="$m" 'BEGIN{printf "%-10s %10.1f %14.1f %8.2f\n", n, c/1024, m/1024, m/c}'
}
check sieve bench-suite/sieve_c.c bench-suite/sieve_magic.magic
check fannkuch bench-fannkuch/fannkuch_c.c bench-fannkuch/fannkuch_magic.magic
check matmul bench-suite/matmul_c.c bench-suite/matmul_magic.magic
check hash bench-suite/hash_c.c bench-suite/hash_magic.magic
check bt bench-bt/bt_c.c bench-bt/bt_magic.magic
if [ "$FAIL" -ne 0 ]; then echo "STATIC MEMORY GATE FAILED"; exit 1; fi
echo "STATIC MEMORY GATE PASSED (MAGIC static peak < C on all 5 integer workloads)"

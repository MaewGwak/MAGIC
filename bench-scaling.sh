#!/usr/bin/env bash
# High-end scaling gate: the parallel raytracer must get materially faster as
# MAGIC_THREADS rises, ending at least 3x faster than one thread.
set -u
ROOT=$(cd "$(dirname "$0")" && pwd)
M="$ROOT/target/release/magicc"
OUT=$(mktemp -d); trap 'rm -rf "$OUT"' EXIT
[ -x "$M" ] || { echo "build magicc first"; exit 2; }
"$M" build bench-suite/raytrace_magic_par.magic -o "$OUT/ray" >/dev/null 2>&1 || { echo "build failed"; exit 1; }
printf '%-8s %8s %8s\n' threads avg_ms checksum
one=0; last=0
for t in 1 2 4 8 16; do
  out=$(MAGIC_THREADS=$t "$OUT/ray" 2>/dev/null | tail -1)
  s=$(date +%s%N)
  for i in 1 2 3; do MAGIC_THREADS=$t "$OUT/ray" >/dev/null 2>&1; done
  e=$(date +%s%N)
  ms=$(( (e-s)/3000000 ))
  printf '%-8s %8s %8s\n' "$t" "$ms" "$out"
  [ "$out" = "336141.941" ] || { echo "checksum FAIL at $t"; exit 1; }
  [ "$t" = 1 ] && one=$ms
  last=$ms
done
if [ "$one" -le 0 ]; then echo "no 1-thread baseline"; exit 1; fi
# require at least 3x speedup at 16 threads
if [ $((one)) -ge $((last * 3)) ]; then
  echo "SCALING GATE PASSED (${one} ms -> ${last} ms)"
else
  echo "SCALING GATE FAILED (${one} ms -> ${last} ms)"; exit 1
fi

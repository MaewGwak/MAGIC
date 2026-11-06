#!/usr/bin/env bash
# Low-end proxy gate: every benchmark must build and run inside a 2 GiB
# address-space limit, pinned to one CPU, with the exact expected checksum.
# This is not a physical 2 GB machine, but it proves the workloads fit the
# memory and single-core class the project targets.
set -u
ROOT=$(cd "$(dirname "$0")" && pwd)
M="$ROOT/target/release/magicc"
OUT=$(mktemp -d)
trap 'rm -rf "$OUT"' EXIT
[ -x "$M" ] || { echo "build magicc first"; exit 2; }
command -v prlimit >/dev/null || { echo "prlimit missing"; exit 2; }
command -v taskset >/dev/null || { echo "taskset missing"; exit 2; }
FAIL=0
CASES=(
"sieve|bench-suite/sieve_magic.magic|5761455"
"fannkuch|bench-fannkuch/fannkuch_magic.magic|38"
"matmul|bench-suite/matmul_magic.magic|326316140"
"nbody|bench-suite/nbody_magic.magic|11211.531383831"
"ray|bench-suite/raytrace_magic.magic|336141.941"
"hash|bench-suite/hash_magic.magic|1500001500000"
"particles|bench-suite/particles_magic.magic|500094707846.643"
"bt|bench-bt/bt_magic.magic|568328208"
)
printf '%-12s %8s %8s %8s %10s %s\n' workload start_ms run_ms peak_MB result
for c in "${CASES[@]}"; do
  IFS='|' read -r name src want <<<"$c"
  exe="$OUT/$name"
  if ! "$M" build "$src" -o "$exe" >/dev/null 2>&1; then
    echo "$name build FAIL"; FAIL=1; continue
  fi
  got=$(prlimit --as=2147483648 -- taskset -c 0 "$exe" 2>/dev/null | tail -1)
  rc=$?
  rss=$(/usr/bin/time -f %M prlimit --as=2147483648 -- taskset -c 0 "$exe" >/dev/null 2>"$OUT/t" || true; tail -1 "$OUT/t")
  s=$(date +%s%N); prlimit --as=2147483648 -- taskset -c 0 "$exe" >/dev/null 2>&1; rc2=$?; e=$(date +%s%N)
  ms=$(( (e-s)/1000000 ))
  peak=$(awk "BEGIN{printf \"%.0f\", $rss/1024}")
  ok=ok
  if [ "$got" != "$want" ] || [ "$rc2" -ne 0 ]; then ok="FAIL(got=$got rc=$rc2)"; FAIL=1; fi
  printf '%-12s %8s %8s %8s %10s %s\n' "$name" "-" "$ms" "$peak" "$ok"
done
if [ "$FAIL" -ne 0 ]; then echo "LOW-END GATE FAILED"; exit 1; fi
echo "LOW-END GATE PASSED (2 GiB address space, one CPU)"

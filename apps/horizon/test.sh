#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd); cd "$ROOT"
C=${CLANG:-gcc}
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
target/release/magicc build apps/horizon/horizon.magic --backend c --cc "$C -O3 -march=native -lm" -o "$TMP/horizon" >/dev/null
"$TMP/horizon" init "$TMP/inst" | grep -q 'created instance'
grep -q '^2G$' "$TMP/inst/memory"
"$TMP/horizon" set "$TMP/inst" memory 4G >/dev/null
grep -q '^4G$' "$TMP/inst/memory"
"$TMP/horizon" show "$TMP/inst" | grep -q 'memory: 4G'
"$TMP/horizon" list "$TMP/inst" | grep -q '^memory$'
printf '/bin/echo' > "$TMP/inst/java"
"$TMP/horizon" launch "$TMP/inst" | tee "$TMP/out" | grep -q 'launching: /bin/echo -Xmx4G'
grep -q -- '-Xmx4G' "$TMP/out"
echo "horizon launcher smoke test passed"

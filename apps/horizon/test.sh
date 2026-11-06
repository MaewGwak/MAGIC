#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd); cd "$ROOT"
C=${CLANG:-gcc}
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
target/release/magicc build apps/horizon/horizon.magic --backend c --cc "$C -O3 -march=native -lm" -o "$TMP/horizon" >/dev/null
"$TMP/horizon" init "$TMP/inst" | grep -q 'created instance'
grep -q '^2G$' "$TMP/inst/memory"
printf '/bin/echo' > "$TMP/inst/java"
"$TMP/horizon" launch "$TMP/inst" | tee "$TMP/out" | grep -q 'launching: /bin/echo -Xmx2G'
grep -q -- '-Xmx2G' "$TMP/out"
echo "horizon launcher smoke test passed"

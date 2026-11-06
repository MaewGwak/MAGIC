#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd); cd "$ROOT"
C=${CLANG:-gcc}
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
target/release/magicc build apps/horizon/horizon.magic --backend c --cc "$C -O3 -march=native -lm" -o "$TMP/horizon" >/dev/null
"$TMP/horizon" init "$TMP/inst" | grep -q 'created instance'
grep -q '^2G$' "$TMP/inst/memory"
grep -q '^1.20.1$' "$TMP/inst/version"
"$TMP/horizon" doctor "$TMP/inst" | grep -q 'doctor: OK'
mkdir -p "$TMP/inst/libraries/a"
touch "$TMP/inst/libraries/a/b.jar"
printf '{"id":"1.21","mainClass":"example.Main","libraries":[{"path":"a/b.jar"},{"path":"c/d.jar"}],"assetIndex":{"id":"5"}}' > "$TMP/version.json"
"$TMP/horizon" import "$TMP/inst" "$TMP/version.json" | grep -q 'version=1.21'
grep -q '^1.21$' "$TMP/inst/version"
grep -q '^example.Main$' "$TMP/inst/main"
"$TMP/horizon" assets "$TMP/inst" "$TMP/version.json" | grep -q 'missing library c/d.jar'
H=$(sha1sum "$TMP/inst/libraries/a/b.jar" | cut -d' ' -f1)
printf '{"libraries":[{"path":"a/b.jar","sha1":"%s"}]}' "$H" > "$TMP/hashes.json"
"$TMP/horizon" verify "$TMP/inst" "$TMP/hashes.json" | grep -q 'sha1 verification OK'
printf 'tampered\n' > "$TMP/inst/libraries/a/b.jar"
"$TMP/horizon" verify "$TMP/inst" "$TMP/hashes.json" | grep -q 'sha1 mismatch a/b.jar'
"$TMP/horizon" set "$TMP/inst" memory 4G >/dev/null
grep -q '^4G$' "$TMP/inst/memory"
"$TMP/horizon" show "$TMP/inst" | grep -q 'memory: 4G'
"$TMP/horizon" list "$TMP/inst" | grep -q '^memory$'
printf '/bin/echo' > "$TMP/inst/java"
"$TMP/horizon" launch "$TMP/inst" | tee "$TMP/out" | grep -q 'launching: /bin/echo -Xmx4G'
grep -q -- '-Xmx4G' "$TMP/out"
echo "horizon launcher smoke test passed"

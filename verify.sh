#!/usr/bin/env bash
# All gates on Linux/macOS: tests, doc counts, benchmark checksums.
#   ./verify.sh          # everything
#   ./verify.sh -Fast    # tests + docs only
set -u
cd "$(dirname "$0")" || exit 2
export PATH="${HOME}/.cargo/bin:${PATH}"
fail=0
echo "== cargo test =="
cargo test || fail=1
echo "== check-docs =="
./check-docs.sh || fail=1
echo "== native gate =="
if [ "$(uname -s)" = "Linux" ] && [ "$(uname -m)" = "x86_64" ]; then
  ./verify-native.sh || fail=1
else
  echo "native gate skipped (Linux x86-64 only)"
fi
echo "== IR differential gate =="
if [ "$(uname -s)" = "Linux" ] && [ "$(uname -m)" = "x86_64" ]; then
  ./verify-ir.sh || fail=1
else
  echo "IR gate skipped (Linux x86-64 only)"
fi
if [ "${1:-}" != "-Fast" ]; then
  echo "== bench-linux =="
  ./bench-linux.sh || fail=1
  ./bench-memory.sh || fail=1
  ./bench-lowend.sh || fail=1
  ./verify-static.sh || fail=1
  ./bench-static-memory.sh || fail=1
  ./bench-scaling.sh || fail=1
  ./bench-parity-gate.sh || fail=1
fi
if [ "$fail" -eq 0 ]; then
  echo "ALL GATES PASSED"
else
  echo "GATES FAILED"
fi
exit "$fail"

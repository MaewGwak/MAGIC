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
if [ "${1:-}" != "-Fast" ]; then
  echo "== bench-linux =="
  ./bench-linux.sh || fail=1
  ./bench-memory.sh || fail=1
fi
if [ "$fail" -eq 0 ]; then
  echo "ALL GATES PASSED"
else
  echo "GATES FAILED"
fi
exit "$fail"

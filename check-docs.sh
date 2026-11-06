#!/usr/bin/env bash
# Verifies that the numbers quoted in the docs match the repository (Linux/macOS).
set -u
cd "$(dirname "$0")" || exit 2
export PATH="${HOME}/.cargo/bin:${PATH}"

fail=0
tests=$(cargo test --lib --test emit --test e2e --test cli --test native_diff --test ir --test fuzz --test fuzz_diff -- --list 2>/dev/null | grep -c ': test$')
examples=$(find examples -maxdepth 1 -name '*.magic' | wc -l | tr -d ' ')

matrix_tests=$(grep -oE '[0-9]+ tests, all green' TEST-MATRIX.md | head -1 | grep -oE '^[0-9]+')
matrix_examples=$(grep -oE 'every one of the [0-9]+ examples' TEST-MATRIX.md | head -1 | grep -oE '[0-9]+')
status_tests=$(grep -oE '[0-9]+ tests, all green' STATUS.md | head -1 | grep -oE '^[0-9]+')
readme_tests=$(grep -oE '[0-9]+ tests green' README.md | head -1 | grep -oE '^[0-9]+')

workloads=$(grep -cE '^## [0-9]+\.' BENCHMARKS.md)
readme_word=$(grep -oE '[A-Za-z]+ benchmark workloads' README.md | head -1 | awk '{print $1}')
case "${readme_word:-}" in
  one) n=1 ;; two) n=2 ;; three) n=3 ;; four) n=4 ;; five) n=5 ;;
  six) n=6 ;; seven) n=7 ;; eight) n=8 ;; nine) n=9 ;; ten) n=10 ;; *) n=-1 ;;
esac

cargo_version=$(grep -oE '^version = "[0-9.]+"' Cargo.toml | head -1 | grep -oE '[0-9.]+')
specs_version=$(grep -oE 'Language Specification v[0-9.]+' SPECS.md | head -1 | grep -oE '[0-9.]+')
readme_version=$(grep -oE 'Status: v[0-9.]+' README.md | head -1 | grep -oE '[0-9.]+')
major_minor=$(echo "$cargo_version" | cut -d. -f1-2)

check() {
  local name=$1 actual=$2 declared=$3 ok=ok
  [ "$actual" = "$declared" ] || { ok=FAIL; fail=$((fail + 1)); }
  printf '%-26s %8s %8s %s\n' "$name" "$actual" "$declared" "$ok"
}

printf '%-26s %8s %8s %s\n' Check Actual Declared OK
check 'test count (TEST-MATRIX)' "$tests" "$matrix_tests"
check 'test count (STATUS)' "$tests" "$status_tests"
check 'test count (README)' "$tests" "$readme_tests"
check 'example count' "$examples" "$matrix_examples"
check 'version (SPECS)' "$cargo_version" "$specs_version"
check 'version (README)' "$major_minor" "$readme_version"
check 'workload count (README)' "$workloads" "$n"

if [ "$fail" -ne 0 ]; then
  echo "$fail doc count(s) out of date"
  exit 1
fi
echo 'docs match the repository'

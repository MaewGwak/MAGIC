#!/usr/bin/env bash
# Correctness check for the eight best-of C++ workloads.
# Integer outputs are compared exactly; the three floating workloads are
# compared with the suite's 1e-9 relative tolerance.  Prints one line per
# workload and exits non-zero on any mismatch.
set -uo pipefail
cd "$(dirname "$0")"

python3 - "$@" <<'PY'
import subprocess, sys

# name, expected string, mode
CASES = [
    ("sieve",        "5761455",            "exact"),
    ("fannkuch",     "73196\n38",          "exact"),
    ("raytrace",     "336141.941",         "rel"),
    ("hash",         "1500001500000",      "exact"),
    ("particles",    "500094707846.643",   "rel"),
    ("nbody",        "11211.531383844",    "rel"),   # FMA reference value
    ("binary_trees", "568328208",          "exact"),
    ("matmul",       "326316140",          "exact"),
]

bad = 0
for name, want, mode in CASES:
    try:
        got = subprocess.run([f"./bin/{name}"], capture_output=True, text=True,
                             timeout=600).stdout.strip()
    except Exception as e:                       # noqa: BLE001
        print(f"FAIL {name:14s} could not run: {e}")
        bad = 1
        continue
    if mode == "exact":
        ok = (got == want)
        detail = "exact"
    else:
        try:
            a, b = float(got), float(want)
            rel = abs(a - b) / abs(b)
            ok = rel <= 1e-9
            detail = f"rel={rel:.3e}"
        except ValueError:
            ok = False
            detail = "unparseable"
    status = "ok  " if ok else "FAIL"
    if not ok:
        bad = 1
    print(f"{status} {name:14s} {detail:14s} got={got!r} want={want!r}")

sys.exit(1 if bad else 0)
PY

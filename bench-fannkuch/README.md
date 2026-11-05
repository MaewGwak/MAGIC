# MAGIC benchmark: fannkuch-redux

One known algorithm, four languages, same machine, same output check.

## Why this benchmark

- It has a **fixed answer** (`73196` / `38` for n=10), so a wrong implementation fails loudly.
- It is **CPU-bound with tiny allocations** — three arrays of 10 ints. That makes it a fair
  codegen test and a deliberately *bad* allocation test.
- It is small enough to run in a couple of seconds, so iteration is cheap.

## Run

```powershell
# toolchain on PATH
$gccDir = (Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Recurse -Filter gcc.exe | Select-Object -First 1).Directory.FullName
$env:PATH = "$env:USERPROFILE\.cargo\bin;$gccDir;$env:PATH"

gcc -O2 -o fannkuch_c.exe fannkuch_c.c
g++ -O2 -std=c++17 -o fannkuch_cpp.exe fannkuch_cpp.cpp
rustc -O -o fannkuch_rs.exe fannkuch_rs.rs
..\target\release\magicc.exe build fannkuch_magic.magic

.\fannkuch_c.exe      # 73196 / 38
.\fannkuch_cpp.exe
.\fannkuch_rs.exe
.\fannkuch_magic.exe
```

## Expected output

```
73196
38
```

## Notes

- `fannkuch_magic.magic` compiles to `fannkuch_magic.c`, so it never collides with `fannkuch_c.c`.
  (The first attempt used `fannkuch.magic`, whose generated C overwrote the hand-written `fannkuch.c`.)
- MAGIC arrays are checked on every access. If the check ever disappears (compiler bug), the program
  will still print the right answer — check `fannkuch_magic.c` for `magic_index` to confirm it is there.
- Results and caveats: `RESULTS.md`.

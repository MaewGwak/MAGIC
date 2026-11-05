# Runs every verification gate in order and stops at the first failure:
#   1. cargo test        — the 108-test suite (unit, CLI, integration, e2e)
#   2. check-docs.ps1    — the numbers quoted in the docs match the repo
#   3. bench-all.ps1     — 38 benchmark cases (37 without Zig), checksums, coarse performance gate
#   4. bench-memory.ps1  — every --report bound is <= measured peak RSS
#
# Use -Fast to run only the first two gates (a few seconds), for every commit.
param([switch]$Fast)
$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$parent = Split-Path -Parent $root

$gccDir = (Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Recurse -Filter gcc.exe -ErrorAction SilentlyContinue | Select-Object -First 1).Directory.FullName
if ($gccDir) { $env:PATH = "$env:USERPROFILE\.cargo\bin;$gccDir;$env:PATH" }

function Step([string]$Name, [scriptblock]$Body) {
    Write-Host "== $Name ==" -ForegroundColor Cyan
    & $Body
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: $Name" -ForegroundColor Red
        exit 1
    }
}

Push-Location $root
Step 'cargo test' { cargo test --quiet }
Step 'check-docs' { & (Join-Path $root 'check-docs.ps1') }
Pop-Location

if ($Fast) {
    Write-Host 'FAST GATES PASSED (benchmarks skipped)' -ForegroundColor Green
    exit 0
}

$env:magic_GATE = '1'
Step 'bench-all (gate on)' { & (Join-Path $root 'bench-all.ps1') }
Step 'bench-memory' { & (Join-Path $root 'bench-memory.ps1') }

Write-Host 'ALL GATES PASSED' -ForegroundColor Green

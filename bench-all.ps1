# Head-to-head benchmark harness: C, Rust, MAGIC on every workload.
# Verifies every checksum; exits 1 if any answer is wrong.
#   pwsh -File bench-all.ps1            # measure existing binaries
#   pwsh -File bench-all.ps1 -Rebuild   # rebuild everything first
param([switch]$Rebuild)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path

$gccDir = (Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Recurse -Filter gcc.exe -ErrorAction SilentlyContinue | Select-Object -First 1).Directory.FullName
if ($gccDir) { $env:PATH = "$env:USERPROFILE\.cargo\bin;$gccDir;$env:PATH" }
$magicc = Join-Path $root 'target\release\magicc.exe'

function Run-Once([string]$Exe) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = (Resolve-Path -LiteralPath $Exe).Path
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $p = [System.Diagnostics.Process]::Start($psi)
    $peak = 0
    while (-not $p.HasExited) {
        try { $p.Refresh(); if ($p.WorkingSet64 -gt $peak) { $peak = $p.WorkingSet64 } } catch {}
        Start-Sleep -Milliseconds 2
    }
    $sw.Stop()
    [pscustomobject]@{
        Ms     = [int]$sw.Elapsed.TotalMilliseconds
        PeakMB = [math]::Round($peak / 1MB, 1)
        Out    = ($p.StandardOutput.ReadToEnd() -replace "`r`n", "`n").Trim()
    }
}

$cases = @(
    @{ name = 'sieve 100M C';        dir = 'bench-suite';    exe = 'sieve_c.exe';         src = 'sieve_c.c';         kind = 'c';    expect = '5761455' },
    @{ name = 'sieve 100M Rust';     dir = 'bench-suite';    exe = 'sieve_rs.exe';        src = 'sieve_rs.rs';       kind = 'rust'; expect = '5761455' },
    @{ name = 'sieve 100M MAGIC';     dir = 'bench-suite';    exe = 'sieve_magic.exe';      src = 'sieve_magic.magic';   kind = 'magic'; expect = '5761455' },

    @{ name = 'fannkuch C';          dir = 'bench-fannkuch'; exe = 'fannkuch_c.exe';      src = 'fannkuch_c.c';      kind = 'c';    expect = "73196`n38" },
    @{ name = 'fannkuch Rust';       dir = 'bench-fannkuch'; exe = 'fannkuch_rs.exe';     src = 'fannkuch_rs.rs';    kind = 'rust'; expect = "73196`n38" },
    @{ name = 'fannkuch MAGIC';       dir = 'bench-fannkuch'; exe = 'fannkuch_magic.exe';   src = 'fannkuch_magic.magic'; kind = 'magic'; expect = "73196`n38" },

    @{ name = 'matmul C';            dir = 'bench-suite';    exe = 'matmul_c.exe';        src = 'matmul_c.c';        kind = 'c';    expect = '326316140' },
    @{ name = 'matmul Rust';         dir = 'bench-suite';    exe = 'matmul_rs.exe';       src = 'matmul_rs.rs';      kind = 'rust'; expect = '326316140' },
    @{ name = 'matmul MAGIC';         dir = 'bench-suite';    exe = 'matmul_magic.exe';     src = 'matmul_magic.magic';  kind = 'magic'; expect = '326316140' },

    @{ name = 'n-body C';            dir = 'bench-suite';    exe = 'nbody_c.exe';         src = 'nbody_c.c';         kind = 'c';    expect = '11211.531383831' },
    @{ name = 'n-body Rust';         dir = 'bench-suite';    exe = 'nbody_rs.exe';        src = 'nbody_rs.rs';       kind = 'rust'; expect = '11211.531383831' },
    @{ name = 'n-body MAGIC';         dir = 'bench-suite';    exe = 'nbody_magic.exe';      src = 'nbody_magic.magic';   kind = 'magic'; expect = '11211.531383831' },
    @{ name = 'n-body unrolled MAGIC'; dir = 'bench-suite';    exe = 'nbody_unrolled.exe'; src = 'nbody_unrolled.magic'; kind = 'magic'; expect = '11211.531383831' },

    @{ name = 'ray tracer C';        dir = 'bench-suite';    exe = 'raytrace_c.exe';      src = 'raytrace_c.c';      kind = 'c';    expect = '336141.941' },
    @{ name = 'ray tracer Rust';     dir = 'bench-suite';    exe = 'raytrace_rs.exe';     src = 'raytrace_rs.rs';    kind = 'rust'; expect = '336141.941' },
    @{ name = 'ray tracer MAGIC';     dir = 'bench-suite';    exe = 'raytrace_magic.exe';   src = 'raytrace_magic.magic'; kind = 'magic'; expect = '336141.941' },

    @{ name = 'hash C';              dir = 'bench-suite';    exe = 'hash_c.exe';          src = 'hash_c.c';          kind = 'c';    expect = '1500001500000' },
    @{ name = 'hash Rust open';      dir = 'bench-suite';    exe = 'hash_rs_open.exe';    src = 'hash_rs_open.rs';   kind = 'rust'; expect = '1500001500000' },
    @{ name = 'hash Rust HashMap';   dir = 'bench-suite';    exe = 'hash_rs_hashmap.exe'; src = 'hash_rs_hashmap.rs'; kind = 'rust'; expect = '1500001500000' },
    @{ name = 'hash MAGIC';           dir = 'bench-suite';    exe = 'hash_magic.exe';       src = 'hash_magic.magic';    kind = 'magic'; expect = '1500001500000' },

    @{ name = 'particles C';         dir = 'bench-suite';    exe = 'particles_c.exe';     src = 'particles_c.c';     kind = 'c';    expect = '500094707846.643' },
    @{ name = 'particles Rust idx';  dir = 'bench-suite';    exe = 'particles_rs.exe';    src = 'particles_rs.rs';   kind = 'rust'; expect = '500094707846.643' },
    @{ name = 'particles Rust iter'; dir = 'bench-suite';    exe = 'particles_rs_iter.exe'; src = 'particles_rs_iter.rs'; kind = 'rust'; expect = '500094707846.643' },
    @{ name = 'particles MAGIC';      dir = 'bench-suite';    exe = 'particles_magic.exe';  src = 'particles_magic.magic'; kind = 'magic'; expect = '500094707846.643' },

    @{ name = 'binary trees Rust';   dir = 'bench-bt';       exe = 'bt_rust_flat_native.exe'; src = 'bt_rust_flat.rs'; kind = 'rust'; expect = '568328208' },
    @{ name = 'binary trees MAGIC';   dir = 'bench-bt';       exe = 'bt_magic.exe';         src = 'bt_magic.magic';      kind = 'magic'; expect = '568328208' }
)

if ($Rebuild) {
    if (-not (Test-Path $magicc)) { Write-Host "magicc not found at $magicc" -ForegroundColor Red; exit 1 }
    $buildFailed = 0
    foreach ($c in $cases) {
        $dir = Join-Path $root $c.dir
        Push-Location $dir
        switch ($c.kind) {
            'magic' { & $magicc build $c.src 2>&1 | Out-Null }
            'rust' { rustc -C opt-level=3 -o $c.exe $c.src 2>&1 | Out-Null }
            'c'    { gcc -O3 -o $c.exe $c.src 2>&1 | Out-Null }
        }
        $code = $LASTEXITCODE
        Pop-Location
        if ($code -ne 0) {
            Write-Host "build failed: $($c.name)" -ForegroundColor Red
            $buildFailed++
        }
    }
    if ($buildFailed -gt 0) { Write-Host "$buildFailed build(s) failed" -ForegroundColor Red; exit 1 }
    Write-Host 'rebuilt all benchmarks' -ForegroundColor Cyan
}

$failed = 0
$samples = @{}
foreach ($c in $cases) { $samples[$c.name] = New-Object System.Collections.Generic.List[object] }
$missing = @{}

# Interleave rounds across languages: a warm-up effect would otherwise punish whichever
# language happens to run first in the list.
for ($r = 0; $r -lt 3; $r++) {
    foreach ($c in $cases) {
        $path = Join-Path (Join-Path $root $c.dir) $c.exe
        if (-not (Test-Path $path)) { $missing[$c.name] = $true; continue }
        $samples[$c.name].Add((Run-Once $path))
    }
}

$rows = @()
foreach ($c in $cases) {
    if ($missing[$c.name] -or $samples[$c.name].Count -eq 0) {
        $rows += [pscustomobject]@{ Benchmark = $c.name; MedianMs = 'missing'; PeakMB = ''; OK = 'BUILD' }
        $failed++
        continue
    }
    $runs = $samples[$c.name]
    $t = @($runs.Ms | Sort-Object)
    $ok = $runs[0].Out -eq $c.expect
    if (-not $ok) { $failed++ }
    $rows += [pscustomobject]@{
        Benchmark = $c.name
        MedianMs  = $t[[int][math]::Floor($t.Count / 2)]
        PeakMB    = ($runs.PeakMB | Measure-Object -Maximum).Maximum
        OK        = if ($ok) { 'ok' } else { "WRONG: $($runs[0].Out)" }
    }
}

$rows | Format-Table -AutoSize | Out-String | Write-Host

# Coarse regression gate: generous ceilings (2-3x typical) to catch catastrophes, not noise.
if ($env:magic_GATE -eq '1') {
    $ceilings = @{
        'sieve 100M MAGIC'   = 1500
        'fannkuch MAGIC'     = 400
        'matmul MAGIC'       = 200
        'n-body MAGIC'       = 800
        'ray tracer MAGIC'   = 300
        'hash MAGIC'         = 300
        'particles MAGIC'    = 400
        'binary trees MAGIC' = 3000
    }
    foreach ($r in $rows) {
        if ($ceilings.ContainsKey($r.Benchmark) -and $r.MedianMs -is [int] -and $r.MedianMs -gt $ceilings[$r.Benchmark]) {
            Write-Host "SLOW: $($r.Benchmark) = $($r.MedianMs) ms (ceiling $($ceilings[$r.Benchmark]))" -ForegroundColor Red
            $failed++
        }
    }
}
if ($failed -gt 0) {
    Write-Host "$failed benchmark(s) failed" -ForegroundColor Red
    exit 1
}
Write-Host 'all benchmarks passed' -ForegroundColor Green


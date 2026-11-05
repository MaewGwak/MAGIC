# Compares each MAGIC benchmark's compiler-reported static memory bound with measured peak RSS.
# Exits 1 if a program's peak RSS is suspiciously below its reported bound.
$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$magicc = Join-Path $root 'target\release\magicc.exe'

$cases = @(
    @{ name = 'sieve 100M';   dir = 'bench-suite';    exe = 'sieve_magic.exe';    src = 'sieve_magic.magic' },
    @{ name = 'fannkuch';     dir = 'bench-fannkuch'; exe = 'fannkuch_magic.exe'; src = 'fannkuch_magic.magic' },
    @{ name = 'matmul 384';   dir = 'bench-suite';    exe = 'matmul_magic.exe';   src = 'matmul_magic.magic' },
    @{ name = 'n-body 10M';   dir = 'bench-suite';    exe = 'nbody_magic.exe';    src = 'nbody_magic.magic' },
    @{ name = 'ray tracer';   dir = 'bench-suite';    exe = 'raytrace_magic.exe'; src = 'raytrace_magic.magic' },
    @{ name = 'hash 1M';      dir = 'bench-suite';    exe = 'hash_magic.exe';     src = 'hash_magic.magic' },
    @{ name = 'binary trees'; dir = 'bench-bt';       exe = 'bt_magic.exe';       src = 'bt_magic.magic' }
)

function Peak-MB([string]$Exe) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = (Resolve-Path -LiteralPath $Exe).Path
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $p = [System.Diagnostics.Process]::Start($psi)
    $peak = 0
    while (-not $p.HasExited) {
        try { $p.Refresh(); if ($p.WorkingSet64 -gt $peak) { $peak = $p.WorkingSet64 } } catch {}
        Start-Sleep -Milliseconds 2
    }
    $null = $p.StandardOutput.ReadToEnd()
    [math]::Round($peak / 1MB, 1)
}

$rows = @()
$failed = 0
foreach ($c in $cases) {
    $dir = Join-Path $root $c.dir
    Push-Location $dir
    $report = & $magicc check $c.src --report 2>&1 | Out-String
    Pop-Location
    $m = [regex]::Match($report, 'main <= (\d+) bytes')
    if (-not $m.Success) {
        $rows += [pscustomobject]@{ Benchmark = $c.name; ReportedMB = 'n/a'; PeakMB = ''; OverheadMB = ''; OK = 'NO BOUND' }
        $failed++
        continue
    }
    $boundMB = [math]::Round([int64]$m.Groups[1].Value / 1MB, 1)
    $exe = Join-Path $dir $c.exe
    if (-not (Test-Path $exe)) {
        $rows += [pscustomobject]@{ Benchmark = $c.name; ReportedMB = $boundMB; PeakMB = 'missing'; OverheadMB = ''; OK = 'BUILD' }
        $failed++
        continue
    }
    $peak = Peak-MB $exe
    $ok = $peak -ge ($boundMB * 0.95)
    if (-not $ok) { $failed++ }
    $rows += [pscustomobject]@{
        Benchmark  = $c.name
        ReportedMB = $boundMB
        PeakMB     = $peak
        OverheadMB = [math]::Round($peak - $boundMB, 1)
        OK         = if ($ok) { 'ok' } else { 'PEAK < BOUND' }
    }
}

$rows | Format-Table -AutoSize | Out-String | Write-Host
if ($failed -gt 0) {
    Write-Host "$failed case(s) failed" -ForegroundColor Red
    exit 1
}
Write-Host 'every reported bound is <= measured peak RSS' -ForegroundColor Green

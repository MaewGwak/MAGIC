# Sieve benchmark: build, run 5x, report median time and peak RSS.
$ErrorActionPreference = 'Continue'
$root = $PSScriptRoot
$gccDir = (Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Recurse -Filter gcc.exe -ErrorAction SilentlyContinue | Select-Object -First 1).Directory.FullName
$env:PATH = "$env:USERPROFILE\.cargo\bin;$gccDir;C:\src\goose\build;$env:PATH"

$go = $null
foreach ($p in @('C:\Program Files\Go\bin\go.exe', "$env:LOCALAPPDATA\Programs\Go\bin\go.exe")) {
    if (Test-Path $p) { $go = $p; break }
}
if (-not $go) { $c = Get-Command go -ErrorAction SilentlyContinue; if ($c) { $go = $c.Source } }

function Build-Timed([string]$Exe, [string[]]$ArgList) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    & $Exe @ArgList 2>&1 | Out-Null
    $sw.Stop()
    [int]$sw.Elapsed.TotalMilliseconds
}

$builds = [ordered]@{}
$builds['C']     = Build-Timed 'gcc' @('-O2', '-o', 'sieve_c.exe', 'sieve_c.c')
$builds['C++']   = Build-Timed 'g++' @('-O2', '-std=c++17', '-o', 'sieve_cpp.exe', 'sieve_cpp.cpp')
$builds['Rust']  = Build-Timed 'rustc' @('-O', '-o', 'sieve_rs.exe', 'sieve_rs.rs')
if ($go) {
    $old = $env:GO111MODULE; $env:GO111MODULE = 'off'
    $builds['Go'] = Build-Timed $go @('build', '-o', 'sieve_go.exe', 'sieve_go.go')
    $env:GO111MODULE = $old
}
$builds['Goose'] = Build-Timed 'C:\src\goose\build\goose.exe' @('build', 'sieve_goose.goose')
$builds['magic']  = Build-Timed (Join-Path $root '..\target\release\magicc.exe') @('build', 'sieve_magic.magic')

function Run-Once([string]$Exe) {
    if (-not (Test-Path -LiteralPath $Exe)) { return $null }
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

$exeMap = [ordered]@{
    'C'     = 'sieve_c.exe'
    'C++'   = 'sieve_cpp.exe'
    'Rust'  = 'sieve_rs.exe'
    'Go'    = 'sieve_go.exe'
    'Goose' = 'sieve_goose.exe'
    'magic'  = 'sieve_magic.exe'
}

$rows = @()
foreach ($k in $exeMap.Keys) {
    $exe = $exeMap[$k]
    if (-not (Test-Path $exe)) { continue }
    $runs = @(1..5 | ForEach-Object { Run-Once ".\$exe" } | Where-Object { $_ -ne $null })
    $t = @($runs.Ms | Sort-Object)
    $rows += [pscustomobject]@{
        Language = $k
        BuildMs  = $builds[$k]
        MedianMs = $t[[int]([math]::Floor($t.Count / 2))]
        BestMs   = $t[0]
        PeakMB   = ($runs.PeakMB | Measure-Object -Maximum).Maximum
        Correct  = ($runs[0].Out -eq '664579')
    }
}

Write-Host ''
$rows | Format-Table -AutoSize | Out-String | Write-Host
if (-not $go) { Write-Host 'Go was not found - row omitted.' -ForegroundColor Yellow }

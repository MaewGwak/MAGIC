# Interleaved sieve benchmark: 15 rounds, all languages in each round.
# Interleaving averages out thermal/frequency drift and background load.
$ErrorActionPreference = 'Continue'
$root = $PSScriptRoot
$gccDir = (Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Recurse -Filter gcc.exe -ErrorAction SilentlyContinue | Select-Object -First 1).Directory.FullName
$env:PATH = "$env:USERPROFILE\.cargo\bin;$gccDir;C:\src\goose\build;C:\Program Files\Go\bin;$env:PATH"

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
$env:GO111MODULE = 'off'
$builds['Go']    = Build-Timed 'C:\Program Files\Go\bin\go.exe' @('build', '-o', 'sieve_go.exe', 'sieve_go.go')
$env:GO111MODULE = $null
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
        Start-Sleep -Milliseconds 1
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

$samples = @{}
foreach ($k in $exeMap.Keys) { $samples[$k] = New-Object System.Collections.Generic.List[object] }

for ($round = 0; $round -lt 15; $round++) {
    foreach ($k in $exeMap.Keys) {
        $exe = $exeMap[$k]
        if (-not (Test-Path $exe)) { continue }
        $r = Run-Once ".\$exe"
        if ($r) { $samples[$k].Add($r) }
    }
}

$rows = @()
foreach ($k in $exeMap.Keys) {
    $s = $samples[$k]
    if ($s.Count -eq 0) { continue }
    $t = @($s | ForEach-Object { $_.Ms } | Sort-Object)
    $rows += [pscustomobject]@{
        Language = $k
        BuildMs  = $builds[$k]
        MinMs    = $t[0]
        MedianMs = $t[[int]([math]::Floor($t.Count / 2))]
        PeakMB   = ($s | ForEach-Object { $_.PeakMB } | Measure-Object -Maximum).Maximum
        Correct  = ($s[0].Out -eq '664579')
        N        = $s.Count
    }
}

$rows | Format-Table -AutoSize | Out-String | Write-Host

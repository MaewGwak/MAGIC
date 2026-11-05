# Verifies that the numbers quoted in the docs match the repository.
# Exits 1 on any mismatch, so doc drift is caught mechanically instead of by hand.
$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Push-Location $root
try {
    $list = & cargo test --lib --test emit --test e2e --test cli -- --list 2>$null
    $tests = ($list | Where-Object { $_ -match ': test$' }).Count

    $examples = (Get-ChildItem 'examples\*.magic').Count

    $matrix = Get-Content 'TEST-MATRIX.md' -Raw
    $declaredTests = [int][regex]::Match($matrix, '(\d+) tests, all green').Groups[1].Value
    $declaredExamples = [int][regex]::Match($matrix, 'every one of the (\d+) examples').Groups[1].Value

    $status = Get-Content 'STATUS.md' -Raw
    $statusTests = [int][regex]::Match($status, '(\d+) tests, all green').Groups[1].Value

    $readme = Get-Content 'README.md' -Raw
    $readmeTests = [int][regex]::Match($readme, '(\d+) tests green').Groups[1].Value

    $bench = Get-Content 'BENCHMARKS.md' -Raw
    $workloads = ([regex]::Matches($bench, '(?m)^## \d+\.')).Count
    $wordToNum = @{ 'one'=1; 'two'=2; 'three'=3; 'four'=4; 'five'=5; 'six'=6; 'seven'=7; 'eight'=8; 'nine'=9; 'ten'=10 }
    $readmeWord = [regex]::Match($readme, '(\w+) benchmark workloads').Groups[1].Value.ToLower()
    $readmeWorkloads = if ($wordToNum.ContainsKey($readmeWord)) { $wordToNum[$readmeWord] } else { -1 }

    $cargo = Get-Content 'Cargo.toml' -Raw
    $cargoVersion = [regex]::Match($cargo, 'version = "([\d.]+)"').Groups[1].Value
    $specs = Get-Content 'SPECS.md' -Raw
    $specsVersion = [regex]::Match($specs, 'Language Specification v([\d.]+)').Groups[1].Value
    $readmeVersion = [regex]::Match($readme, 'Status: v([\d.]+)').Groups[1].Value
    $cargoMajorMinor = (($cargoVersion -split '\.')[0..1] -join '.')

    $rows = @(
        [pscustomobject]@{ Check = 'test count (TEST-MATRIX)'; Actual = $tests; Declared = $declaredTests },
        [pscustomobject]@{ Check = 'test count (STATUS)';      Actual = $tests; Declared = $statusTests },
        [pscustomobject]@{ Check = 'test count (README)';      Actual = $tests; Declared = $readmeTests },
        [pscustomobject]@{ Check = 'example count';            Actual = $examples; Declared = $declaredExamples },
        [pscustomobject]@{ Check = 'version (SPECS)';          Actual = $cargoVersion; Declared = $specsVersion },
        [pscustomobject]@{ Check = 'version (README)';         Actual = $cargoMajorMinor; Declared = $readmeVersion },
        [pscustomobject]@{ Check = 'workload count (README)';  Actual = $workloads; Declared = $readmeWorkloads }
    )
    $rows | Format-Table -AutoSize | Out-String | Write-Host

    $bad = @($rows | Where-Object { $_.Actual -ne $_.Declared })
    if ($bad.Count -gt 0) {
        Write-Host "$($bad.Count) doc count(s) out of date" -ForegroundColor Red
        exit 1
    }
    Write-Host 'docs match the repository' -ForegroundColor Green
}
finally {
    Pop-Location
}

# MAGIC installer (Windows).
#
#   powershell -ExecutionPolicy Bypass -File install.ps1
#   powershell -ExecutionPolicy Bypass -File install.ps1 -WithToolchain
#
# Installs to %LOCALAPPDATA%\MAGIC\bin and adds it to the user PATH.
# Run from a checkout, or from anywhere: it clones the repo when needed.

param(
    [string]$Prefix = "$env:LOCALAPPDATA\MAGIC",
    [switch]$WithToolchain,
    [switch]$NoPath
)

$ErrorActionPreference = 'Stop'
$repo = 'https://github.com/MaewGwak/MAGIC.git'

function Need([string]$name, [string]$hint) {
    if (-not (Get-Command $name -ErrorAction SilentlyContinue)) {
        throw "$name not found. $hint"
    }
}

# 1. Toolchain -------------------------------------------------------------
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    if ($WithToolchain) {
        if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
            throw "winget not found; install Rust from https://rustup.rs then rerun"
        }
        Write-Host "installing Rust (rustup) ..."
        winget install --id Rustlang.Rustup -e --accept-package-agreements --accept-source-agreements
    } else {
        throw "Rust not found. Install https://rustup.rs, or rerun with -WithToolchain"
    }
}

$cc = @('gcc', 'clang', 'cc') | Where-Object { Get-Command $_ -ErrorAction SilentlyContinue } | Select-Object -First 1
if (-not $cc) {
    if ($WithToolchain) {
        if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
            throw "winget not found; install a C compiler (gcc/clang) then rerun"
        }
        Write-Host "installing a C compiler (MinGW) ..."
        winget install --id BrechtSanders.WinLibs.POSIX.UCRT -e --accept-package-agreements --accept-source-agreements
        # winget updates the user PATH for new shells; add it to this one too.
        $mingw = Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Recurse -Filter gcc.exe -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if ($mingw) { $env:PATH = "$($mingw.Directory.FullName);$env:PATH" }
    } else {
        throw "No C compiler found. Install gcc or clang, or rerun with -WithToolchain"
    }
}

# 2. Source ----------------------------------------------------------------
$here = $PSScriptRoot
if ($here -and (Test-Path (Join-Path $here 'Cargo.toml'))) {
    $src = $here
} else {
    Need git 'Install Git from https://git-scm.com and rerun.'
    $tmp = Join-Path ([IO.Path]::GetTempPath()) ("magic-build-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
    New-Item -ItemType Directory -Path $tmp | Out-Null
    Write-Host "cloning $repo ..."
    git clone --depth 1 $repo (Join-Path $tmp 'magic')
    $src = Join-Path $tmp 'magic'
}

# 3. Build -----------------------------------------------------------------
Write-Host "building ..."
Push-Location $src
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally {
    Pop-Location
}

# 4. Install ---------------------------------------------------------------
$bin = Join-Path $Prefix 'bin'
New-Item -ItemType Directory -Path $bin -Force | Out-Null
$exe = if ($IsWindows -or $env:OS -eq 'Windows_NT') { 'magicc.exe' } else { 'magicc' }
$built = Join-Path $src "target\release\$exe"
if (-not (Test-Path $built)) { $built = Join-Path $src "target/release/$exe" }
Copy-Item -LiteralPath $built -Destination (Join-Path $bin $exe) -Force
# `magic` is the friendly name; `magicc` stays for scripts.
Copy-Item -LiteralPath (Join-Path $bin $exe) -Destination (Join-Path $bin 'magic.exe') -Force

# 5. PATH ------------------------------------------------------------------
if (-not $NoPath) {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (($userPath -split ';') -notcontains $bin) {
        [Environment]::SetEnvironmentVariable('Path', "$userPath;$bin", 'User')
        Write-Host "added $bin to the user PATH (open a new terminal)"
    }
}

# 6. Report ----------------------------------------------------------------
$magic = Join-Path $bin 'magic.exe'
& $magic --version
Write-Host "installed: $bin"
Write-Host "run: magic -v"

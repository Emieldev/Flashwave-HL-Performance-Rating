# Build Flashwave.tf from the GitHub source and run it.
#
#   powershell -ExecutionPolicy Bypass -File run-from-source.ps1
#
# Run it again later to pull the newest code and rebuild.
#
# What it needs (it checks, and says what is missing):
#   - git                  https://git-scm.com/download/win
#   - Node.js 20+          https://nodejs.org
#   - Rust                 https://rustup.rs
#   - Visual Studio Build Tools with "Desktop development with C++"
#                          https://visualstudio.microsoft.com/visual-cpp-build-tools/
#
# The first build takes 5-15 minutes and a few GB of disk. Later ones are
# much faster.
#
# IMPORTANT: this does NOT get around a Windows Defender "Attack surface
# reduction" block. A copy you build yourself is a brand-new executable that
# nobody else has run, which is exactly what that rule blocks -- and it can
# block the build itself, because compiling runs many small new programs.
# If Defender stopped the installer, it will stop this too.
#
# It uses the same database as the installed app. Close the installed app
# before running this one.

$ErrorActionPreference = "Stop"
$repo = "https://github.com/bartflk/Flashwave-HL-Performance-Rating.git"
$dir = Join-Path $env:USERPROFILE "Flashwave-src"

function Need($cmd, $what, $url) {
    if (-not (Get-Command $cmd -ErrorAction SilentlyContinue)) {
        Write-Host "Missing: $what. Install it from $url, then open a NEW PowerShell window and run this again." -ForegroundColor Red
        exit 1
    }
}

Write-Host "Checking what is installed..." -ForegroundColor Cyan
Need "git" "git" "https://git-scm.com/download/win"
Need "node" "Node.js" "https://nodejs.org"
Need "cargo" "Rust" "https://rustup.rs"

# The C++ build tools have no command on PATH; vswhere is how to find them.
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
$vc = $null
if (Test-Path $vswhere) {
    $vc = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
}
if (-not $vc) {
    Write-Host "Missing: Visual Studio Build Tools with 'Desktop development with C++'." -ForegroundColor Red
    Write-Host "Get it from https://visualstudio.microsoft.com/visual-cpp-build-tools/ and tick that workload." -ForegroundColor Red
    exit 1
}

if (Test-Path (Join-Path $dir ".git")) {
    Write-Host "Updating $dir ..." -ForegroundColor Cyan
    git -C $dir pull --ff-only
    if ($LASTEXITCODE -ne 0) { Write-Host "git pull failed (local changes?). Nothing was built." -ForegroundColor Red; exit 1 }
} else {
    Write-Host "Downloading the source to $dir ..." -ForegroundColor Cyan
    git clone $repo $dir
    if ($LASTEXITCODE -ne 0) { Write-Host "git clone failed." -ForegroundColor Red; exit 1 }
}

Push-Location $dir
try {
    # npm.cmd, not npm: PowerShell's default policy refuses npm.ps1.
    Write-Host "Installing packages..." -ForegroundColor Cyan
    & npm.cmd install
    if ($LASTEXITCODE -ne 0) { throw "npm install failed" }
    & npm.cmd --prefix ui install
    if ($LASTEXITCODE -ne 0) { throw "npm install (ui) failed" }

    # --no-bundle builds the app without the installer, which is also the step
    # that needs the release signing key only the maintainer has.
    Write-Host "Building (the first time takes a while)..." -ForegroundColor Cyan
    & npm.cmd run tauri -- build --no-bundle
    if ($LASTEXITCODE -ne 0) { throw "build failed -- if Defender showed a 'Risky action blocked' popup, that is the cause" }
} finally {
    Pop-Location
}

$exe = Join-Path $dir "target\release\hl-app.exe"
if (-not (Test-Path $exe)) {
    Write-Host "The build said it finished but $exe is not there." -ForegroundColor Red
    exit 1
}
Write-Host "Starting Flashwave.tf" -ForegroundColor Green
Start-Process $exe

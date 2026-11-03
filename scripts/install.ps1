# Builds, signs and installs otzar-mcp, then registers it in Claude Code (user scope).
# Usage: powershell -ExecutionPolicy Bypass -File scripts\install.ps1 [-Target C:\tools\otzar-mcp] [-NoRegister]
param(
    [string]$Target = 'C:\tools\otzar-mcp',
    [switch]$NoRegister
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

    $exe = Join-Path $repo 'target\release\otzar-mcp.exe'
    & (Join-Path $PSScriptRoot 'sign.ps1') $exe

    New-Item -ItemType Directory -Force $Target | Out-Null
    # A running Claude session keeps the old exe locked; ask to close it instead of failing obscurely.
    try { Copy-Item $exe (Join-Path $Target 'otzar-mcp.exe') -Force }
    catch { throw "Cannot replace $Target\otzar-mcp.exe. Close Claude sessions that use it and retry." }

    $example = Join-Path $Target 'otzar-mcp.example.toml'
    Copy-Item (Join-Path $repo 'otzar-mcp.example.toml') $example -Force

    if (-not $NoRegister) {
        $existing = claude mcp list 2>$null | Select-String -Pattern '^otzar:' -Quiet
        if (-not $existing) {
            claude mcp add otzar --scope user -- (Join-Path $Target 'otzar-mcp.exe')
        }
    }
    Write-Host "Installed to $Target"
}
finally { Pop-Location }

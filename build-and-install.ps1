# Cloud Drive Mount - Build and Install
# Builds the WiX bootstrapper installer with a chosen version, then launches it.
#
# Usage:
#   .\build-and-install.ps1 -Version "0.0.7"

param(
    [Parameter(Mandatory=$true, HelpMessage="Version number, e.g. 0.0.7")]
    [string]$Version
)

$ErrorActionPreference = "Stop"
$repoRoot = $PSScriptRoot
if (-not $repoRoot) { $repoRoot = (Get-Location).Path }

function Write-Header($text) {
    Write-Host ""
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host "  $text" -ForegroundColor Cyan
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host ""
}

if ($Version -notmatch '^\d+(\.\d+){0,3}$') {
    Write-Warning "Version '$Version' doesn't look like a standard version number."
    $confirm = Read-Host "Continue anyway? (y/n)"
    if ($confirm -ne 'y') { exit 1 }
}

Write-Header "Cloud Drive Mount Builder"
Write-Host "Building version: $Version" -ForegroundColor Yellow

Write-Header "Building Installer"
$buildInstaller = Join-Path $repoRoot 'build-installer.ps1'
& $buildInstaller -Version $Version -Configuration Release

if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$installerPath = Join-Path $repoRoot 'installer\bin\Release\CloudDriveMount.bootstrapper.exe'
if (-not (Test-Path $installerPath)) {
    throw "Installer not found after build: $installerPath"
}

$installerSize = [math]::Round((Get-Item $installerPath).Length / 1MB, 2)
Write-Host "Installer built successfully." -ForegroundColor Green
Write-Host "  Path: $installerPath" -ForegroundColor Gray
Write-Host "  Size: $installerSize MB" -ForegroundColor Gray

Write-Header "Launching Installer"
Start-Process -FilePath $installerPath

Write-Header "Done"
Write-Host "Installer launched. Cloud Drive Mount v$Version setup is now running." -ForegroundColor Green

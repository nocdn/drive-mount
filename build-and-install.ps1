# Cloud Drive Mount - Build and Install
# Builds the WiX MSI installer with a chosen version, then launches it.
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

Write-Header "Building MSI"
$buildInstaller = Join-Path $repoRoot 'build-installer.ps1'
& $buildInstaller -Version $Version -Configuration Release

if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$msiPath = Join-Path $repoRoot 'installer\bin\Release\CloudDriveMount.installer.msi'
if (-not (Test-Path $msiPath)) {
    throw "MSI not found after build: $msiPath"
}

$msiSize = [math]::Round((Get-Item $msiPath).Length / 1MB, 2)
Write-Host "MSI built successfully." -ForegroundColor Green
Write-Host "  Path: $msiPath" -ForegroundColor Gray
Write-Host "  Size: $msiSize MB" -ForegroundColor Gray

Write-Header "Launching Installer"
Start-Process -FilePath $msiPath

Write-Header "Done"
Write-Host "Installer launched. Cloud Drive Mount v$Version setup is now running." -ForegroundColor Green

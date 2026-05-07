# Download latest WinFsp MSI and place it in the repo root for bundling into the installer.
# Run this from the repo root before publishing.

$ErrorActionPreference = "Stop"

$repoRoot = $PSScriptRoot
$destFile = Join-Path $repoRoot "winfsp.msi"

if (Test-Path $destFile) {
    Write-Host "winfsp.msi already exists at $destFile. Delete it first if you want to re-download."
    exit 0
}

$apiUrl = "https://api.github.com/repos/winfsp/winfsp/releases/latest"
Write-Host "Fetching latest WinFsp release info..."
$release = Invoke-RestMethod -Uri $apiUrl -Headers @{ "User-Agent" = "PowerShell" }

$asset = $release.assets | Where-Object { $_.name -match "\.msi$" } | Select-Object -First 1
if (-not $asset) {
    throw "Could not find MSI asset in latest WinFsp release."
}

Write-Host "Downloading $($asset.name)..."
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $destFile -UseBasicParsing
Write-Host "winfsp.msi saved to $destFile ($([math]::Round((Get-Item $destFile).Length / 1MB, 2)) MB)"

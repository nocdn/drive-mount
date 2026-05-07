# Download latest rclone Windows 64-bit binary and place it next to the project for self-contained distribution.
# Run this from the repo root before publishing.

$ErrorActionPreference = "Stop"

$projectDir = Join-Path $PSScriptRoot "B2DriveMount"
$destFile = Join-Path $projectDir "rclone.exe"

if (Test-Path $destFile) {
    Write-Host "rclone.exe already exists at $destFile. Delete it first if you want to re-download."
    exit 0
}

$apiUrl = "https://api.github.com/repos/rclone/rclone/releases/latest"
Write-Host "Fetching latest rclone release info..."
$release = Invoke-RestMethod -Uri $apiUrl -Headers @{ "User-Agent" = "PowerShell" }

# Find the windows-amd64 zip asset
$asset = $release.assets | Where-Object { $_.name -match "windows-amd64.*\.zip$" } | Select-Object -First 1
if (-not $asset) {
    throw "Could not find windows-amd64 zip asset in latest rclone release."
}

$zipUrl = $asset.browser_download_url
$zipPath = Join-Path $env:TEMP "rclone-windows-amd64.zip"

Write-Host "Downloading $($asset.name)..."
Invoke-WebRequest -Uri $zipUrl -OutFile $zipPath -UseBasicParsing

Write-Host "Extracting..."
$extractDir = Join-Path $env:TEMP "rclone-extract"
if (Test-Path $extractDir) { Remove-Item -Recurse -Force $extractDir }
Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force

# Find rclone.exe inside extracted folder
$found = Get-ChildItem -Path $extractDir -Recurse -Filter "rclone.exe" | Select-Object -First 1
if (-not $found) {
    throw "rclone.exe not found inside downloaded archive."
}

Copy-Item -Path $found.FullName -Destination $destFile -Force
Write-Host "rclone.exe saved to $destFile ($([math]::Round((Get-Item $destFile).Length / 1MB, 2)) MB)"

# Cleanup
Remove-Item -Recurse -Force $extractDir -ErrorAction SilentlyContinue
Remove-Item -Force $zipPath -ErrorAction SilentlyContinue

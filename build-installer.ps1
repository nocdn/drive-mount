param(
    [Parameter(Mandatory = $true)]
    [string]$Version,

    [ValidateSet('Release', 'Debug')]
    [string]$Configuration = 'Release',

    [switch]$OpenInstaller
)

$dotnetCandidates = @()

if ($env:DOTNET_ROOT) {
    $dotnetCandidates += (Join-Path $env:DOTNET_ROOT 'dotnet.exe')
}

$dotnetCandidates += @(
    'C:\Program Files\dotnet\dotnet.exe',
    'dotnet'
)

$dotnetCommand = $null

foreach ($candidate in $dotnetCandidates) {
    if ($candidate -eq 'dotnet') {
        $command = Get-Command dotnet -ErrorAction SilentlyContinue
        if ($command) {
            $dotnetCommand = $command.Source
            break
        }

        continue
    }

    if (Test-Path $candidate) {
        $dotnetCommand = $candidate
        break
    }
}

if (-not $dotnetCommand) {
    throw 'dotnet.exe was not found. Install the .NET SDK or add dotnet to PATH.'
}

$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$installerProject = Join-Path $repoRoot 'installer\CloudDriveMount.installer.wixproj'

Write-Host "Building Cloud Drive Mount MSI with PackageVersion=$Version..."
& $dotnetCommand build $installerProject -c $Configuration "-p:PackageVersion=$Version"

if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$msiPath = Join-Path $repoRoot "installer\bin\$Configuration\CloudDriveMount.installer.msi"
Write-Host "Built MSI: $msiPath"

if ($OpenInstaller) {
    Start-Process $msiPath
}

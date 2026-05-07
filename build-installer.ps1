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
$bootstrapperProject = Join-Path $repoRoot 'installer\CloudDriveMount.bootstrapper.wixproj'

Write-Host "Building Cloud Drive Mount installer with PackageVersion=$Version..."
& $dotnetCommand build $bootstrapperProject -c $Configuration "-p:PackageVersion=$Version"

if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$installerPath = Join-Path $repoRoot "installer\bin\$Configuration\CloudDriveMount.bootstrapper.exe"
Write-Host "Built installer: $installerPath"

if ($OpenInstaller) {
    Start-Process $installerPath
}

[CmdletBinding()]
param([switch]$DebugBuild, [switch]$Install, [string]$InstallDir, [switch]$NoPath, [switch]$NoCompatCopy)
$ErrorActionPreference = 'Stop'
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    $options = @('build','--workspace','--locked')
    $profile = if ($DebugBuild) { 'debug' } else { 'release' }
    if (-not $DebugBuild) { $options += '--release' }
    cargo @options
    if ($LASTEXITCODE -ne 0) { throw 'Cargo build failed; install Rust and its platform build prerequisites.' }
    $metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Could not locate Cargo output' }
    $binaryDirectory = Join-Path $metadata.target_directory $profile
    if ($Install) {
        $installOptions = @{BinaryDirectory=$binaryDirectory;NoPath=$NoPath;NoCompatCopy=$NoCompatCopy}
        if ($InstallDir) { $installOptions.InstallDir=$InstallDir }
        & (Join-Path $PSScriptRoot 'install.ps1') @installOptions
    } else { Write-Host "Built: $(Join-Path $binaryDirectory 'cpuv.exe')" }
} finally { Pop-Location }

[CmdletBinding()]
param([switch]$SkipBuild, [switch]$DebugBuild, [string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
Push-Location $repo
try {
    $hostInfo = rustc -vV
    if ($LASTEXITCODE -ne 0 -or -not ($hostInfo -match '^host: x86_64-pc-windows-msvc$')) {
        throw 'Standalone packaging currently requires the x86_64-pc-windows-msvc Rust host toolchain.'
    }
    if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repo 'dist' }
    $output = [IO.Path]::GetFullPath($OutputDirectory)
    New-Item -ItemType Directory -Force -Path $output | Out-Null
    if (-not $SkipBuild) { & (Join-Path $PSScriptRoot 'build.ps1') -DebugBuild:$DebugBuild }
    $metadata = cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Could not read package metadata' }
    $profile = if ($DebugBuild) { 'debug' } else { 'release' }
    $binary = Join-Path $metadata.target_directory ($profile + '\cpuv.exe')
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw 'Build cpuv before packaging' }
    $version = (& $binary --version).Trim()
    if ($LASTEXITCODE -ne 0 -or $version -notmatch '^cpuv (\d+\.\d+\.\d+)$') { throw 'Invalid executable version' }
    $version = $Matches[1]
    $expectedVersion = ($metadata.packages | Where-Object name -eq 'cpuv-cli').version
    if ($version -ne $expectedVersion) { throw 'Executable version differs from source metadata' }
    $setup = Join-Path $output 'cpuv-setup.exe'
    Copy-Item -LiteralPath $binary -Destination $setup -Force
    $setupHash = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText(($setup + '.sha256'),($setupHash + '  cpuv-setup.exe' + "`n"),[Text.Encoding]::ASCII)
    Copy-Item -LiteralPath (Join-Path $repo 'install.ps1') -Destination (Join-Path $output 'install.ps1') -Force
    $staging = Join-Path ([IO.Path]::GetTempPath()) ('cpuv-package-' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $staging | Out-Null
    try {
        Copy-Item -LiteralPath $binary -Destination (Join-Path $staging 'cpuv.exe')
        foreach ($name in @('LICENSE','NOTICE','README.md','install.ps1')) { Copy-Item -LiteralPath (Join-Path $repo $name) -Destination $staging }
        [IO.File]::WriteAllText((Join-Path $staging 'install.cmd'),"@echo off`r`n`"%~dp0cpuv.exe`" self install %*`r`nif errorlevel 1 pause`r`n",[Text.Encoding]::ASCII)
        $zip = Join-Path $output 'cpuv-x86_64-pc-windows-msvc.zip'
        Compress-Archive -Path (Join-Path $staging '*') -DestinationPath $zip -Force
        $zipHash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
        [IO.File]::WriteAllText(($zip + '.sha256'),($zipHash + '  ' + (Split-Path $zip -Leaf) + "`n"),[Text.Encoding]::ASCII)
    } finally {
        Get-ChildItem -LiteralPath $staging -File | Remove-Item -Force
        Remove-Item -LiteralPath $staging -Force
    }
    Write-Host "Packaged cpuv $version in $output"
} finally { Pop-Location }

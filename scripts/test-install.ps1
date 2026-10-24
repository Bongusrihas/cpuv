param([string]$StaleBinary = (Join-Path $PSScriptRoot '..\target\debug\cpuv.exe'))
$ErrorActionPreference = 'Stop'
$StaleBinary = (Resolve-Path -LiteralPath $StaleBinary).Path
$sourceInstaller = Join-Path $PSScriptRoot 'install.ps1'
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('cpuv install gate ' + [guid]::NewGuid().ToString('N'))
$project = Join-Path $fixture 'project'
$customTarget = Join-Path $fixture 'custom cargo target'
$destination = Join-Path $fixture 'isolated installation'
New-Item -ItemType Directory -Path (Join-Path $project 'src'),(Join-Path $project 'scripts'),(Join-Path $project 'target\release') | Out-Null
@'
[package]
name = "cpuv-install-fixture"
version = "9.8.7"
edition = "2021"
[[bin]]
name = "cpuv"
path = "src/main.rs"
'@ | Set-Content -LiteralPath (Join-Path $project 'Cargo.toml') -Encoding ascii
'fn main() { println!("cpuv 9.8.7"); }' | Set-Content -LiteralPath (Join-Path $project 'src\main.rs') -Encoding ascii
Copy-Item -LiteralPath $sourceInstaller -Destination (Join-Path $project 'scripts\install.ps1')
Copy-Item -LiteralPath $StaleBinary -Destination (Join-Path $project 'target\release\cpuv.exe')
$previousTarget = $env:CARGO_TARGET_DIR
$previousPath = $env:PATH
$previousUserPath = [Environment]::GetEnvironmentVariable('Path','User')
Push-Location -LiteralPath $project
try {
    $env:CARGO_TARGET_DIR = $customTarget
    cargo build --release --offline
    if ($LASTEXITCODE -ne 0) { throw 'Isolated Cargo fixture did not build' }
    & (Join-Path $project 'scripts\install.ps1') -InstallDir $destination -NoPath -NoCompatCopy
    $expected = Join-Path $customTarget 'release\cpuv.exe'
    $installed = Join-Path $destination 'cpuv.exe'
    if ((Get-FileHash -LiteralPath $expected).Hash -ne (Get-FileHash -LiteralPath $installed).Hash) { throw 'Installer selected the stale default Cargo target executable' }
    $version = & $installed --version
    if ($LASTEXITCODE -ne 0 -or $version -ne 'cpuv 9.8.7') { throw 'Wrong executable installed' }
    if ($env:PATH -cne $previousPath -or [Environment]::GetEnvironmentVariable('Path','User') -cne $previousUserPath) { throw '-NoPath changed PATH' }
    $receipt = [ordered]@{ test='F16 custom Cargo target'; result='passed'; custom_target=$customTarget; stale_binary=$StaleBinary; installed_sha256=(Get-FileHash -LiteralPath $installed).Hash; user_path_changed=$false }
    $receipt | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $fixture 'evidence.json') -Encoding utf8
    Write-Output "PASS: custom-target installation ignored the stale executable; fixture: $fixture"
} finally {
    $env:CARGO_TARGET_DIR = $previousTarget
    Pop-Location
}

param(
    [Parameter(Mandatory = $true)][string]$Compiler,
    [string]$Binary = (Join-Path $PSScriptRoot '..\target\debug\cpuv.exe')
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$Compiler = (Resolve-Path -LiteralPath $Compiler).Path
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('cpuv Conan phase1 ' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture | Out-Null
function Invoke-Cpuv {
    & $Binary --project $fixture @args
    if ($LASTEXITCODE -ne 0) { throw "Conan Phase 1 command failed ($LASTEXITCODE); fixture: $fixture" }
}
function Reject-Cpuv {
    & $Binary --project $fixture @args
    if ($LASTEXITCODE -eq 0) { throw "Expected rejected command: $args" }
}
Invoke-Cpuv --compiler $Compiler --provider conan init
Reject-Cpuv --offline sync
Invoke-Cpuv add fmt/11.2.0
@'
#include <fmt/format.h>
int main(){fmt::print("Conan Phase 1 consumer passed\n");return 0;}
'@ | Set-Content -LiteralPath (Join-Path $fixture 'main.cpp') -Encoding ascii
$tc = Get-Content -LiteralPath (Join-Path $fixture '.cpuv\toolchain.json') -Raw | ConvertFrom-Json
$driver = if ($tc.driver -eq 'msvc') { 'cl' } else { 'g++' }
$manifestPath = Join-Path $fixture 'cpuv.toml'
$lockPath = Join-Path $fixture 'cpuv.lock'
$text = [IO.File]::ReadAllText($manifestPath)
$text = $text.Replace('reference = "fmt/11.2.0"', ('reference = "fmt/11.2.0"' + "`n" + 'linkage = "shared"'))
[IO.File]::WriteAllText($manifestPath,$text,[Text.UTF8Encoding]::new($false))
$lockBefore = (Get-FileHash -LiteralPath $lockPath).Hash
Reject-Cpuv --offline sync
if ((Get-FileHash -LiteralPath $lockPath).Hash -ne $lockBefore) { throw 'Rejected changed manifest replaced its lock' }
Invoke-Cpuv lock
$envDir = Join-Path $fixture '.cpuv\env'
if (-not (Get-ChildItem -LiteralPath (Join-Path $envDir 'bin') -File | Where-Object Extension -ieq '.dll')) { throw 'Shared linkage produced no runtime DLL' }
Invoke-Cpuv run $driver main.cpp
$lockBefore = (Get-FileHash -LiteralPath $lockPath).Hash
Invoke-Cpuv --offline --refresh sync
if ((Get-FileHash -LiteralPath $lockPath).Hash -ne $lockBefore) { throw 'Frozen restore changed the Conan lock' }
# Damaged generated metadata must be repaired from the complete provider cache.
[IO.File]::WriteAllText((Join-Path $envDir 'manifest.json'),'{corrupt')
Invoke-Cpuv --offline sync
Invoke-Cpuv verify
Invoke-Cpuv run $driver main.cpp
$text = [IO.File]::ReadAllText($manifestPath).Replace('linkage = "shared"','linkage = "static"')
[IO.File]::WriteAllText($manifestPath,$text,[Text.UTF8Encoding]::new($false))
Reject-Cpuv --offline sync
Invoke-Cpuv lock
Invoke-Cpuv verify
Invoke-Cpuv run $driver main.cpp
# Missing upstream recipes must fail before compilation and preserve a usable environment.
# A synthetic name avoids making the test depend on one real library remaining unpublished.
$missingPackage = 'cpuv-missing-fixture-' + [guid]::NewGuid().ToString('N')
$preservedFiles = @($manifestPath, $lockPath, (Join-Path $envDir 'manifest.json'))
$beforeMissing = @($preservedFiles | ForEach-Object { (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash })
$previousErrorAction = $ErrorActionPreference
try {
    $ErrorActionPreference = 'Continue'
    $missingOutput = & $Binary --project $fixture add $missingPackage 2>&1
    $missingExit = $LASTEXITCODE
} finally { $ErrorActionPreference = $previousErrorAction }
$missingOutput | Write-Output
if ($missingExit -eq 0) { throw 'Missing catalog recipe was unexpectedly installed' }
$missingText = $missingOutput -join "`n"
if (-not $missingText.Contains($missingPackage) -or -not $missingText.Contains('Conan could not resolve this package name/version')) {
    throw 'Missing catalog recipe did not report the resolution-stage failure'
}
$afterMissing = @($preservedFiles | ForEach-Object { (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash })
if (Compare-Object $beforeMissing $afterMissing) { throw 'Rejected missing recipe changed the manifest, lock or environment receipt' }
Invoke-Cpuv verify
Invoke-Cpuv run $driver main.cpp
$previousFlags = $env:CXXFLAGS
$lockBefore = (Get-FileHash -LiteralPath $lockPath).Hash
try {
    $env:CXXFLAGS = '-DCPUV_CHANGED_PROFILE=1'
    Reject-Cpuv --offline sync
} finally { $env:CXXFLAGS = $previousFlags }
if ((Get-FileHash -LiteralPath $lockPath).Hash -ne $lockBefore) { throw 'Rejected changed flags replaced the lock' }
Invoke-Cpuv --offline sync
Invoke-Cpuv remove fmt
Invoke-Cpuv verify
Write-Output "PASS: Conan Phase 1 shared/static, missing-recipe preservation, frozen/profile/offline/repair contracts; fixture: $fixture"

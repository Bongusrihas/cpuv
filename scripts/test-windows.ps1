param(
    [Parameter(Mandatory = $true)][string]$Compiler,
    [string]$Binary = (Join-Path $PSScriptRoot '..\target\debug\cpuv.exe'),
    [switch]$Catalog,
    [switch]$CMake,
    [switch]$Bundle
)
$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$Compiler = (Resolve-Path -LiteralPath $Compiler).Path
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('cpuv integration ' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture | Out-Null
function Invoke-Cpuv {
    & $Binary --project $fixture @args
    if ($LASTEXITCODE -ne 0) { throw "cpuv failed ($LASTEXITCODE); fixture retained at $fixture" }
}
Invoke-Cpuv --compiler $Compiler init
$toolchain = Get-Content -LiteralPath (Join-Path $fixture '.cpuv\toolchain.json') -Raw | ConvertFrom-Json
$driver = if ($toolchain.driver -eq 'msvc') { 'cl' } else { 'g++' }
@'
#include <iostream>
int main(int argc, char**) { std::cout << "cpuv native integration passed\n"; return argc == 2 ? 0 : 7; }
'@ | Set-Content -LiteralPath (Join-Path $fixture 'main file.cpp') -Encoding ascii
Invoke-Cpuv run $driver 'main file.cpp' '--' program-argument
'"main file.cpp"' | Set-Content -LiteralPath (Join-Path $fixture 'input.rsp') -Encoding ascii
Invoke-Cpuv run $driver '@input.rsp' '--' program-argument
if ($toolchain.driver -eq 'msvc') {
    Invoke-Cpuv run --no-run $driver /c 'main file.cpp' /Foseparate.obj
    Invoke-Cpuv run $driver separate.obj '--' program-argument
} else {
    Invoke-Cpuv run --no-run $driver -c 'main file.cpp' -o separate.o
    Invoke-Cpuv run $driver separate.o '--' program-argument
}
Invoke-Cpuv verify
$shimName = if ($toolchain.driver -eq 'msvc') { '.cpuv\bin\cl.exe' } else { '.cpuv\bin\g++.exe' }
$shim = Join-Path $fixture $shimName
$shimOutput = Join-Path $fixture '.cpuv\build\direct.exe'
Push-Location -LiteralPath $fixture
try {
    if ($toolchain.driver -eq 'msvc') {
        & $shim 'main file.cpp' "/Fe$shimOutput" ("/Fo" + (Join-Path $fixture '.cpuv\build\direct.obj'))
    } else { & $shim 'main file.cpp' '-o' $shimOutput }
    if ($LASTEXITCODE -ne 0) { throw 'Compiler shim failed' }
    & $shimOutput argument
    if ($LASTEXITCODE -ne 0) { throw 'Shim-built executable failed' }
    $manifestPath = Join-Path $fixture 'cpuv.toml'
    $originalRequest = [IO.File]::ReadAllText($manifestPath)
    try {
        [IO.File]::WriteAllText($manifestPath, ($originalRequest + "`n[link]`nlibraries = [`"should-require-sync`"]`n"), [Text.UTF8Encoding]::new($false))
        & $shim '--version'
        if ($LASTEXITCODE -eq 0) { throw 'Shim accepted a changed manifest' }
    } finally { [IO.File]::WriteAllText($manifestPath, $originalRequest, [Text.UTF8Encoding]::new($false)) }
} finally { Pop-Location }
if (Test-Path -LiteralPath (Join-Path $fixture 'main file.exe')) { throw 'Compiler output escaped the build directory' }
# A failed compile must not execute the successful binary left by an earlier run.
'#error deliberate integration failure' | Set-Content -LiteralPath (Join-Path $fixture 'broken.cpp') -Encoding ascii
& $Binary --project $fixture run $driver broken.cpp
if ($LASTEXITCODE -eq 0) { throw 'Failed compilation incorrectly succeeded' }
if ($Catalog) {
    Invoke-Cpuv add fmt/11.2.0
    $restored = Get-Content -LiteralPath (Join-Path $fixture ".cpuv\toolchain.json") -Raw | ConvertFrom-Json
    if ($restored.compiler_path -ne $toolchain.compiler_path) { throw "Dependency resolution changed the selected compiler" }
    @'
#include <fmt/format.h>
int main() { fmt::print("cpuv catalog integration passed\n"); }
'@ | Set-Content -LiteralPath (Join-Path $fixture 'library.cpp') -Encoding ascii
    Invoke-Cpuv run $driver library.cpp
    $lockPath = Join-Path $fixture 'cpuv.lock'
    $lockBefore = (Get-FileHash -LiteralPath $lockPath -Algorithm SHA256).Hash
    $previousFlags = $env:CXXFLAGS
    try {
        $env:CXXFLAGS = "$previousFlags -DCPUV_FROZEN_PROFILE_CHECK=1"
        & $Binary --project $fixture --offline sync
        if ($LASTEXITCODE -eq 0) { throw 'Frozen sync accepted changed build flags' }
    } finally { $env:CXXFLAGS = $previousFlags }
    if ($lockBefore -ne (Get-FileHash -LiteralPath $lockPath -Algorithm SHA256).Hash) { throw 'Rejected sync changed its lock' }
    Invoke-Cpuv --offline sync
    Invoke-Cpuv verify
    Invoke-Cpuv run $driver library.cpp
}
if ($CMake) {
    @'
cmake_minimum_required(VERSION 3.20)
project(cpuv_integration LANGUAGES C CXX)
enable_testing()
add_executable(consumer "main file.cpp")
add_test(NAME consumer COMMAND consumer argument)
'@ | Set-Content -LiteralPath (Join-Path $fixture 'CMakeLists.txt') -Encoding ascii
    Invoke-Cpuv test
    if (-not (Test-Path -LiteralPath (Join-Path $fixture '.cpuv\build\cmake\compile_commands.json'))) { throw 'Missing compile commands' }
}
if ($Bundle) {
    $entry = if ($Catalog) { 'library.exe' } else { 'main file.exe' }
    $output = Join-Path $fixture 'application'
    Invoke-Cpuv bundle $entry $output
    $previousPath = $env:PATH
    try {
        $env:PATH = Join-Path $env:SystemRoot 'System32'
        if ($Catalog) { & (Join-Path $output $entry) } else { & (Join-Path $output $entry) argument }
        if ($LASTEXITCODE -ne 0) { throw 'Exported application failed with a minimal PATH' }
    } finally { $env:PATH = $previousPath }
}
Write-Output "PASS: $($toolchain.abi.compiler_family) $($toolchain.gcc_version), $($toolchain.abi.arch); fixture: $fixture"

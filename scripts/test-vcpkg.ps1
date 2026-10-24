param(
    [Parameter(Mandatory = $true)][string]$Compiler,
    [string]$Binary = (Join-Path $PSScriptRoot '..\target\debug\cpuv.exe')
)
$ErrorActionPreference = 'Stop'
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$Compiler = (Resolve-Path -LiteralPath $Compiler).Path
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('cpuv vcpkg integration ' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture | Out-Null
Write-Output "Fixture: $fixture"
function Invoke-Cpuv {
    & $Binary --project $fixture @args
    if ($LASTEXITCODE -ne 0) { throw "cpuv failed ($LASTEXITCODE); fixture retained at $fixture" }
}
Invoke-Cpuv --compiler $Compiler --provider vcpkg init
Invoke-Cpuv add zlib
@'
#include <zlib.h>
#include <iostream>
int main() { std::cout << zlibVersion() << std::endl; return 0; }
'@ | Set-Content -LiteralPath (Join-Path $fixture 'main.cpp') -Encoding ascii
@'
cmake_minimum_required(VERSION 3.20)
project(cpuv_vcpkg_consumer LANGUAGES C CXX)
enable_testing()
find_package(ZLIB REQUIRED)
add_executable(consumer main.cpp)
target_link_libraries(consumer PRIVATE ZLIB::ZLIB)
add_test(NAME consumer COMMAND consumer)
'@ | Set-Content -LiteralPath (Join-Path $fixture 'CMakeLists.txt') -Encoding ascii
Invoke-Cpuv test
$lockBefore = (Get-FileHash -LiteralPath (Join-Path $fixture 'cpuv.lock') -Algorithm SHA256).Hash
Invoke-Cpuv sync
Invoke-Cpuv --offline sync
$lockAfter = (Get-FileHash -LiteralPath (Join-Path $fixture 'cpuv.lock') -Algorithm SHA256).Hash
if ($lockBefore -ne $lockAfter) { throw 'Locked restore changed the graph' }
Invoke-Cpuv verify
Invoke-Cpuv test
$destination = Join-Path $fixture 'application'
Invoke-Cpuv bundle 'cmake/consumer.exe' $destination
$previousPath = $env:PATH
try {
    $env:PATH = Join-Path $env:SystemRoot 'System32'
    & (Join-Path $destination 'consumer.exe')
    if ($LASTEXITCODE -ne 0) { throw 'Exported vcpkg consumer failed with a minimal PATH' }
} finally { $env:PATH = $previousPath }
Write-Output "PASS: vcpkg restore, locked graph, CMake/CTest and deployment; fixture: $fixture"

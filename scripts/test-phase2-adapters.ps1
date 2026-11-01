param(
    [Parameter(Mandatory=$true)][string]$Compiler,
    [Parameter(Mandatory=$true)][string]$MsvcCompiler,
    [string]$Binary=(Join-Path $PSScriptRoot '../target/debug/cpuv.exe')
)
$ErrorActionPreference='Stop'
$Binary=(Resolve-Path -LiteralPath $Binary).Path
$fixture=Join-Path ([IO.Path]::GetTempPath()) ('cpuv phase2 adapters '+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture | Out-Null
function Invoke-Cpuv {
    param([string]$Project,[Parameter(ValueFromRemainingArguments=$true)][string[]]$Arguments)
    & $Binary --project $Project @Arguments
    if ($LASTEXITCODE -ne 0) {throw "Phase 2 adapter failed; fixture: $fixture"}
}
$meson=Join-Path $fixture 'meson'
Invoke-Cpuv $meson --compiler $Compiler init
@'
#include <cstdlib>
int main(){return std::getenv("CPUV_ADAPTER_FIXTURE")?0:1;}
'@ | Set-Content -LiteralPath (Join-Path $meson 'main.cpp') -Encoding ascii
@'
project('consumer','cpp')
app=executable('consumer','main.cpp')
test('consumer',app)
'@ | Set-Content -LiteralPath (Join-Path $meson 'meson.build') -Encoding ascii
@'
[build]
system="meson"
[runtime.environment]
CPUV_ADAPTER_FIXTURE="verified"
[tools.python]
version="3.13.18"
executable="python.exe"
url="https://github.com/astral-sh/python-build-standalone/releases/download/20261026/cpython-3.13.18%2B20261026-x86_64-pc-windows-msvc-install_only_stripped.tar.gz"
sha256="7369f7a35f261222a6ac8b3a87c1c88eeebee8af945d85346202591a64038dd3"
archive="tar.gz"
strip_components=1
[tools.meson]
version="1.12.1"
executable="meson.py"
interpreter="python"
conan_package="meson"
url="https://files.pythonhosted.org/packages/f9/c9/8c9983f4f3d9c4e22fd76bcf8cd053e4472aefae45fd044669b6daa34b53/meson-1.12.1.tar.gz"
sha256="ab0a6ca09f8ef70c564c8241fb5a23957886a0b53fb58412b5e07eaf07dba743"
archive="tar.gz"
strip_components=1
'@ | Add-Content -LiteralPath (Join-Path $meson 'cpuv.toml') -Encoding ascii
$vendor=Join-Path $meson 'vendor'
New-Item -ItemType Directory -Path $vendor | Out-Null
"project('meson_fixture','cpp')`nlibrary('meson_fixture','value.cpp',install:true)`ninstall_headers('value.h')" | Set-Content -LiteralPath (Join-Path $vendor 'meson.build') -Encoding ascii
'int meson_fixture_value(){return 71;}' | Set-Content -LiteralPath (Join-Path $vendor 'value.cpp') -Encoding ascii
'int meson_fixture_value();' | Set-Content -LiteralPath (Join-Path $vendor 'value.h') -Encoding ascii
$snapshot=& $Binary --project $meson source inspect $vendor | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) {throw 'Meson source snapshot failed'}
$spec=Join-Path $meson 'source-package.toml'
@"
version="1.0"
[source.input]
kind="local"
path="vendor"
sha256="$($snapshot.sha256)"
[source.build]
system="meson"
tools=["meson/1.12.1"]
[source.interface]
libraries=["meson_fixture"]
"@ | Set-Content -LiteralPath $spec -Encoding ascii
Invoke-Cpuv $meson source add meson-fixture $spec
"#include <cstdlib>`n#include <value.h>`nint main(){return std::getenv(`"CPUV_ADAPTER_FIXTURE`") && meson_fixture_value()==71?0:1;}" | Set-Content -LiteralPath (Join-Path $meson 'main.cpp') -Encoding ascii
Invoke-Cpuv $meson test
Invoke-Cpuv $meson --offline test

$msbuild=Join-Path $fixture 'msbuild'
Invoke-Cpuv $msbuild --compiler $MsvcCompiler init
@'
#if _MSVC_LANG < 202002L
#error CPUv did not propagate the selected C++20 profile through MSBuild
#endif
int main(){return 0;}
'@ | Set-Content -LiteralPath (Join-Path $msbuild 'main.cpp') -Encoding ascii
$manifestPath=Join-Path $msbuild 'cpuv.toml'
[IO.File]::WriteAllText($manifestPath,[IO.File]::ReadAllText($manifestPath).Replace('std = "default"','std = "c++20"'),[Text.UTF8Encoding]::new($false))
@'
<Project DefaultTargets="Build" ToolsVersion="Current" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
 <ItemGroup Label="ProjectConfigurations"><ProjectConfiguration Include="Release|x64"><Configuration>Release</Configuration><Platform>x64</Platform></ProjectConfiguration></ItemGroup>
 <PropertyGroup Label="Globals"><ProjectGuid>{F4106998-B8E3-4F59-A23E-A17D3F324E72}</ProjectGuid></PropertyGroup>
 <Import Project="$(VCTargetsPath)\Microsoft.Cpp.Default.props"/>
 <PropertyGroup Label="Configuration"><ConfigurationType>Application</ConfigurationType></PropertyGroup>
 <Import Project="$(VCTargetsPath)\Microsoft.Cpp.props"/>
 <PropertyGroup><OutDir>$(MSBuildProjectDirectory)\out\</OutDir><IntDir>$(MSBuildProjectDirectory)\obj\</IntDir></PropertyGroup>
 <ItemDefinitionGroup><ClCompile><RuntimeLibrary>MultiThreadedDLL</RuntimeLibrary></ClCompile><Link><SubSystem>Console</SubSystem></Link></ItemDefinitionGroup>
 <ItemGroup><ClCompile Include="main.cpp"/></ItemGroup>
 <Import Project="$(VCTargetsPath)\Microsoft.Cpp.targets"/>
 <Target Name="RunTests" DependsOnTargets="Build"><Exec Command="&quot;$(TargetPath)&quot;"/></Target>
</Project>
'@ | Set-Content -LiteralPath (Join-Path $msbuild 'consumer.vcxproj') -Encoding ascii
@'
[build]
system="msbuild"
project="consumer.vcxproj"
test_arguments=["/t:RunTests"]
'@ | Add-Content -LiteralPath (Join-Path $msbuild 'cpuv.toml') -Encoding ascii
$vendor=Join-Path $msbuild 'vendor'
New-Item -ItemType Directory -Path $vendor | Out-Null
$project=[IO.File]::ReadAllText((Join-Path $msbuild 'consumer.vcxproj')).Replace('<ConfigurationType>Application</ConfigurationType>','<ConfigurationType>StaticLibrary</ConfigurationType>')
[IO.File]::WriteAllText((Join-Path $vendor 'consumer.vcxproj'),$project,[Text.UTF8Encoding]::new($false))
'int msbuild_fixture_value(){return 72;}' | Set-Content -LiteralPath (Join-Path $vendor 'main.cpp') -Encoding ascii
'int msbuild_fixture_value();' | Set-Content -LiteralPath (Join-Path $vendor 'value.h') -Encoding ascii
$snapshot=& $Binary --project $msbuild source inspect $vendor | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) {throw 'MSBuild source snapshot failed'}
$spec=Join-Path $msbuild 'source-package.toml'
@"
version="1.0"
[source.input]
kind="local"
path="vendor"
sha256="$($snapshot.sha256)"
[source.build]
system="msbuild"
project="consumer.vcxproj"
install={"build/out/consumer.lib"="lib/msbuild_fixture.lib","source/value.h"="include/value.h"}
[source.interface]
libraries=["msbuild_fixture"]
"@ | Set-Content -LiteralPath $spec -Encoding ascii
Invoke-Cpuv $msbuild source add msbuild-fixture $spec
@'
#include <value.h>
#if _MSVC_LANG < 202002L
#error CPUv did not propagate the selected C++20 profile through MSBuild
#endif
int main(){return msbuild_fixture_value()==72?0:1;}
'@ | Set-Content -LiteralPath (Join-Path $msbuild 'main.cpp') -Encoding ascii
Invoke-Cpuv $msbuild test
if (Test-Path -LiteralPath (Join-Path $msbuild 'out')) {throw 'MSBuild wrote outputs into the source tree'}

$runtime=Join-Path $fixture 'runtime'
Invoke-Cpuv $runtime --compiler $MsvcCompiler init
New-Item -ItemType Directory -Path (Join-Path $runtime 'resources') | Out-Null
'phase2' | Set-Content -LiteralPath (Join-Path $runtime 'resources/value.txt') -Encoding ascii
'extern "C" __declspec(dllexport) int fixture_dependency(){return 74;}' | Set-Content -LiteralPath (Join-Path $runtime 'dependency.cpp') -Encoding ascii
@'
extern "C" __declspec(dllimport) int fixture_dependency();
extern "C" __declspec(dllexport) int fixture_plugin(){return fixture_dependency();}
'@ | Set-Content -LiteralPath (Join-Path $runtime 'plugin.cpp') -Encoding ascii
@'
#include <windows.h>
#include <cstdlib>
#include <fstream>
#include <string>
extern "C" __declspec(dllimport) int fixture_dependency();
int main(){
  auto root=std::getenv("CPUV_FIXTURE_RESOURCES"); auto plugin=std::getenv("CPUV_FIXTURE_PLUGIN");
  if(!root||!plugin)return 1;
  std::ifstream input(std::string(root)+"/value.txt"); std::string value; input>>value;
  auto dll=LoadLibraryA(plugin); if(!dll)return 2;
  auto call=reinterpret_cast<int(*)()>(GetProcAddress(dll,"fixture_plugin"));
  return call&&call()==74&&fixture_dependency()==74&&value=="phase2"?0:3;
}
'@ | Set-Content -LiteralPath (Join-Path $runtime 'main.cpp') -Encoding ascii
@'
cmake_minimum_required(VERSION 3.20)
project(runtime_fixture LANGUAGES CXX)
add_library(fixture_dependency SHARED dependency.cpp)
add_library(fixture_plugin SHARED plugin.cpp)
target_link_libraries(fixture_plugin PRIVATE fixture_dependency)
set_target_properties(fixture_plugin PROPERTIES SUFFIX ".pyd")
add_executable(consumer main.cpp)
target_link_libraries(consumer PRIVATE fixture_dependency delayimp)
target_link_options(consumer PRIVATE /DELAYLOAD:fixture_dependency.dll)
enable_testing()
add_test(NAME consumer COMMAND consumer)
'@ | Set-Content -LiteralPath (Join-Path $runtime 'CMakeLists.txt') -Encoding ascii
@'
[runtime]
assets=["resources"]
plugins=[".cpuv/build/cmake/fixture_plugin.pyd"]
search_paths=[".cpuv/build/cmake"]
[runtime.environment]
CPUV_FIXTURE_RESOURCES="${root}/resources"
CPUV_FIXTURE_PLUGIN="${root}/.cpuv/build/cmake/fixture_plugin.pyd"
'@ | Add-Content -LiteralPath (Join-Path $runtime 'cpuv.toml') -Encoding ascii
Invoke-Cpuv $runtime lock
Invoke-Cpuv $runtime test
# A deployment's plugin location is relative to its root, unlike the project's build tree.
$request=Join-Path $runtime 'cpuv.toml'
$text=[IO.File]::ReadAllText($request).Replace('"${root}/.cpuv/build/cmake/fixture_plugin.pyd"','"${root}/fixture_plugin.pyd"')
[IO.File]::WriteAllText($request,$text,[Text.UTF8Encoding]::new($false))
Invoke-Cpuv $runtime lock
$destination=Join-Path $fixture 'application'
Invoke-Cpuv $runtime bundle 'cmake/consumer.exe' $destination
$receipt=Get-Content -LiteralPath (Join-Path $destination 'cpuv-bundle.json') -Raw | ConvertFrom-Json
if (-not $receipt.files.'cpuv-run.ps1') {throw 'Runtime launcher is missing its digest'}
if (-not (Test-Path -LiteralPath (Join-Path $destination 'fixture_dependency.dll'))) {throw 'Delay-import closure is incomplete'}
$previousPath=$env:PATH
try {
    $env:PATH=Join-Path $env:SystemRoot 'System32'
    & (Join-Path $destination 'cpuv-run.ps1')
    if ($LASTEXITCODE -ne 0) {throw 'Plugin/resource/delay-import bundle failed with a minimal PATH'}
} finally {$env:PATH=$previousPath}
Write-Output "PASS: Meson, MSBuild, pinned specialist tools, plugin/resources/environment and delayed imports; fixture: $fixture"

param(
    [Parameter(Mandatory=$true)][string]$Compiler,
    [string]$MsvcCompiler,
    [string]$ClangClCompiler,
    [string[]]$AdditionalCompiler=@(),
    [string[]]$CrossCompiler=@(),
    [string]$EvidenceDirectory,
    [switch]$ReleaseBuild
)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
if (-not $EvidenceDirectory) {$EvidenceDirectory=Join-Path ([IO.Path]::GetTempPath()) ('cpuv-phase2-evidence-'+[guid]::NewGuid().ToString('N'))}
New-Item -ItemType Directory -Path $EvidenceDirectory -Force | Out-Null
$EvidenceDirectory=(Resolve-Path -LiteralPath $EvidenceDirectory).Path
if (-not $MsvcCompiler) {
    $vswhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $MsvcCompiler=& $vswhere -latest -prerelease -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find 'VC/Tools/MSVC/**/bin/Hostx64/x64/cl.exe' | Select-Object -First 1
}
if (-not $MsvcCompiler) {throw 'Phase 2 requires Microsoft C++ Build Tools and the Windows SDK'}
$previousCompiler=$env:CPUV_TEST_CXX
$previousWindow=$env:CPUV_MAX_ZSTD_WINDOW_MIB
$profileArguments=@(if($ReleaseBuild){'--release'})
$buildProfile=if($ReleaseBuild){'release'}else{'debug'}
$status='failed'
Push-Location -LiteralPath $repo
Start-Transcript -LiteralPath (Join-Path $EvidenceDirectory 'gate.log') | Out-Null
try {
    $env:CPUV_TEST_CXX=(Resolve-Path -LiteralPath $Compiler).Path
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) {throw 'Formatting failed'}
    cargo clippy --workspace --all-targets --locked @profileArguments -- -D warnings
    if ($LASTEXITCODE -ne 0) {throw 'Clippy failed'}
    cargo test --workspace --locked @profileArguments
    if ($LASTEXITCODE -ne 0) {throw 'Workspace tests failed'}
    cargo test -p cpuv-cli --test phase2 --locked @profileArguments -- --ignored --nocapture
    if ($LASTEXITCODE -ne 0) {throw 'Source/lock/offline/cache acceptance failed'}
    cargo build --workspace --locked @profileArguments
    if ($LASTEXITCODE -ne 0) {throw 'Build failed'}
    $metadata=cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) {throw 'Cargo metadata failed'}
    $binary=Join-Path $metadata.target_directory ($buildProfile+'/cpuv.exe')
    & (Join-Path $PSScriptRoot 'test-phase2-adapters.ps1') -Compiler $Compiler -MsvcCompiler $MsvcCompiler -Binary $binary
    & (Join-Path $PSScriptRoot 'test-vcpkg.ps1') -Compiler $MsvcCompiler -Binary $binary
    if (-not $ClangClCompiler) {
        $env:CPUV_MAX_ZSTD_WINDOW_MIB='1024'
        & $binary toolchain provision (Join-Path $repo 'examples/toolchains/llvm-cl-23.1.4.toml')
        if ($LASTEXITCODE -ne 0) {throw 'Verified stable clang-cl provisioning failed'}
        # The archive receipt supplies the selected executable without changing a user's default compiler.
        $cache=if($env:CPUV_CACHE_DIR){$env:CPUV_CACHE_DIR}else{Join-Path $env:LOCALAPPDATA 'cpuv'}
        $receipt=Get-Content -LiteralPath (Join-Path $cache 'toolchains/receipts/1a3f2f998e230b5791cc4e2bfe11a1f7c07fe42c96ff44f227c9db0bedfaf091.json') -Raw | ConvertFrom-Json
        $ClangClCompiler=$receipt.compiler
    }
    & (Join-Path $PSScriptRoot 'test-windows.ps1') -Compiler $ClangClCompiler -Binary $binary -Catalog -CMake -Bundle
    foreach ($candidate in @($Compiler,$MsvcCompiler,$ClangClCompiler)+$AdditionalCompiler) {
        $identity=[IO.Path]::GetFileNameWithoutExtension($candidate)+'-'+[guid]::NewGuid().ToString('N')
        & $binary --compiler $candidate toolchain capabilities | Set-Content -LiteralPath (Join-Path $EvidenceDirectory ($identity+'.json')) -Encoding utf8
        if ($LASTEXITCODE -ne 0) {throw "Native capability probe failed: $candidate"}
    }
    foreach ($candidate in $AdditionalCompiler) {
        & (Join-Path $PSScriptRoot 'test-windows.ps1') -Compiler $candidate -Binary $binary -CMake -Bundle
    }
    if ($CrossCompiler.Count -eq 0) {
        & $binary toolchain provision (Join-Path $repo 'examples/toolchains/llvm-mingw-arm64-20261020.toml')
        if ($LASTEXITCODE -ne 0) {throw 'ARM64 cross-compiler provisioning failed'}
        $cache=if($env:CPUV_CACHE_DIR){$env:CPUV_CACHE_DIR}else{Join-Path $env:LOCALAPPDATA 'cpuv'}
        $receipt=Get-Content -LiteralPath (Join-Path $cache 'toolchains/receipts/fd5462b7f5d93b2ff981fcd3f9c7e427e47f3203c64ea91c27bcc444eebd74cd.json') -Raw | ConvertFrom-Json
        $CrossCompiler=@($receipt.compiler)
    }
    foreach ($candidate in $CrossCompiler) {
        & $binary --compiler $candidate doctor | Set-Content -LiteralPath (Join-Path $EvidenceDirectory ('cross-'+[guid]::NewGuid().ToString('N')+'.json')) -Encoding utf8
        if ($LASTEXITCODE -ne 0) {throw "Cross compile/link probe failed: $candidate"}
    }
    $status='passed'
} finally {
    $revision=(& git rev-parse HEAD).Trim()
    $files=Get-ChildItem -LiteralPath @('crates','scripts','.github','.cargo','examples/toolchains','Cargo.toml','Cargo.lock','build-tools.toml') -Recurse -File
    $hashes=@($files | Sort-Object FullName | ForEach-Object {[ordered]@{path=(Resolve-Path -LiteralPath $_.FullName -Relative);sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}})
    [ordered]@{schema=1;phase=2;result=$status;utc=[DateTime]::UtcNow.ToString('o');source_revision=$revision;compiler=$Compiler;msvc=$MsvcCompiler;clang_cl=$ClangClCompiler;additional=$AdditionalCompiler;cross_compile_only=$CrossCompiler;source_files=$hashes} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $EvidenceDirectory 'evidence.json') -Encoding utf8
    $env:CPUV_TEST_CXX=$previousCompiler
    $env:CPUV_MAX_ZSTD_WINDOW_MIB=$previousWindow
    Stop-Transcript | Out-Null
    Pop-Location
}
Write-Output "Phase 2 gate: $status. Evidence: $EvidenceDirectory"

param(
    [Parameter(Mandatory = $true)][string]$Compiler,
    [string]$EvidenceDirectory,
    [switch]$SkipConan,
    [switch]$ReleaseBuild
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$Compiler = (Resolve-Path -LiteralPath $Compiler).Path
$repo = Split-Path $PSScriptRoot -Parent
if (-not $EvidenceDirectory) { $EvidenceDirectory = Join-Path ([IO.Path]::GetTempPath()) ('cpuv-phase1-evidence-' + [guid]::NewGuid().ToString('N')) }
New-Item -ItemType Directory -Force -Path $EvidenceDirectory | Out-Null
$EvidenceDirectory = (Resolve-Path -LiteralPath $EvidenceDirectory).Path
$previousCompiler = $env:CPUV_TEST_CXX
$transcribing = $false
$status = 'failed'
$binary = $null
$profileArguments = @(if ($ReleaseBuild) { '--release' })
$buildProfile = if ($ReleaseBuild) { 'release' } else { 'debug' }
$nativeLog = Join-Path $EvidenceDirectory 'commands.log'
function Write-GateLog {
    param([Parameter(ValueFromPipeline = $true)]$Entry)
    process {
        Add-Content -LiteralPath $nativeLog -Value ([string]$Entry) -Encoding utf8
        Write-Output $Entry
    }
}
Push-Location -LiteralPath $repo
try {
    Start-Transcript -LiteralPath (Join-Path $EvidenceDirectory 'gate.log') | Out-Null
    $transcribing = $true
    $env:CPUV_TEST_CXX = $Compiler
    cargo fmt --all -- --check 2>&1 | Write-GateLog
    if ($LASTEXITCODE -ne 0) { throw 'Phase 1 formatting gate failed' }
    cargo clippy --workspace --all-targets --locked @profileArguments -- -D warnings 2>&1 | Write-GateLog
    if ($LASTEXITCODE -ne 0) { throw 'Phase 1 Clippy gate failed' }
    cargo test --workspace --locked @profileArguments 2>&1 | Write-GateLog
    if ($LASTEXITCODE -ne 0) { throw 'Phase 1 unit/regression gate failed' }
    cargo test -p cpuv-cli --test phase1 --locked @profileArguments -- --ignored --nocapture 2>&1 | Write-GateLog
    if ($LASTEXITCODE -ne 0) { throw 'Phase 1 local repository/native gate failed' }
    cargo build --workspace --locked @profileArguments 2>&1 | Write-GateLog
    if ($LASTEXITCODE -ne 0) { throw 'Phase 1 build gate failed' }
    $metadata = cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Could not identify tested build outputs' }
    $binary = Join-Path $metadata.target_directory ($buildProfile + '\cpuv.exe')
    & $binary --compiler $Compiler doctor | Set-Content -LiteralPath (Join-Path $EvidenceDirectory 'compiler.json') -Encoding utf8
    if ($LASTEXITCODE -ne 0) { throw 'Phase 1 compiler/linker/run check failed' }
    & (Join-Path $PSScriptRoot 'test-install.ps1') -StaleBinary $binary 2>&1 | Write-GateLog
    if (-not $SkipConan) { & (Join-Path $PSScriptRoot 'test-conan-phase1.ps1') -Compiler $Compiler -Binary $binary 2>&1 | Write-GateLog }
    $status = if ($SkipConan) { 'partial: Conan gate skipped' } else { 'passed' }
    Write-Output "Phase 1 gate: $status. Evidence: $EvidenceDirectory"
} finally {
    $revision = (& git rev-parse HEAD).Trim()
    $files = & rg --files crates scripts .github .cargo Cargo.toml Cargo.lock build-tools.toml
    $hashes = @($files | Sort-Object | ForEach-Object { [ordered]@{ path=$_; sha256=(Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant() } })
    $binaryHash = if ($binary -and (Test-Path -LiteralPath $binary)) { (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant() } else { $null }
    [ordered]@{ schema=1; phase=1; result=$status; profile=$buildProfile; rustc=((& rustc -vV) -join "`n"); utc=[DateTime]::UtcNow.ToString('o'); source_revision=$revision; compiler=$Compiler; binary=$binary; binary_sha256=$binaryHash; source_files=$hashes } |
        ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $EvidenceDirectory 'evidence.json') -Encoding utf8
    if ($transcribing) { Stop-Transcript | Out-Null }
    $env:CPUV_TEST_CXX = $previousCompiler
    Pop-Location
}

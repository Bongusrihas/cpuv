param(
    [Parameter(Mandatory=$true)][string[]]$Compiler,
    [string]$EvidenceDirectory,
    [switch]$ReleaseBuild
)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
if (-not $EvidenceDirectory) {$EvidenceDirectory=Join-Path ([IO.Path]::GetTempPath()) ('cpuv-phase3-'+[guid]::NewGuid().ToString('N'))}
New-Item -ItemType Directory -Path $EvidenceDirectory -Force | Out-Null
$EvidenceDirectory=(Resolve-Path -LiteralPath $EvidenceDirectory).Path
$previousCompiler=$env:CPUV_TEST_CXX
$previousEvidence=$env:CPUV_PHASE3_EVIDENCE
$previousPath=$env:PATH
$previousUserPath=[Environment]::GetEnvironmentVariable('Path','User')
$profileArguments=@(if($ReleaseBuild){'--release'})
$status='failed'
Push-Location -LiteralPath $repo
Start-Transcript -LiteralPath (Join-Path $EvidenceDirectory 'gate.log') | Out-Null
try {
    $env:CPUV_PHASE3_EVIDENCE=$EvidenceDirectory
    cargo test -p cpuv-cli --test lifecycle --locked @profileArguments -- --nocapture
    if ($LASTEXITCODE -ne 0) {throw 'Setup/update/uninstall lifecycle failed'}
    foreach ($candidate in $Compiler) {
        $env:CPUV_TEST_CXX=(Resolve-Path -LiteralPath $candidate).Path
        cargo test -p cpuv-cli --test phase3 --locked @profileArguments -- --ignored --nocapture
        if ($LASTEXITCODE -ne 0) {throw "Phase 3 consumer matrix failed: $candidate"}
    }
    if ($env:PATH -cne $previousPath -or [Environment]::GetEnvironmentVariable('Path','User') -cne $previousUserPath) {throw 'Lifecycle tests changed the invoking user PATH'}
    $status='passed'
} finally {
    $revision=(& git rev-parse HEAD).Trim()
    $sourceFiles=@(& git ls-files --cached --others --exclude-standard -- crates scripts .github Cargo.toml Cargo.lock build-tools.toml)
    $hashes=@($sourceFiles | Sort-Object -Unique | ForEach-Object {[ordered]@{path=$_;sha256=(Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()}})
    $timings=@(& (Join-Path $PSScriptRoot 'summarize-phase3.ps1') -EvidenceDirectory $EvidenceDirectory)
    [ordered]@{schema=1;phase=3;result=$status;utc=[DateTime]::UtcNow.ToString('o');source_revision=$revision;os=[Environment]::OSVersion.VersionString;process_architecture=$env:PROCESSOR_ARCHITECTURE;compiler=$Compiler;source_files=$hashes;timings=$timings;timing_note='Observed wall time; no absolute pass/fail threshold or cross-machine speed claim.'} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $EvidenceDirectory 'evidence.json') -Encoding utf8
    $env:CPUV_TEST_CXX=$previousCompiler
    $env:CPUV_PHASE3_EVIDENCE=$previousEvidence
    Stop-Transcript | Out-Null
    Pop-Location
}
Write-Output "Phase 3 gate: $status. Evidence: $EvidenceDirectory"

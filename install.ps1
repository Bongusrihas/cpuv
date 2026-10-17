[CmdletBinding()]
param(
    [ValidatePattern('^(latest|v?\d+\.\d+\.\d+(?:-[A-Za-z0-9.-]+)?)$')]
    [string]$Version = $(if ($env:CPUV_VERSION) { $env:CPUV_VERSION } else { 'latest' }),
    [string]$InstallDir = $env:CPUV_INSTALL_DIR,
    [switch]$NoPath,
    [string]$ReleaseBaseUrl = 'https://github.com/Bongusrihas/cpuv/releases'
)
$ErrorActionPreference = 'Stop'
if (-not [Environment]::Is64BitOperatingSystem) { throw 'cpuv currently distributes a 64-bit Windows executable.' }
$savedProgressPreference = $ProgressPreference
$ProgressPreference = 'SilentlyContinue'
$temporaryDirectory = $null
try {
    # The portable ZIP can install entirely offline; no source build is attempted.
    $localBinary = if ($PSScriptRoot) { Join-Path $PSScriptRoot 'cpuv.exe' } else { $null }
    if ($localBinary -and (Test-Path -LiteralPath $localBinary -PathType Leaf)) {
        $installer = $localBinary
        $arguments = @('self','install')
    } else {
        $base = [Uri]$ReleaseBaseUrl
        if (-not $base.IsAbsoluteUri -or ($base.Scheme -ne 'https' -and -not ($base.IsLoopback -and $base.Scheme -eq 'http'))) {
            throw 'The release URL must use HTTPS (loopback HTTP is allowed for local testing).'
        }
        $tag = if ($Version -eq 'latest') { 'latest/download' } else { 'download/v' + $Version.TrimStart('v') }
        $assetBase = $ReleaseBaseUrl.TrimEnd('/') + '/' + $tag
        $temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) ('cpuv-install-' + [Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $temporaryDirectory | Out-Null
        $installer = Join-Path $temporaryDirectory 'cpuv-setup.exe'
        $checksumFile = Join-Path $temporaryDirectory 'cpuv-setup.exe.sha256'
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
        Write-Host 'Downloading the prebuilt cpuv installer...'
        try {
            Invoke-WebRequest -UseBasicParsing -Uri ($assetBase + '/cpuv-setup.exe') -OutFile $installer
            Invoke-WebRequest -UseBasicParsing -Uri ($assetBase + '/cpuv-setup.exe.sha256') -OutFile $checksumFile
        } catch {
            throw "Could not download the prebuilt cpuv release. The maintainer must publish its release assets first. $($_.Exception.Message)"
        }
        $checksum = (Get-Content -Raw -LiteralPath $checksumFile).Trim()
        if ($checksum -notmatch '^([a-fA-F0-9]{64})\s+\*?cpuv-setup\.exe$') { throw 'Invalid installer checksum file; nothing was executed.' }
        $expected = $Matches[1]
        $hasher = [Security.Cryptography.SHA256]::Create()
        $installerStream = [IO.File]::OpenRead($installer)
        try { $actual = [BitConverter]::ToString($hasher.ComputeHash($installerStream)).Replace('-','') }
        finally { $installerStream.Dispose(); $hasher.Dispose() }
        if ($actual -ine $expected) { throw 'Installer checksum mismatch; nothing was executed.' }
        $arguments = @()
    }
    if ($InstallDir) { $arguments += @('--install-dir',$InstallDir) }
    if ($NoPath) { $arguments += '--no-path' }
    & $installer @arguments
    if ($LASTEXITCODE -ne 0) { throw 'cpuv installation failed.' }
    if (-not $NoPath) {
        $destination = if ($InstallDir) { [IO.Path]::GetFullPath($InstallDir) } else { Join-Path $env:USERPROFILE '.local\bin' }
        $env:PATH = $destination + ';' + $env:PATH
        Write-Host 'Ready. Use cpuv init in your project folder.'
    }
} finally {
    $ProgressPreference = $savedProgressPreference
    if ($temporaryDirectory) {
        foreach ($temporaryFile in @('cpuv-setup.exe','cpuv-setup.exe.sha256')) {
            $temporaryPath = Join-Path $temporaryDirectory $temporaryFile
            if (Test-Path -LiteralPath $temporaryPath) { Remove-Item -LiteralPath $temporaryPath -Force }
        }
        Remove-Item -LiteralPath $temporaryDirectory -Force
    }
}

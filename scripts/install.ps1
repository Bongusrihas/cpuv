[CmdletBinding()]
param(
    [string]$BinaryDirectory = (Join-Path (Split-Path $PSScriptRoot -Parent) 'target\release'),
    [string]$InstallDir = (Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'cpuv\bin'),
    [switch]$NoPath,
    [switch]$NoCompatCopy
)
$ErrorActionPreference = 'Stop'
$binary = Join-Path $BinaryDirectory 'cpuv.exe'
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw "Missing $binary. Build cpuv first." }
$destination = [IO.Path]::GetFullPath($InstallDir)
New-Item -ItemType Directory -Force -Path $destination | Out-Null
foreach ($name in @('cpuv.exe','cpuv-shim.exe')) {
    $source = Join-Path $BinaryDirectory $name
    if (Test-Path -LiteralPath $source -PathType Leaf) {
        if ([IO.Path]::GetFullPath($source) -ine (Join-Path $destination $name)) {
            Copy-Item -LiteralPath $source -Destination (Join-Path $destination $name) -Force
        }
    }
}
$compatibilityDirectory = Join-Path ([Environment]::GetFolderPath('UserProfile')) '.local\bin'
if (-not $NoCompatCopy -and $compatibilityDirectory -ine $destination) {
    New-Item -ItemType Directory -Force -Path $compatibilityDirectory | Out-Null
    foreach ($name in @('cpuv.exe','cpuv-shim.exe')) {
        $source = Join-Path $BinaryDirectory $name
        if (Test-Path -LiteralPath $source -PathType Leaf) {
            Copy-Item -LiteralPath $source -Destination (Join-Path $compatibilityDirectory $name) -Force
        }
    }
    $legacy = Join-Path $compatibilityDirectory 'cpuv.cmd'
    if ((Test-Path -LiteralPath $legacy) -and (Get-Content -Raw -LiteralPath $legacy).Contains('rem cpuv command launcher')) {
        Remove-Item -LiteralPath $legacy
    }
}
if (-not $NoPath) {
    $userPath = [Environment]::GetEnvironmentVariable('Path','User')
    $otherEntries = @($userPath -split ';' | Where-Object {
        $_ -and [Environment]::ExpandEnvironmentVariables($_).TrimEnd('\','/') -ine $destination.TrimEnd('\','/')
    })
    $updatedPath = (@($destination) + $otherEntries) -join ';'
    if ($updatedPath -cne $userPath) {
        $backupDirectory = Split-Path $destination -Parent
        $backup = Join-Path $backupDirectory 'user-path-before-install.txt'
        if (-not (Test-Path -LiteralPath $backup)) { [IO.File]::WriteAllText($backup,[string]$userPath) }
        [Environment]::SetEnvironmentVariable('Path',$updatedPath,'User')
    }
    $env:PATH = $destination + ';' + $env:PATH
    if (-not ('CpuvEnvironmentBroadcast' -as [type])) {
        Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class CpuvEnvironmentBroadcast {
    [DllImport("user32.dll", CharSet=CharSet.Unicode)]
    public static extern IntPtr SendMessageTimeout(IntPtr window, uint message,
        UIntPtr wParam, string lParam, uint flags, uint timeout, out UIntPtr result);
}
'@
    }
    $result = [UIntPtr]::Zero
    [void][CpuvEnvironmentBroadcast]::SendMessageTimeout([IntPtr]0xffff,0x001a,[UIntPtr]::Zero,'Environment',2,2000,[ref]$result)
}
Write-Host "Installed cpuv in $destination"
if (-not $NoPath) { Write-Host 'User PATH updated. Reopen an existing terminal if it does not find cpuv.' }
& (Join-Path $destination 'cpuv.exe') --version
if ($LASTEXITCODE -ne 0) { throw 'Installed executable did not start successfully.' }

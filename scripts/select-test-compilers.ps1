param([switch]$IncludeX86)
$ErrorActionPreference='Stop'
$vswhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$installations=@(& $vswhere -all -prerelease -products '*' -property installationPath)
if ($LASTEXITCODE -ne 0) {throw 'Visual Studio discovery failed'}
$architectures=if($env:PROCESSOR_ARCHITECTURE -eq 'ARM64'){@('arm64')}elseif($IncludeX86){@('x64','x86')}else{@('x64')}
$selected=@()
foreach ($architecture in $architectures) {
    $candidates=@(foreach($installation in $installations) {
        $toolsets=Join-Path $installation 'VC/Tools/MSVC'
        if (-not (Test-Path -LiteralPath $toolsets -PathType Container)) {continue}
        foreach($toolset in Get-ChildItem -LiteralPath $toolsets -Directory) {
            # Native ARM coverage requires native-hosted tools. Emulated x64-hosted cross
            # toolsets need a separate Conan build-host profile and are not this lane.
            $hosts=if($env:PROCESSOR_ARCHITECTURE -eq 'ARM64'){@('Hostarm64')}else{@('Hostx64')}
            foreach($hostDirectory in $hosts) {
                $compiler=Join-Path $toolset.FullName ("bin/$hostDirectory/$architecture/cl.exe")
                if (Test-Path -LiteralPath $compiler -PathType Leaf) {
                    [pscustomobject]@{version=[version]$toolset.Name;compiler=$compiler}
                    break
                }
            }
        }
    })
    if ($candidates.Count -eq 0) {throw "No runnable Microsoft compiler for $architecture"}
    $ordered=@($candidates | Sort-Object version,compiler)
    $selected+=@($ordered[0].compiler,$ordered[-1].compiler)
}
$selected | Select-Object -Unique

param([Parameter(Mandatory=$true)][string]$EvidenceDirectory)
$ErrorActionPreference='Stop'
$measurements=@(Get-ChildItem -LiteralPath $EvidenceDirectory -Filter 'consumer-*.json' | ForEach-Object {
    $case=Get-Content -LiteralPath $_.FullName -Raw | ConvertFrom-Json
    foreach($measurement in $case.measurements) {
        [pscustomobject]@{compiler=$case.toolchain.compiler_path;operation=$measurement.operation;milliseconds=$measurement.milliseconds}
    }
})
$measurements | Group-Object compiler,operation | ForEach-Object {
    $values=@($_.Group.milliseconds | Sort-Object)
    $middle=[math]::Floor($values.Count/2)
    $median=if($values.Count%2 -eq 0){($values[$middle-1]+$values[$middle])/2}else{$values[$middle]}
    [pscustomobject]@{compiler=$_.Group[0].compiler;operation=$_.Group[0].operation;samples=$values.Count;median_ms=$median;p95_ms=$values[[math]::Ceiling($values.Count*0.95)-1]}
}

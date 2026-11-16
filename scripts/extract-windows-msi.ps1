param([string]$Inspection='target/distribution-validation/installer-inspection.json')
$ErrorActionPreference='Stop'
$report=Get-Content -LiteralPath $Inspection -Raw | ConvertFrom-Json
$allowed=@('CostInitialize','FileCost','CostFinalize','InstallValidate','InstallInitialize','InstallAdminPackage','InstallFiles','InstallFinalize')
if (-not $report.AdminExecuteSequence -or $report.AdminExecuteSequence.unavailable) { throw 'Administrative action sequence was not inspected' }
foreach ($row in $report.AdminExecuteSequence) {
    if ($row[0] -notin $allowed) { throw 'Unexpected administrative installer action; extraction excluded' }
}
if ((Get-FileHash -LiteralPath $report.msi).Hash -ne $report.sha256) { throw 'Installer changed after inspection' }
$workspace=(Resolve-Path '.').Path
$destination=[IO.Path]::GetFullPath((Join-Path $workspace ('target/distribution-validation/msi-admin-'+[guid]::NewGuid().ToString('N').Substring(0,8))))
if (-not $destination.StartsWith($workspace+'\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Extraction must stay in this checkout' }
New-Item -ItemType Directory -Path $destination | Out-Null
$arguments='/a "'+$report.msi+'" /qn TARGETDIR="'+$destination+'" /L*v "'+(Join-Path $destination 'extraction.log')+'" /norestart'
$process=Start-Process msiexec.exe -ArgumentList $arguments -WindowStyle Hidden -PassThru
if (-not $process.WaitForExit(60000)) { throw 'Administrative extraction did not finish within60s; do not assume install validation' }
$files=@(Get-ChildItem -LiteralPath $destination -Recurse -File | Where-Object Extension -eq '.exe' | ForEach-Object {
    @{relativePath=$_.FullName.Substring($destination.Length+1);sha256=(Get-FileHash -LiteralPath $_.FullName).Hash;bytes=$_.Length}
})
$result=@{timestamp=[DateTime]::UtcNow.ToString('o');operation='MSI administrative payload extraction only, not installation';exitCode=$process.ExitCode;source=$report.source;msiSha256=$report.sha256;output=$destination;files=$files}
$result | ConvertTo-Json -Depth 5 | Set-Content target/distribution-validation/msi-extraction.json
$result | ConvertTo-Json -Depth 5
if ($process.ExitCode -ne 0) { throw 'MSI extraction failed; inspect retained log' }

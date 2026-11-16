param([Parameter(Mandatory=$true)][ValidatePattern('^[a-f0-9]{40}$')][string]$SourceCommit, [string]$Output='target/distribution-validation/installer-inspection.json')
$ErrorActionPreference='Stop'
$installer=New-Object -ComObject WindowsInstaller.Installer
$msi=(Resolve-Path 'target/release/bundle/msi/Quotis_2.0.0_x64_en-US.msi').Path
$database=$installer.OpenDatabase($msi,0)
function Query-Msi([string]$Query) {
    $view=$database.OpenView($Query)
    [void]$view.Execute()
    $rows=@()
    while ($null -ne ($record=$view.Fetch())) {
        $values=@()
        $count=$record.GetType().InvokeMember('FieldCount',[Reflection.BindingFlags]::GetProperty,$null,$record,$null)
        for ($i=1;$i -le $count;$i++) { $values += $record.GetType().InvokeMember('StringData',[Reflection.BindingFlags]::GetProperty,$null,$record,@($i)) }
        $rows += ,$values
    }
    [void]$view.Close()
    ,$rows
}
$report=[ordered]@{timestamp=[DateTime]::UtcNow.ToString('o');source=$SourceCommit;msi=$msi;sha256=(Get-FileHash -LiteralPath $msi -Algorithm SHA256).Hash}
foreach ($table in @('Property','File','Registry','Upgrade','LaunchCondition','CustomAction','AdminExecuteSequence','InstallExecuteSequence')) {
    try { $report[$table]=Query-Msi ('SELECT * FROM `' + $table + '`') }
    catch { $report[$table]=@{unavailable=$_.Exception.Message} }
}
$report.signing=@()
foreach ($file in @('target/release/quotis.exe','target/release/quotis-cli.exe','target/release/bundle/nsis/Quotis_2.0.0_x64-setup.exe','target/release/bundle/msi/Quotis_2.0.0_x64_en-US.msi')) {
    $signature=Get-AuthenticodeSignature -LiteralPath $file
    $report.signing += @{file=$file;sha256=(Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash;status=$signature.Status.ToString()}
}
$free=(Get-PSDrive C).Free
try { $sandboxFeature=(Get-CimInstance Win32_OptionalFeature -Filter "Name='Containers-DisposableClientVM'").InstallState } catch { $sandboxFeature='UNVERIFIED: '+$_.Exception.Message }
$report.preflight=@{systemFreeBytes=$free;workspaceFreeBytes=(Get-PSDrive D).Free;sandboxFeature=$sandboxFeature;ownerInstallPresent=(Test-Path 'HKCU:/Software/Microsoft/Windows/CurrentVersion/Uninstall/Quotis');sandboxStarted=$false;reason=if ($free -lt 1GB) {'System-volume free space below 1 GB; host installation excluded to preserve owner0.2.1'} else {'No disposable OS test executed by this read-only inspection'}}
$report | ConvertTo-Json -Depth 15 | Set-Content -LiteralPath $Output
$report.Property | Where-Object {$_[0] -in @('ProductName','ProductVersion','ProductCode','UpgradeCode','Manufacturer','ALLUSERS','MSIINSTALLPERUSER')} | ConvertTo-Json -Depth 5
$report.File | ConvertTo-Json -Depth 5
$report.AdminExecuteSequence | ConvertTo-Json -Depth 5
$report.LaunchCondition | ConvertTo-Json -Depth 5
Write-Output "Full read-only MSI/signature inspection: $Output"

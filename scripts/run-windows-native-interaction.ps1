param([int]$TimeoutSeconds=15)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices; using System.Text;
public class QuotisNativeWindows {
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder title, int max);
}
'@
$session=Get-Content target/distribution-validation/current-session.json -Raw | ConvertFrom-Json
$process=Get-Process -Id $session.pid
if ($process.Path -ne $session.executable) { throw 'Validation process identity changed' }
$null=$process.Handle
$results=[Collections.Generic.List[object]]::new()
function Wait-Check([string]$Name,[scriptblock]$Check) {
    $end=[DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        try { if (& $Check) { $results.Add(@{name=$Name;result='PASS';timestamp=[DateTime]::UtcNow.ToString('o')}); Write-Output "PASS $Name"; return } } catch { $lastCheckError=$_.Exception.Message }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $end)
    $results.Add(@{name=$Name;result='FAIL';detail=$lastCheckError;timestamp=[DateTime]::UtcNow.ToString('o')})
    throw "Native check failed: $Name"
}
function Window([string]$Title) {
    $found=[Collections.Generic.List[long]]::new()
    $callback=[QuotisNativeWindows+EnumProc]{param($handle,$parameter)
        [uint32]$ownerId=0; [void][QuotisNativeWindows]::GetWindowThreadProcessId($handle,[ref]$ownerId)
        if ($ownerId -eq $session.pid) {
            $titleText=[Text.StringBuilder]::new(256)
            [void][QuotisNativeWindows]::GetWindowText($handle,$titleText,256)
            if ($titleText.ToString() -eq $Title) { $found.Add($handle.ToInt64()) }
        }
        return $true
    }
    [void][QuotisNativeWindows]::EnumWindows($callback,[IntPtr]::Zero)
    if ($found.Count -ne 1) { throw "Expected one owned $Title window" }
    [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$found[0])
}
function Control($Root,[string]$Type,[string]$NamePattern) {
    $all=$Root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
    $hits=@($all | Where-Object {$_.Current.ControlType.ProgrammaticName -eq "ControlType.$Type" -and $_.Current.Name -match $NamePattern})
    if ($hits.Count -ne 1) { throw "Expected one $Type / $NamePattern" }
    $hits[0]
}
function Snapshot { Get-Content (Join-Path $session.profile 'snapshot.json') -Raw | ConvertFrom-Json }
function Settings { Get-Content (Join-Path $session.profile 'settings.committed.json') -Raw | ConvertFrom-Json }
function Gemini { (Snapshot).providers | Where-Object id -eq gemini }
$main=$null; $settingsWindow=$null
try {
    Wait-Check 'native empty-state button available' { $script:main=Window 'Quotis'; $null=Control $main Button 'No tools.*Expand details'; $true }
    (Control $main Button 'No tools.*Expand details').GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Expand()
    Wait-Check 'native expand callback and accessible details' { (Control (Window 'Quotis') Button 'No tools.*Collapse details').Current.Name -and (Control (Window 'Quotis') Button '^Settings$') }
    (Control (Window 'Quotis') Button '^Settings$').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Wait-Check 'Settings opens through native control' { $script:settingsWindow=Window 'Quotis Settings'; $null=Control $settingsWindow CheckBox '^Gemini CLIOff$'; $true }
    (Control (Window 'Quotis Settings') CheckBox '^Gemini CLIOff$').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
    Wait-Check 'native provider toggle persists to committed settings' { (Settings).providers.gemini.enabled }
    Wait-Check 'isolated local source collects42 tokens' { (Gemini).metrics[0].used -eq 42 }
    Wait-Check 'Settings badge updates to Connected without reopening' { $null=Control (Window 'Quotis Settings') CheckBox '^Gemini CLIConnected$'; $true }
    for ($i=0; $i -lt 4; $i++) {
        (Control (Window 'Quotis Settings') CheckBox '^Gemini CLI').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
    }
    Start-Sleep -Milliseconds 300
    Wait-Check 'rapid native provider changes retain final disk and runtime choice' { (Settings).providers.gemini.enabled -and (Gemini).status -eq 'connected' }
    Wait-Check 'rapid native provider changes leave control enabled' { (Control (Window 'Quotis Settings') CheckBox '^Gemini CLI').GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Current.ToggleState -eq [System.Windows.Automation.ToggleState]::On }
    foreach ($toggle in @(@('^Glass Translucent','glass'),@('^Expand on hover$','hover_expand'),@('^Keep on top$','always_on_top'))) {
        (Control (Window 'Quotis Settings') CheckBox $toggle[0]).GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
        $property=$toggle[1]
        Wait-Check "native $property control persists" { (Settings).$property -eq $true }
    }
    $source=(Settings).providers.gemini.dir
    Add-Content -LiteralPath (Join-Path $source 'tmp/session.jsonl') -Value '{"type":"unknown_usage","amount":900}'
    Wait-Check 'unknown log coverage reaches native overlay as lower bound' { $null=Control (Window 'Quotis') Button '≥42.*partial lower bound'; $true }
    $locked=Join-Path $source 'tmp/locked.jsonl'
    $stream=[IO.File]::Open($locked,[IO.FileMode]::CreateNew,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
    try {
        $stream.WriteByte(10); $stream.Flush()
        Wait-Check 'native read failure retains last metrics with error' { (Gemini).error -and (Gemini).metrics[0].used -eq 42 }
        # WebView2 can retain a checkbox's computed label after its sibling text changes.
        # Check the badge's own accessible Text node for live updates instead.
        Wait-Check 'Settings badge updates to Issue on collection error' { $null=Control (Window 'Quotis Settings') Text '^Issue$'; $true }
    } finally { $stream.Dispose(); Rename-Item -LiteralPath $locked -NewName 'locked.evidence' }
    Wait-Check 'Settings badge recovers after local error' { $null=Control (Window 'Quotis Settings') Text '^Connected$'; $true }
    (Control (Window 'Quotis Settings') Button '^Close settings$').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Wait-Check 'Settings closes and main remains available' { try {$null=Window 'Quotis Settings'; return $false} catch { return $null -ne (Window 'Quotis') } }
    (Control (Window 'Quotis') Button 'Collapse details').GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Collapse()
    Wait-Check 'native collapse callback' { $null=Control (Window 'Quotis') Button 'Expand details'; $true }
    (Control (Window 'Quotis') Button 'Expand details').GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Expand()
    Wait-Check 'details can reopen' { $null=Control (Window 'Quotis') Button '^Quit Quotis$'; $true }
    (Control (Window 'Quotis') Button '^Quit Quotis$').GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Wait-Check 'normal quit through native button' { $process.HasExited -and $process.ExitCode -eq 0 }
    Wait-Check 'settings replicas agree after normal quit' { (Get-FileHash (Join-Path $session.profile 'settings.json')).Hash -eq (Get-FileHash (Join-Path $session.profile 'settings.committed.json')).Hash -and (Get-FileHash (Join-Path $session.profile 'settings.json.bak')).Hash -eq (Get-FileHash (Join-Path $session.profile 'settings.committed.json')).Hash }
} finally {
    @{source=$session.sourceCommit;binarySha256=$session.sha256;results=@($results.ToArray());globalInputUsed=$false;visualCertification='UNVERIFIED: unrelated foreground application occludes window-region captures';profile=$session.profile} | ConvertTo-Json -Depth 15 | Set-Content (Join-Path $session.root 'native-interaction-report.json')
    if (-not $process.HasExited) {
        [void]$process.CloseMainWindow()
        if (-not $process.WaitForExit(5000)) { $process.Kill(); [void]$process.WaitForExit(5000) }
    }
}

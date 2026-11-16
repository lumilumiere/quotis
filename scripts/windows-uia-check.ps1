param(
    [Parameter(Mandatory=$true)][int]$ProcessId,
    [Parameter(Mandatory=$true)][long]$WindowHandle,
    [Parameter(Mandatory=$true)][string]$Name,
    [ValidateSet('Inspect','Invoke','Expand','Collapse','Toggle','SetValue','Select')][string]$Action='Inspect',
    [string]$Value,
    [string]$ControlType
)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class QuotisUiaOwner { [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid); }'
$session=Get-Content target/distribution-validation/current-session.json -Raw | ConvertFrom-Json
if ($ProcessId -ne $session.pid) { throw 'Only the recorded validation process may be controlled' }
$process=Get-Process -Id $ProcessId
if ($process.Path -ne $session.executable) { throw 'Validation process identity changed' }
[uint32]$ownerId=0
[void][QuotisUiaOwner]::GetWindowThreadProcessId([IntPtr]$WindowHandle,[ref]$ownerId)
if ($ownerId -ne $ProcessId) { throw 'Window does not belong to the validation process' }
if ($Action -ne 'Inspect' -and $Name -match 'autostart|start with|login|on startup') { throw 'Autostart/OS registration changes are excluded from this host validation' }
$root=[System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$WindowHandle)
$condition=[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,$Name)
$matches=$root.FindAll([System.Windows.Automation.TreeScope]::Descendants,$condition)
if ($ControlType) { $matches=@($matches | Where-Object { $_.Current.ControlType.ProgrammaticName -eq "ControlType.$ControlType" }) }
if ($matches.Count -ne 1) { throw "Expected exactly one control named '$Name', found $($matches.Count)" }
$element=$matches[0]
$patterns=@($element.GetSupportedPatterns() | ForEach-Object ProgrammaticName)
switch ($Action) {
    'Invoke' { $element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke() }
    'Expand' { $element.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Expand() }
    'Collapse' { $element.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Collapse() }
    'Toggle' { $element.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle() }
    'SetValue' { $element.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($Value) }
    'Select' { $element.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select() }
}
$result=[ordered]@{timestamp=[DateTime]::UtcNow.ToString('o');processId=$ProcessId;windowHandle=$WindowHandle;name=$Name;action=$Action;patterns=$patterns;providerCallReturned=$true;verification='requires fresh accessibility/disk observation; no global input fallback'}
$result | ConvertTo-Json -Compress | Add-Content -LiteralPath (Join-Path $session.root 'uia-actions.jsonl')
$result | ConvertTo-Json -Compress

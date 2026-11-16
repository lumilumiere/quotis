$ErrorActionPreference='Stop'
$session=Get-Content target/distribution-validation/current-session.json -Raw | ConvertFrom-Json
$workspace=(Resolve-Path '.').Path
if (-not $session.root.StartsWith($workspace+'\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Expected repository fixture profile' }
if (Get-Process -Id $session.pid -ErrorAction SilentlyContinue) { throw 'Complete the native interaction check before restart' }
if ((Get-FileHash -LiteralPath $session.executable).Hash -ne $session.sha256) { throw 'Fixture executable changed' }
foreach ($hive in @('HKCU:','HKLM:')) {
    $key=Get-Item -LiteralPath "$hive/Software/Microsoft/Windows/CurrentVersion/Run" -ErrorAction SilentlyContinue
    if ($key -and $key.GetValueNames().Contains('Quotis')) { throw 'Use VM for existing autostart registration' }
}
$settings=Get-Content (Join-Path $session.profile 'settings.committed.json') -Raw | ConvertFrom-Json
if ($settings.autostart) { throw 'Autostart changes excluded' }
foreach ($provider in $settings.providers.PSObject.Properties) {
    if (-not ([IO.Path]::GetFullPath($provider.Value.dir)).StartsWith($session.root+'\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Provider outside fixture root' }
    if ($provider.Value.enabled -and $provider.Name -ne 'gemini') { throw 'Only synthetic local Gemini source allowed' }
}
$psi=[Diagnostics.ProcessStartInfo]::new($session.executable)
$psi.UseShellExecute=$false
$psi.CreateNoWindow=$true
$psi.WindowStyle='Hidden'
foreach ($name in @('HOME','USERPROFILE','APPDATA','LOCALAPPDATA','XDG_CONFIG_HOME','CLAUDE_CONFIG_DIR','CODEX_HOME')) { $psi.Environment[$name]=Join-Path $session.root "environment/$name" }
$psi.Environment['QUOTIS_DATA_DIR']=$session.profile
$psi.Environment['WEBVIEW2_USER_DATA_FOLDER']=Join-Path $session.root 'webview'
foreach ($name in @('OPENROUTER_API_KEY','DEEPSEEK_API_KEY','WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS')) { [void]$psi.Environment.Remove($name) }
$started=[DateTime]::UtcNow
$process=[Diagnostics.Process]::Start($psi)
try {
    $deadline=$started.AddSeconds(15)
    do {
        Start-Sleep -Milliseconds 100
        $process.Refresh()
        $snapshot=Get-Content (Join-Path $session.profile 'snapshot.json') -Raw | ConvertFrom-Json
        $gemini=$snapshot.providers | Where-Object id -eq 'gemini'
        $ready=$process.MainWindowHandle -ne 0 -and $gemini.metrics -and $gemini.metrics[0].used -eq 42 -and (Get-Item (Join-Path $session.profile 'snapshot.json')).LastWriteTimeUtc -ge $started
    } while (-not $ready -and -not $process.HasExited -and [DateTime]::UtcNow -lt $deadline)
    if (-not $ready) { throw 'Restart did not produce native window and fresh fixture snapshot' }
    if (-not $process.CloseMainWindow() -or -not $process.WaitForExit(15000) -or $process.ExitCode -ne 0) { throw 'Restart shutdown failed' }
    $loaded=Get-Content (Join-Path $session.profile 'settings.committed.json') -Raw | ConvertFrom-Json
    if (-not ($loaded.glass -and $loaded.hover_expand -and $loaded.always_on_top -and $loaded.providers.gemini.enabled -and $loaded.mode -eq 'island')) { throw 'Settings lost across restart' }
    @{source=$session.sourceCommit;timestamp=[DateTime]::UtcNow.ToString('o');binarySha256=$session.sha256;startupWindowAndCollection='PASS';persistedFields=@('glass','hover_expand','always_on_top','gemini.enabled','mode');fixtureTokens=42;exitCode=$process.ExitCode;globalInputUsed=$false} | ConvertTo-Json -Depth 5 | Set-Content target/distribution-validation/restart.json
} finally {
    if (-not $process.HasExited) { [void]$process.CloseMainWindow(); if (-not $process.WaitForExit(5000)) { $process.Kill(); [void]$process.WaitForExit(5000) } }
}
$before=@{}
foreach ($name in @('settings.json','settings.json.bak','settings.committed.json','snapshot.json')) { $before[$name]=(Get-FileHash (Join-Path $session.profile $name)).Hash }
$psi.FileName=Join-Path $session.root 'portable/quotis-cli.exe'
$psi.RedirectStandardOutput=$true
$psi.RedirectStandardError=$true
foreach ($arg in @('status','--live','--offline','--json')) { $psi.ArgumentList.Add($arg) }
$cli=[Diagnostics.Process]::Start($psi)
$output=$cli.StandardOutput.ReadToEndAsync()
$errors=$cli.StandardError.ReadToEndAsync()
if (-not $cli.WaitForExit(15000)) { $cli.Kill(); throw 'Synthetic CLI smoke timed out' }
$unchanged=$true
foreach ($name in $before.Keys) { if ($before[$name] -ne (Get-FileHash (Join-Path $session.profile $name)).Hash) { $unchanged=$false } }
@{source=$session.sourceCommit;timestamp=[DateTime]::UtcNow.ToString('o');command='quotis-cli status --live --offline --json';scope='synthetic local fixture only; no account comparison';exitCode=$cli.ExitCode;stateFilesUnchanged=$unchanged;stdout=($output.Result | ConvertFrom-Json);stderr=$errors.Result} | ConvertTo-Json -Depth 15 | Set-Content target/distribution-validation/cli-read-only.json
if ($cli.ExitCode -ne 0 -or -not $unchanged) { throw 'CLI state/exit invariant failed' }
Write-Output 'PASS restart with persisted settings, native normal close and read-only synthetic live/offline CLI'

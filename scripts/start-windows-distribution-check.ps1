param([Parameter(Mandatory=$true)][ValidatePattern('^[a-f0-9]{40}$')][string]$SourceCommit, [string]$EvidenceRoot = 'target/distribution-validation', [string]$Executable = 'target/release/quotis.exe')
$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path '.').Path
$evidenceBase = [IO.Path]::GetFullPath((Join-Path $workspace $EvidenceRoot))
if (-not $evidenceBase.StartsWith($workspace + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Evidence directory must be inside this checkout' }
foreach ($hive in @('HKCU:', 'HKLM:')) {
    $runKey = "$hive\Software\Microsoft\Windows\CurrentVersion\Run"
    $key = Get-Item -LiteralPath $runKey -ErrorAction SilentlyContinue
    if ($key -and $key.GetValueNames().Contains('Quotis')) { throw 'Existing Quotis autostart registration: use a VM instead of this host test' }
}
$root = Join-Path $evidenceBase ([DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0,8))
New-Item -ItemType Directory -Path $root | Out-Null
$portable = Join-Path $root 'portable'
New-Item -ItemType Directory -Path $portable | Out-Null
$sourceExecutable=(Resolve-Path -LiteralPath $Executable).Path
if (-not $sourceExecutable.StartsWith($workspace + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Use a generated executable inside this checkout' }
Copy-Item -LiteralPath $sourceExecutable -Destination (Join-Path $portable 'quotis.exe')
Copy-Item -LiteralPath target/release/quotis-cli.exe -Destination $portable
$profileDir = Join-Path $root 'profile'
New-Item -ItemType Directory -Path $profileDir | Out-Null
$providers = @{}
foreach ($id in @('claude','codex','copilot','cursor','gemini','qwen','opencode','openrouter','deepseek')) {
    $source = Join-Path $root "sources/$id"
    New-Item -ItemType Directory -Path $source -Force | Out-Null
    $providers[$id] = @{enabled=$false; dir=$source}
}
New-Item -ItemType Directory -Path (Join-Path $providers.gemini.dir 'tmp') | Out-Null
[IO.File]::WriteAllText((Join-Path $providers.gemini.dir 'tmp/session.jsonl'), ((@{timestamp=[DateTime]::UtcNow.ToString('o');tokens=@{total=42}} | ConvertTo-Json -Compress) + "`n"))
@{providers=$providers;always_on_top=$false;autostart=$false;mode='island';theme='dark';position=@(500,200)} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $profileDir 'settings.json')
$owner = Get-ItemProperty -LiteralPath 'HKCU:/Software/Microsoft/Windows/CurrentVersion/Uninstall/Quotis' -ErrorAction SilentlyContinue
$ownerExe = if ($owner) { Join-Path $owner.InstallLocation.Trim('"') 'quotis.exe' } else { $null }
$before = @{displayVersion=$owner.DisplayVersion; executableHash=if ($ownerExe -and (Test-Path -LiteralPath $ownerExe)) { (Get-FileHash -LiteralPath $ownerExe -Algorithm SHA256).Hash } else { $null }}
$psi = [Diagnostics.ProcessStartInfo]::new((Join-Path $portable 'quotis.exe'))
$psi.UseShellExecute = $false
$psi.CreateNoWindow = $true
$psi.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
foreach ($name in @('HOME','USERPROFILE','APPDATA','LOCALAPPDATA','XDG_CONFIG_HOME','CLAUDE_CONFIG_DIR','CODEX_HOME')) {
    $dir = Join-Path $root "environment/$name"
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
    $psi.Environment[$name] = $dir
}
$psi.Environment['QUOTIS_DATA_DIR'] = $profileDir
$psi.Environment['WEBVIEW2_USER_DATA_FOLDER'] = Join-Path $root 'webview'
[void]$psi.Environment.Remove('OPENROUTER_API_KEY')
[void]$psi.Environment.Remove('DEEPSEEK_API_KEY')
[void]$psi.Environment.Remove('WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS')
$app = [Diagnostics.Process]::Start($psi)
$state = [ordered]@{sourceCommit=$SourceCommit;root=$root;pid=$app.Id;profile=$profileDir;executable=(Join-Path $portable 'quotis.exe');sha256=(Get-FileHash -LiteralPath (Join-Path $portable 'quotis.exe') -Algorithm SHA256).Hash;ownerBefore=$before;startedAt=[DateTime]::UtcNow.ToString('o')}
$state | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $root 'session.json')
$state | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $evidenceBase 'current-session.json')
$state | ConvertTo-Json -Depth 5 -Compress

param([string]$Executable = 'target/release/quotis.exe', [int]$SampleSeconds = 60)
$ErrorActionPreference = 'Stop'
if (-not $env:QUOTIS_DATA_DIR -or $env:QUOTIS_DATA_DIR -notlike '*quotis-hardening-*') { throw 'Run through scripts/with-fixtures.ps1' }
$exePath = (Resolve-Path -LiteralPath $Executable).Path
$artifactRoot = Join-Path $env:QUOTIS_DATA_DIR 'runtime-evidence'
New-Item -ItemType Directory -Path $artifactRoot | Out-Null
$sourceDir = Join-Path $artifactRoot 'gemini'
New-Item -ItemType Directory -Path (Join-Path $sourceDir 'tmp') | Out-Null
$logFile = Join-Path $sourceDir 'tmp/session.jsonl'
$record = @{ timestamp=[DateTime]::UtcNow.ToString('o'); tokens=@{total=42} } | ConvertTo-Json -Compress
[IO.File]::WriteAllText($logFile, $record + "`n")
$providerSettings = @{}
foreach ($id in @('claude','codex','copilot','cursor','gemini','qwen','opencode','openrouter','deepseek')) {
    $providerSettings[$id] = @{enabled=($id -eq 'gemini'); dir=$sourceDir}
}
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class QuotisRuntimeWindow { [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int height, uint flags); }'
function Read-Tree([int]$RootId) {
    $all = Get-CimInstance Win32_Process
    $ids = [Collections.Generic.HashSet[int]]::new()
    [void]$ids.Add($RootId)
    do {
        $before = $ids.Count
        foreach ($item in $all) { if ($ids.Contains([int]$item.ParentProcessId)) { [void]$ids.Add([int]$item.ProcessId) } }
    } while ($before -ne $ids.Count)
    @(Get-Process -Id @($ids) -ErrorAction SilentlyContinue | Select-Object Id,ProcessName,CPU,PrivateMemorySize64,WorkingSet64)
}
function Cpu-Seconds($Tree) { ($Tree | Measure-Object CPU -Sum).Sum }
$runs = @()
$appProcess = $null
try {
    foreach ($mode in @('island','notch','panel')) {
        [IO.File]::WriteAllText($logFile, $record + "`n")
        # Each launch has a new settings directory: it cannot affect an installed Quotis.
        $runDir = Join-Path $artifactRoot $mode
        New-Item -ItemType Directory -Path $runDir | Out-Null
        $env:QUOTIS_DATA_DIR = $runDir
        $settings = @{providers=$providerSettings; mode=$mode; always_on_top=$false; autostart=$false}
        [IO.File]::WriteAllText((Join-Path $runDir 'settings.json'), ($settings | ConvertTo-Json -Depth 5))
        $started = [Diagnostics.Stopwatch]::StartNew()
        $appProcess = Start-Process -FilePath $exePath -PassThru -WindowStyle Hidden -RedirectStandardError (Join-Path $runDir 'stderr.txt')
        $snapshotPath = Join-Path $runDir 'snapshot.json'
        while (-not (Test-Path -LiteralPath $snapshotPath)) {
            if ($appProcess.HasExited -or $started.Elapsed.TotalSeconds -gt 25) { throw "$mode startup did not produce snapshot" }
            Start-Sleep -Milliseconds 25
        }
        $startupMs = $started.ElapsedMilliseconds
        $snap = Get-Content -LiteralPath $snapshotPath -Raw | ConvertFrom-Json
        $observed = @($snap.providers | Where-Object id -eq 'gemini')[0]
        if ($observed.metrics[0].used -ne 42) { throw 'fixture collection mismatch' }
        $entry = [ordered]@{ mode=$mode; startupToSnapshotMs=$startupMs; fixtureTokens=42; checks=@('startup','fixture snapshot') }
        if ($mode -eq 'island') {
            Start-Sleep -Seconds 20
            $first = Read-Tree $appProcess.Id
            $sampleTimer = [Diagnostics.Stopwatch]::StartNew()
            Write-Output "Sampling final production process tree for $SampleSeconds seconds; artifacts $artifactRoot"
            Start-Sleep -Seconds $SampleSeconds
            $second = Read-Tree $appProcess.Id
            $elapsed = $sampleTimer.Elapsed.TotalSeconds
            $entry.idle = @{seconds=$elapsed; cpuSeconds=((Cpu-Seconds $second)-(Cpu-Seconds $first)); cpuOneCorePercent=(100*((Cpu-Seconds $second)-(Cpu-Seconds $first))/$elapsed); privateMB=(($second|Measure-Object PrivateMemorySize64 -Sum).Sum/1MB); workingSetMB=(($second|Measure-Object WorkingSet64 -Sum).Sum/1MB); processes=$second}
            $beforeRefresh = Read-Tree $appProcess.Id
            [IO.File]::AppendAllText($logFile, $record + "`n")
            Start-Sleep -Seconds 3
            $afterRefresh = Read-Tree $appProcess.Id
            $entry.refreshCpuSeconds = (Cpu-Seconds $afterRefresh)-(Cpu-Seconds $beforeRefresh)
            $blockedFile = Join-Path $sourceDir 'tmp/blocked.jsonl'
            $fileLock = [IO.File]::Open($blockedFile, [IO.FileMode]::CreateNew, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
            try {
                $blockedBytes = [Text.Encoding]::UTF8.GetBytes($record + "`n")
                $fileLock.Write($blockedBytes, 0, $blockedBytes.Length)
                $fileLock.Flush()
                Start-Sleep -Seconds 18
                $afterFailure = Read-Tree $appProcess.Id
                $entry.failureSample = @{ privateMB=(($afterFailure|Measure-Object PrivateMemorySize64 -Sum).Sum/1MB); cpuSeconds=((Cpu-Seconds $afterFailure)-(Cpu-Seconds $afterRefresh)); snapshot=(Get-Content -LiteralPath $snapshotPath -Raw | ConvertFrom-Json) }
            } finally {
                $fileLock.Dispose()
                Rename-Item -LiteralPath $blockedFile -NewName 'blocked.evidence'
            }
        }
        Start-Sleep -Milliseconds 650
        $appProcess.Refresh()
        if ($appProcess.MainWindowHandle -eq 0) { throw "$mode has no target window handle" }
        if ($mode -ne 'notch') {
            if (-not [QuotisRuntimeWindow]::SetWindowPos($appProcess.MainWindowHandle,[IntPtr]::Zero,70,110,0,0,0x15)) { throw 'scoped move failed' }
            Start-Sleep -Milliseconds 150
        }
        if (-not $appProcess.CloseMainWindow()) { throw 'scoped normal close failed' }
        if (-not $appProcess.WaitForExit(15000)) { throw 'normal shutdown timed out' }
        $entry.exitCode = $appProcess.ExitCode
        if ($entry.exitCode -ne 0) { throw 'nonzero desktop exit' }
        $loaded = Get-Content -LiteralPath (Join-Path $runDir 'settings.json') -Raw | ConvertFrom-Json
        if ($loaded.mode -ne $mode) { throw 'mode did not persist' }
        if ($mode -ne 'notch' -and $loaded.position[1] -ne 110) { throw 'immediate normal exit lost move' }
        $entry.position = $loaded.position
        $entry.checks += 'normal shutdown and persistence'
        $runs += $entry
        $appProcess = $null
    }
} finally {
    if ($null -ne $entry -and -not ($runs | Where-Object { $_.mode -eq $entry.mode })) { $runs += $entry }
    if ($null -ne $appProcess -and -not $appProcess.HasExited) { Stop-Process -Id $appProcess.Id }
    $result = @{executable=$exePath; sha256=(Get-FileHash -Algorithm SHA256 -LiteralPath $exePath).Hash; runs=$runs; artifactRoot=$artifactRoot; timestamp=[DateTime]::UtcNow.ToString('o')}
    $result | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $artifactRoot 'report.json')
    Write-Output ($result | Select-Object executable,sha256,timestamp,artifactRoot | ConvertTo-Json -Compress)
    Write-Output ('Completed mode checks: ' + $runs.Count + '; full measurements are in report.json')
}

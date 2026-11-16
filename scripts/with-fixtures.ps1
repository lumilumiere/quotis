param([Parameter(Mandatory = $true)][scriptblock]$Command)
$ErrorActionPreference = 'Stop'
$fixtureRoot = Join-Path ([IO.Path]::GetTempPath()) ('quotis-hardening-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $fixtureRoot | Out-Null
if (-not $env:CARGO_HOME) { $env:CARGO_HOME = Join-Path $env:USERPROFILE '.cargo' }
if (-not $env:RUSTUP_HOME) { $env:RUSTUP_HOME = Join-Path $env:USERPROFILE '.rustup' }
foreach ($name in @('HOME','USERPROFILE','APPDATA','LOCALAPPDATA','XDG_CONFIG_HOME','CLAUDE_CONFIG_DIR','CODEX_HOME','QUOTIS_DATA_DIR')) {
    $location = Join-Path $fixtureRoot $name
    New-Item -ItemType Directory -Path $location | Out-Null
    [Environment]::SetEnvironmentVariable($name, $location, 'Process')
}
foreach ($name in @('OPENROUTER_API_KEY','DEEPSEEK_API_KEY')) {
    [Environment]::SetEnvironmentVariable($name, $null, 'Process')
}
& $Command
if ($null -ne $LASTEXITCODE) { exit $LASTEXITCODE }

param(
    [string]$Workspace = "",
    [switch]$KeepWorkspace
)

$ErrorActionPreference = "Stop"

if (-not $Workspace) {
    $Workspace = Join-Path $env:TEMP ("rango-smoke-" + [System.Guid]::NewGuid().ToString("N"))
}

Write-Host "== Rango Smoke E2E =="
Write-Host "Workspace: $Workspace"

if (Test-Path $Workspace) {
    Remove-Item -Recurse -Force $Workspace
}
New-Item -ItemType Directory -Path $Workspace | Out-Null

$input = Join-Path $Workspace "input.jsonl"
$output = Join-Path $Workspace "output.jsonl"

 $lines = @(
    "{`"name`":`"ana`",`"age`":31}",
    "{`"name`":`"luis`",`"age`":28}",
    "{`"name`":`"zoe`",`"age`":35}"
)
[System.IO.File]::WriteAllLines($input, $lines, [System.Text.UTF8Encoding]::new($false))

cargo run -q -p rango-cli -- init $Workspace
cargo run -q -p rango-cli -- import --path $Workspace --collection users $input
cargo run -q -p rango-cli -- export --path $Workspace --collection users --output $output
cargo run -q -p rango-cli -- doctor $Workspace

if (-not (Test-Path (Join-Path $Workspace "data.redb"))) {
    throw "Missing persistent storage file data.redb"
}
if (-not (Test-Path (Join-Path $Workspace "oplog.rgo"))) {
    throw "Missing oplog.rgo"
}
if (-not (Test-Path $output)) {
    throw "Missing export output file"
}

$lineCount = (Get-Content -Path $output | Measure-Object -Line).Lines
if ($lineCount -lt 3) {
    throw "Expected at least 3 exported lines, got $lineCount"
}

Write-Host "Smoke test passed. Exported lines: $lineCount"

if (-not $KeepWorkspace) {
    Remove-Item -Recurse -Force $Workspace
    Write-Host "Workspace cleaned."
} else {
    Write-Host "Workspace preserved: $Workspace"
}

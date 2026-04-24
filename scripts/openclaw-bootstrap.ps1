param(
    [string]$WorkspacePath = ".rango-openclaw",
    [string]$Namespace = "openclaw",
    [string]$TenantId = "default",
    [string]$NodeId = "openclaw-node",
    [string]$ServerUrl = "",
    [string]$SyncToken = "",
    [string]$Passphrase = ""
)

$ErrorActionPreference = "Stop"

function Invoke-Rango {
    param(
        [string[]]$CommandArgs
    )

    $rango = Get-Command rango -ErrorAction SilentlyContinue
    if ($null -ne $rango) {
        & $rango.Source @CommandArgs
        return
    }

    Write-Host "rango not found in PATH, falling back to cargo run -p rango-cli -- ..."
    & cargo run -q -p rango-cli -- @CommandArgs
}

Write-Host "== Rango OpenClaw bootstrap =="
Write-Host "workspace: $WorkspacePath"

if (-not (Test-Path $WorkspacePath)) {
    New-Item -ItemType Directory -Path $WorkspacePath | Out-Null
}

$initArgs = @("init", $WorkspacePath)
if ($Passphrase -ne "") {
    $initArgs += @("--passphrase", $Passphrase)
}

Invoke-Rango -CommandArgs $initArgs
Invoke-Rango -CommandArgs @("inspect", $WorkspacePath)

$doctorArgs = @("doctor", $WorkspacePath)
if ($Passphrase -ne "") {
    $doctorArgs += @("--passphrase", $Passphrase)
}
Invoke-Rango -CommandArgs $doctorArgs

if ($ServerUrl -ne "" -and $SyncToken -ne "") {
    $syncArgs = @("sync", $WorkspacePath, "--server", $ServerUrl, "--token", $SyncToken, "--node-id", $NodeId)
    if ($Passphrase -ne "") {
        $syncArgs += @("--passphrase", $Passphrase)
    }
    Invoke-Rango -CommandArgs $syncArgs
    Write-Host "initial sync done"
}
elseif ($ServerUrl -ne "" -or $SyncToken -ne "") {
    Write-Warning "ServerUrl and SyncToken must both be provided for sync; skipping sync."
}

$envFile = Join-Path $WorkspacePath ".env.rango"
$envLines = @(
    "RANGO_PATH=$WorkspacePath",
    "RANGO_NAMESPACE=$Namespace",
    "RANGO_TENANT=$TenantId",
    "RANGO_NODE_ID=$NodeId"
)

if ($ServerUrl -ne "") {
    $envLines += "RANGO_SYNC_URL=$ServerUrl"
}
if ($SyncToken -ne "") {
    $envLines += "RANGO_SYNC_TOKEN=$SyncToken"
}

Set-Content -Path $envFile -Value ($envLines -join [Environment]::NewLine) -Encoding UTF8

Write-Host ""
Write-Host "Bootstrap complete."
Write-Host "Env file: $envFile"
Write-Host "Load it in your product runtime and map memory writes/reads to the contract:"
Write-Host "docs/integrations/openclaw-memory-contract.json"

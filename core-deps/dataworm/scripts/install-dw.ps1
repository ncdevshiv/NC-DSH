<#
.SYNOPSIS
  Install DataWorm globally as two commands: `dataworm` and `dw` (via uv tool).

.DESCRIPTION
  Requires uv (https://docs.astral.sh/uv/). Runs
      uv tool install --force --editable <Source>
  so %USERPROFILE%\.local\bin gets both `dataworm.exe` and `dw.exe`, then
  records the resolved absolute Source into %USERPROFILE%\.dataworm\source.txt
  (that marker file is what `dw up` reinstalls from).

.EXAMPLE
  pwsh -File scripts\install-dw.ps1                  # Source = repo root (parent of scripts\)
  pwsh -File scripts\install-dw.ps1 D:\code\dataworm # explicit checkout
#>
param(
    [string]$Source = (Split-Path $PSScriptRoot -Parent)
)

$ErrorActionPreference = "Stop"

# --- resolve + validate the source checkout ---------------------------------
try {
    $Source = (Resolve-Path -LiteralPath $Source).Path
} catch {
    Write-Host "ERROR: source path '$Source' does not exist." -ForegroundColor Red
    exit 1
}
if (-not (Test-Path -LiteralPath (Join-Path $Source "pyproject.toml"))) {
    Write-Host "ERROR: '$Source' has no pyproject.toml - not a DataWorm checkout." -ForegroundColor Red
    exit 1
}

# --- require uv ---------------------------------------------------------------
$uvCmd = Get-Command uv -ErrorAction SilentlyContinue
if (-not $uvCmd) {
    Write-Host "ERROR: 'uv' was not found on PATH." -ForegroundColor Red
    Write-Host "Install it first:"
    Write-Host "  powershell -ExecutionPolicy ByPass -c `"irm https://astral.sh/uv/install.ps1 | iex`""
    exit 1
}

Write-Host "Installing DataWorm globally (editable from: $Source)"
& $uvCmd.Source tool install --force --editable $Source
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: uv tool install failed (exit code $LASTEXITCODE)." -ForegroundColor Red
    exit $LASTEXITCODE
}

# --- record the install source (what `dw up` reinstalls from) -----------------
$markerDir = Join-Path $env:USERPROFILE ".dataworm"
New-Item -ItemType Directory -Force -Path $markerDir | Out-Null
$marker = Join-Path $markerDir "source.txt"
# UTF-8 without BOM so Python's open(encoding="utf-8") reads it cleanly.
[System.IO.File]::WriteAllText($marker, $Source, [System.Text.UTF8Encoding]::new($false))
Write-Host "Recorded install source -> $marker"

# --- PATH check + usage -------------------------------------------------------
$toolBin = Join-Path $env:USERPROFILE ".local\bin"
$onPath = ($env:PATH -split ';') -contains $toolBin

Write-Host ""
Write-Host "Installed. Two global commands are now available:" -ForegroundColor Green
Write-Host ""
Write-Host "  dw           # bare summon: crawl THIS directory once, start watching it,"
Write-Host "               # ensure the daemon, open the dashboard"
Write-Host "  dataworm     # identical twin of dw"
Write-Host "  dw status    # daemon liveness + watched roots"
Write-Host "  dw stop      # shut the daemon down"
Write-Host "  dw up        # self-update: reinstall the latest build from $Source"
Write-Host ""
if (-not $onPath) {
    Write-Warning "$toolBin is NOT on your PATH."
    Write-Warning "Open a new shell, or add it now:"
    Write-Warning "  [Environment]::SetEnvironmentVariable('Path', `$env:PATH + ';$toolBin', 'User')"
}

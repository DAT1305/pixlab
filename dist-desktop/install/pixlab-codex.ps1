param(
  [string]$CodexHome = $(if ($env:CODEX_HOME) { $env:CODEX_HOME } else { Join-Path $HOME ".codex-cli-alt" })
)

$ErrorActionPreference = "Stop"

function Write-Info($message) {
  Write-Host "[PixLab Codex] $message"
}

function Ensure-Directory([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path)) {
    New-Item -ItemType Directory -Path $Path -Force | Out-Null
  }
}

function Remove-TomlBlock([string]$content, [string]$headerPattern) {
  $escapedHeader = [regex]::Escape($headerPattern)
  $pattern = "(?ms)^$escapedHeader\s*\r?\n(?:.*(?:\r?\n|$))*?(?=^\[|^\[\[|\z)"
  return [regex]::Replace($content, $pattern, "")
}

$zipUrl = "https://mafiler.com/downloads/pixlab-codex.zip"
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("pixlab-codex-install-" + [guid]::NewGuid().ToString("N"))
$zipPath = Join-Path $tempRoot "pixlab-codex.zip"
$extractPath = Join-Path $tempRoot "extract"

Ensure-Directory $tempRoot
Ensure-Directory $extractPath

Write-Info "Downloading plugin package..."
Invoke-WebRequest -Uri $zipUrl -OutFile $zipPath

Write-Info "Extracting plugin package..."
Expand-Archive -LiteralPath $zipPath -DestinationPath $extractPath -Force

$pluginSource = Join-Path $extractPath "pixlab-codex"
if (-not (Test-Path -LiteralPath $pluginSource)) {
  if (Test-Path -LiteralPath (Join-Path $extractPath ".codex-plugin")) {
    $pluginSource = $extractPath
  } else {
    throw "Installer archive is invalid. Missing pixlab-codex plugin contents."
  }
}

$marketplaceRoot = Join-Path $CodexHome "marketplaces\\pixlab"
$pluginDest = Join-Path $marketplaceRoot "plugins\\pixlab-codex"
$marketplaceDir = Join-Path $marketplaceRoot ".agents\\plugins"
$marketplaceFile = Join-Path $marketplaceDir "marketplace.json"
$configPath = Join-Path $CodexHome "config.toml"

Ensure-Directory $marketplaceRoot
Ensure-Directory (Split-Path -Parent $pluginDest)
Ensure-Directory $marketplaceDir

if (Test-Path -LiteralPath $pluginDest) {
  Remove-Item -LiteralPath $pluginDest -Recurse -Force
}

Write-Info "Installing plugin files..."
Ensure-Directory $pluginDest
Get-ChildItem -LiteralPath $pluginSource -Force | ForEach-Object {
  Copy-Item -LiteralPath $_.FullName -Destination $pluginDest -Recurse -Force
}

$marketplaceJson = @'
{
  "name": "pixlab",
  "interface": {
    "displayName": "PixLab"
  },
  "plugins": [
    {
      "name": "pixlab-codex",
      "source": {
        "source": "local",
        "path": "./plugins/pixlab-codex"
      },
      "policy": {
        "installation": "AVAILABLE",
        "authentication": "ON_INSTALL"
      },
      "category": "Coding"
    }
  ]
}
'@

Set-Content -LiteralPath $marketplaceFile -Value $marketplaceJson -Encoding UTF8

if (-not (Test-Path -LiteralPath $configPath)) {
  throw "Codex config.toml was not found at $configPath"
}

Write-Info "Updating Codex config..."
$config = Get-Content -LiteralPath $configPath -Raw
$config = Remove-TomlBlock $config '[plugins."pixlab-codex@pixlab-local"]'
$config = Remove-TomlBlock $config '[plugins."pixlab-codex@pixlab"]'
$config = Remove-TomlBlock $config '[marketplaces.pixlab-local]'
$config = Remove-TomlBlock $config '[marketplaces.pixlab]'

$marketplaceSource = "\\?\$marketplaceRoot"
$pluginBlock = @"

[plugins."pixlab-codex@pixlab"]
enabled = true
"@

$marketplaceBlock = @"

[marketplaces.pixlab]
last_updated = "$(Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ")"
source_type = "local"
source = '$marketplaceSource'
"@

if ($config -notmatch '(?m)^\[features\]\s*$') {
  $config += @"

[features]
apps = true
multi_agent = true
"@
}

$config = $config.TrimEnd() + $pluginBlock + $marketplaceBlock + "`r`n"
Set-Content -LiteralPath $configPath -Value $config -Encoding UTF8

Write-Info "Done."
Write-Info "Restart Codex desktop, then enable PixLab Codex from the plugin picker."

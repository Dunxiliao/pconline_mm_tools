$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $scriptDir

$env:APP_ENV = "prod"
$env:CARGO_TARGET_DIR = Join-Path $scriptDir "src-tauri\target-prod"

if (-not (Test-Path (Join-Path $scriptDir "env\.env.prod"))) {
  throw "Missing env/.env.prod"
}

# 可选：签名（需要 .tauri/pconlineinvoice.key 存在）
if (Test-Path ".tauri/pconlineinvoice.key") {
  $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw ".tauri/pconlineinvoice.key"
  if ([string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD)) {
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "Admin123pconline"
  }
}

npm run tauri build

$tauriConfPath = Join-Path $scriptDir "src-tauri\tauri.conf.json"
if (-not (Test-Path $tauriConfPath)) {
  throw "Missing src-tauri/tauri.conf.json"
}
$tauriConf = Get-Content -Raw $tauriConfPath | ConvertFrom-Json
$version = [string]$tauriConf.version
if ([string]::IsNullOrWhiteSpace($version)) {
  throw "tauri.conf.json missing version"
}

$msiDir = Join-Path $scriptDir "src-tauri\target-prod\release\bundle\msi"
$nsisDir = Join-Path $scriptDir "src-tauri\target-prod\release\bundle\nsis"

$installer = $null
if (Test-Path $msiDir) {
  $installer = Get-ChildItem -Path $msiDir -File | Where-Object { $_.Name -like "*.msi" } | Sort-Object LastWriteTime -Descending | Select-Object -First 1
}
if (-not $installer -and (Test-Path $nsisDir)) {
  $installer = Get-ChildItem -Path $nsisDir -File | Where-Object { $_.Name -like "*setup.exe" } | Sort-Object LastWriteTime -Descending | Select-Object -First 1
}
if (-not $installer) {
  throw "Cannot find installer (*.msi or *setup.exe) in bundle output"
}
$sigPath = "$($installer.FullName).sig"
if (-not (Test-Path $sigPath)) {
  throw "Cannot find signature file: $sigPath"
}
$signature = (Get-Content -Raw $sigPath).Trim()
if ([string]::IsNullOrWhiteSpace($signature)) {
  throw "Signature file is empty: $sigPath"
}

# Priority:
# 1) UPDATE_INSTALLER_URL_TEMPLATE (e.g. https://github.com/owner/repo/releases/download/v{version}/{file})
# 2) UPDATE_INSTALLER_BASE_URL (e.g. https://cdn.example.com/releases/v{version})
# 3) derive from updater endpoint
$installerUrlTemplate = $env:UPDATE_INSTALLER_URL_TEMPLATE
$installerBaseUrl = $env:UPDATE_INSTALLER_BASE_URL
$firstEndpoint = $null
if ([string]::IsNullOrWhiteSpace($installerBaseUrl)) {
  if ($tauriConf.plugins -and $tauriConf.plugins.updater -and $tauriConf.plugins.updater.endpoints) {
    $firstEndpoint = [string]($tauriConf.plugins.updater.endpoints | Select-Object -First 1)
  }
}
# if ([string]::IsNullOrWhiteSpace($installerUrlTemplate) -and [string]::IsNullOrWhiteSpace($installerBaseUrl) -and -not [string]::IsNullOrWhiteSpace($firstEndpoint)) {
#   if ($firstEndpoint -match "^https://raw\.githubusercontent\.com/([^/]+)/([^/]+)/refs/heads/[^/]+/latest\.json$") {
#     $owner = $matches[1]
#     $repo = $matches[2]
#     $installerUrlTemplate = "https://github.com/$owner/$repo/releases/download/v{version}/{file}"
#   } else {
#     $installerBaseUrl = $firstEndpoint -replace "/latest\.json$", ""
#   }
# }
$installerUrlTemplate = "https://github.com/Dunxiliao/pconline_tools/releases/download/v{version}/{file}"

if (-not [string]::IsNullOrWhiteSpace($installerUrlTemplate)) {
  $installerUrl = $installerUrlTemplate.Replace("{version}", $version).Replace("{file}", $installer.Name)
} else {
  if ([string]::IsNullOrWhiteSpace($installerBaseUrl)) {
    throw "Missing installer URL source. Set UPDATE_INSTALLER_URL_TEMPLATE or UPDATE_INSTALLER_BASE_URL, or configure updater endpoint ending with /latest.json"
  }
  $installerBaseUrl = $installerBaseUrl.TrimEnd("/")
  $installerUrl = "$installerBaseUrl/$($installer.Name)"
}

$latest = [ordered]@{
  version = $version
  notes = $(if ([string]::IsNullOrWhiteSpace($env:UPDATE_NOTES)) { "Release $version" } else { $env:UPDATE_NOTES })
  pub_date = [DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ")
  platforms = [ordered]@{
    "windows-x86_64" = [ordered]@{
      signature = $signature
      url = $installerUrl
    }
  }
}

$latestJsonPath = Join-Path $scriptDir "latest.json"
$latest | ConvertTo-Json -Depth 10 | Set-Content -Path $latestJsonPath -Encoding UTF8

Write-Host "Generated latest.json: $latestJsonPath"
Write-Host "Installer: $($installer.Name)"
Write-Host "Version: $version"


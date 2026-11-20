$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$prodEnvPath = Join-Path $scriptDir "env\.env.prod"

if (-not (Test-Path -LiteralPath $prodEnvPath)) {
  throw "Missing env/.env.prod; refusing to start the prod environment."
}

$previousLocation = Get-Location
$previousAppEnv = [Environment]::GetEnvironmentVariable("APP_ENV", "Process")
$previousTargetDir = [Environment]::GetEnvironmentVariable("CARGO_TARGET_DIR", "Process")
$exitCode = 0

try {
  Set-Location $scriptDir
  $env:APP_ENV = "prod"
  $env:CARGO_TARGET_DIR = Join-Path $scriptDir "src-tauri\target-test"

  Write-Host "Running Tauri dev with APP_ENV=prod..."
  npm run tauri dev
  $exitCode = $LASTEXITCODE
}
finally {
  Set-Location $previousLocation

  if ($null -eq $previousAppEnv) {
    Remove-Item Env:APP_ENV -ErrorAction SilentlyContinue
  }
  else {
    $env:APP_ENV = $previousAppEnv
  }

  if ($null -eq $previousTargetDir) {
    Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
  }
  else {
    $env:CARGO_TARGET_DIR = $previousTargetDir
  }
}

exit $exitCode

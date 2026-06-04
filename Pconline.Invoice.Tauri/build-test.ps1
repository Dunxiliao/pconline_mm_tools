$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $scriptDir

$env:APP_ENV = "test"
$env:CARGO_TARGET_DIR = Join-Path $scriptDir "src-tauri\target-test"

if (-not (Test-Path (Join-Path $scriptDir "env\.env.test"))) {
  throw "Missing env/.env.test"
}

# 可选：签名（需要 .tauri/pconlineinvoice.key 存在）
if (Test-Path ".tauri/pconlineinvoice.key") {
  $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw ".tauri/pconlineinvoice.key"
}

npm run tauri build


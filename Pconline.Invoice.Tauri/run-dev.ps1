$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $scriptDir

Write-Host "Running: npm run tauri dev (Tauri dev mode)..."
npm run tauri dev
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw .tauri/pconlineinvoice.key
npm run tauri build
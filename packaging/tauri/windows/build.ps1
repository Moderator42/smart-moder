$ErrorActionPreference = "Stop"
Set-Location "$PSScriptRoot\..\..\tauri-app"

npm install
npm run tauri build
Write-Host "Done. MSI/EXE bundles are in src-tauri\target\release\bundle"

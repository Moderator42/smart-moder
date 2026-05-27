@echo off
setlocal
cd /d %~dp0\..\..\tauri-app
npm install
npm run tauri build
echo Done. Bundles are in src-tauri\target\release\bundle

# Smart Config Editor

Tauri + Svelte rewrite of the SmartUK / SmartPDD updater.

## Development

```bash
cd tauri-app
npm install
npm run tauri dev
```

## Tests

```bash
cd tauri-app/src-tauri
cargo test
```

## Build

```bash
cd tauri-app
npm run tauri build
```

The release binary is written to `tauri-app/src-tauri/target/release/`.

## Packaging

- Linux packaging templates: `packaging/tauri/arch/PKGBUILD`
- Windows packaging templates: `packaging/tauri/windows/build.ps1` and `build.cmd`
- Legacy Python-era files remain only as reference while the final cleanup is being finished

## Arch Install (from .deb)

```bash
yay -S debtap
debtap -p Smart.Config.Editor_0.1.0_amd64.deb
sudo pacman -U --nodeps smart-config-editor*.pkg.tar.zst
```

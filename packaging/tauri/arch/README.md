# Arch Linux installation through debtap

AppImage is intentionally not used for this project. GitHub Actions builds the Linux client as a Debian package (`.deb`) on Ubuntu, and Arch users convert that package locally.

## Install debtap

Using an AUR helper:

```bash
yay -S debtap
```

## Convert the release `.deb` to an Arch package

```bash
sudo debtap -u
sudo debtap -q Smart.Config.Editor_0.1.0_amd64.deb
sudo pacman -U smart-config-editor-*.pkg.tar.zst
```

If debtap asks about metadata, keep the package name as `smart-config-editor`.

## Why no AppImage

The AppImage built on Ubuntu can bundle or expect libraries that do not match Arch rolling packages, especially WebKit/GTK-related libraries. For this project, `.deb` is the only Linux artifact produced by CI. Arch installation is handled via debtap conversion.

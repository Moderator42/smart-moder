#!/usr/bin/env bash
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "Usage: $0 path/to/Smart.Config.Editor_0.1.0_amd64.deb"
  exit 1
fi

DEB_PATH="$1"

if ! command -v debtap >/dev/null 2>&1; then
  echo "debtap is not installed. Install it first: yay -S debtap"
  exit 1
fi

sudo debtap -u
sudo debtap -q "$DEB_PATH"
PKG=$(find . -maxdepth 1 -type f \( -name "*.pkg.tar.zst" -o -name "*.pkg.tar.xz" \) | sort | tail -1)

if [ -z "${PKG:-}" ]; then
  echo "Arch package was not created."
  exit 1
fi

sudo pacman -U "$PKG"

#!/bin/bash
# Скрипт удаляет системные либы из AppImage чтобы он использовал те что в системе
# Запускать после: npm run tauri build

set -e

APPIMAGE=$(find src-tauri/target/release/bundle/appimage -name "*.AppImage" | head -1)

if [ -z "$APPIMAGE" ]; then
    echo "AppImage не найден в src-tauri/target/release/bundle/appimage/"
    exit 1
fi

echo "Обрабатываем: $APPIMAGE"

# Распаковываем
WORKDIR=$(mktemp -d)
cp "$APPIMAGE" "$WORKDIR/app.AppImage"
cd "$WORKDIR"
chmod +x app.AppImage
./app.AppImage --appimage-extract > /dev/null 2>&1

# Удаляем конфликтующие либы — AppImage будет брать их из системы
LIBS_TO_REMOVE=(
    "libwebkit2gtk-4.1.so.0"
    "libEGL.so.1"
    "libEGL_mesa.so.0"
    "libGL.so.1"
    "libGLdispatch.so.0"
    "libGLX.so.0"
    "libGLX_mesa.so.0"
    "libGLESv2.so.2"
    "libgbm.so.1"
)

for lib in "${LIBS_TO_REMOVE[@]}"; do
    if [ -f "squashfs-root/usr/lib/$lib" ]; then
        echo "Удаляем: $lib"
        rm "squashfs-root/usr/lib/$lib"
    fi
done

# Пересобираем AppImage
APPIMAGETOOL=$(which appimagetool 2>/dev/null || true)
if [ -z "$APPIMAGETOOL" ]; then
    echo "Скачиваем appimagetool..."
    wget -q "https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage" -O appimagetool
    chmod +x appimagetool
    APPIMAGETOOL="./appimagetool"
fi

APPIMAGE_NAME=$(basename "$APPIMAGE")
OUTDIR=$(dirname "$(realpath "$APPIMAGE")")

ARCH=x86_64 $APPIMAGETOOL squashfs-root "$OUTDIR/$APPIMAGE_NAME" > /dev/null 2>&1

echo "Готово: $OUTDIR/$APPIMAGE_NAME"
cd /
rm -rf "$WORKDIR"

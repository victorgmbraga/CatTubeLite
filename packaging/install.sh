#!/bin/sh
# Install CatTube from this folder.
#   ./install.sh              into ~/.local (no root needed)
#   sudo ./install.sh /usr/local
#   ./install.sh --uninstall [PREFIX]
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
uninstall=0
if [ "${1:-}" = "--uninstall" ]; then uninstall=1; shift; fi
prefix="${1:-$HOME/.local}"
files="bin/cattube share/cattube/cattube-osd.lua share/cattube/cattube-input.conf
share/cattube/ytdl-server.py share/cattube/ytdl-jsc.js share/cattube/ytdl-jsc-qjs.js
share/applications/io.github.coderastic.CatTube.desktop
share/icons/hicolor/scalable/apps/io.github.coderastic.CatTube.svg
share/pixmaps/io.github.coderastic.CatTube.png
share/metainfo/io.github.coderastic.CatTube.metainfo.xml"
if [ "$uninstall" = 1 ]; then
  for f in $files; do rm -f "$prefix/$f"; done
  rmdir "$prefix/share/cattube" 2>/dev/null || true
  echo "CatTube removed from $prefix (your settings stay in ~/.config/cattube)"
  exit 0
fi
# A build has one icon (CatTube's, or the logo it was built with): the
# other one, left by an earlier install, goes.
for f in $files; do
  if [ -e "$here/$f" ]; then
    mkdir -p "$(dirname "$prefix/$f")"
    cp "$here/$f" "$prefix/$f"
  else
    rm -f "$prefix/$f"
  fi
done
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$prefix/share/applications" || true
command -v mpv >/dev/null 2>&1 || echo "Note: CatTube plays videos with mpv. Install it with your package manager."
command -v curl >/dev/null 2>&1 || echo "Note: CatTube needs curl. Install it with your package manager."
command -v python3 >/dev/null 2>&1 || echo "Note: CatTube needs Python 3 (for yt-dlp). Install it with your package manager."
echo "CatTube installed in $prefix (run: cattube)"

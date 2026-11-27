#!/usr/bin/env bash
# Build dist/cattube-<version>-<arch>.tar.gz for this machine's architecture:
# the binary, the player files, the desktop entry and icon, and install.sh.
# Needs mpv, curl and Python 3 on the system it runs on.
# CATTUBE_NAME and CATTUBE_LOGO (a PNG) brand a build made for yourself:
# the app, its desktop entry and its icon (see build.rs).
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
NAME="cattube-$VERSION-$(uname -m)"
# OpenGL only, on every architecture; CATTUBE_WGPU=1 adds the Vulkan
# renderer (wgpu) for a desktop build.
FEATURES=()
if [[ "${CATTUBE_WGPU:-0}" == 1 ]]; then FEATURES=(--features wgpu); fi
cargo build --release --locked "${FEATURES[@]}"
rm -rf "dist/$NAME" && mkdir -p "dist/$NAME"
D="dist/$NAME"
install -Dm755 "${CARGO_TARGET_DIR:-target}/release/cattube" "$D/bin/cattube"
install -Dm644 assets/cattube-osd.lua "$D/share/cattube/cattube-osd.lua"
install -Dm644 assets/cattube-input.conf "$D/share/cattube/cattube-input.conf"
install -Dm644 assets/ytdl-server.py "$D/share/cattube/ytdl-server.py"
install -Dm644 assets/ytdl-jsc.js "$D/share/cattube/ytdl-jsc.js"
install -Dm644 assets/ytdl-jsc-qjs.js "$D/share/cattube/ytdl-jsc-qjs.js"
install -Dm644 packaging/io.github.coderastic.CatTube.desktop "$D/share/applications/io.github.coderastic.CatTube.desktop"
if [[ -n "${CATTUBE_LOGO:-}" ]]; then
  install -Dm644 "$CATTUBE_LOGO" "$D/share/pixmaps/io.github.coderastic.CatTube.png"
else
  install -Dm644 packaging/io.github.coderastic.CatTube.svg "$D/share/icons/hicolor/scalable/apps/io.github.coderastic.CatTube.svg"
fi
install -Dm644 packaging/io.github.coderastic.CatTube.metainfo.xml "$D/share/metainfo/io.github.coderastic.CatTube.metainfo.xml"
if [[ -n "${CATTUBE_NAME:-}" ]]; then
  # Escaped for XML (metainfo), then for sed's replacement (both).
  sed_escape() { sed 's/[\\&|]/\\&/g'; }
  sed_name="$(printf '%s' "$CATTUBE_NAME" | sed_escape)"
  xml_name="$(printf '%s' "$CATTUBE_NAME" | sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g' | sed_escape)"
  sed -i "s|^Name=.*|Name=$sed_name|" "$D/share/applications/io.github.coderastic.CatTube.desktop"
  sed -i "s|^  <name>.*</name>|  <name>$xml_name</name>|" "$D/share/metainfo/io.github.coderastic.CatTube.metainfo.xml"
fi
install -Dm755 packaging/install.sh "$D/install.sh"
install -Dm644 README.md "$D/README.md"
install -Dm644 README.pt-BR.md "$D/README.pt-BR.md"
install -Dm644 LICENSE "$D/LICENSE"
install -Dm644 assets/fonts/LICENSE-Inter.txt "$D/LICENSE-Inter.txt"
install -Dm644 assets/fonts/LICENSE-NotoEmoji.txt "$D/LICENSE-NotoEmoji.txt"
tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"
rm -rf "dist/$NAME"
(cd dist && sha256sum "$NAME.tar.gz" > "$NAME.tar.gz.sha256")
echo "dist/$NAME.tar.gz"

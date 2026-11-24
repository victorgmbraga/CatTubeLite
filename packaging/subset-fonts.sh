#!/usr/bin/env bash
# Cut Inter down to what CatTube draws: Latin (with Vietnamese), Greek,
# Cyrillic, punctuation and common symbols; kerning only, no hinting (egui
# uses neither the other features nor hinting). 410 KB → 152 KB a weight.
# Run on the full Inter TTFs (https://github.com/rsms/inter):
#   pip install fonttools && packaging/subset-fonts.sh /path/to/Inter-*.ttf
set -euo pipefail
cd "$(dirname "$0")/.."
UNICODES="U+0020-007E,U+00A0-036F,U+0370-03FF,U+0400-052F,U+1E00-1EFF,U+2000-206F,U+20A0-20CF,U+2100-218F,U+2190-21FF,U+2200-22FF,U+2300-23FF,U+25A0-25FF,U+2600-26FF,U+2700-27BF,U+FB00-FB06,U+FEFF,U+FFFD"
for font in "$@"; do
  pyftsubset "$font" --unicodes="$UNICODES" --layout-features=kern \
    --no-hinting --desubroutinize --output-file="assets/fonts/$(basename "$font")"
done

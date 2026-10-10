#!/bin/sh
# Joins the intro (grabar.mjs) and the real capture (demo.sh) into the README GIF.
# One palette for both parts: the intro's accent and the app's colours share it.
set -eu
cd "$(dirname "$0")/../../docs/img"
for l in ${1:-es en}; do
  ffmpeg -loglevel error -y -i "intro.$l.gif" -i "demo-captura.$l.gif" -filter_complex \
    "[0:v]fps=15,scale=960:540,setsar=1[a];[1:v]fps=15,scale=960:540,setsar=1[b];[a][b]concat=n=2:v=1,split[x][y];[x]palettegen=stats_mode=diff:max_colors=200[p];[y][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle" \
    "demo.$l.gif"
  echo "docs/img/demo.$l.gif  $(du -k "demo.$l.gif" | cut -f1) KB"
done

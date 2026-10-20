#!/bin/sh
# Copies the installers from a Tauri bundle folder into an output folder, renamed to package-registry-safe
# names: "Code Toolbox_0.1.0_amd64.deb" → "code-toolbox_0.1.0_amd64.deb".
# Usage: scripts/collect-bundles.sh <bundle-dir> <out-dir>
set -eu
bundle=$1
out=$2
mkdir -p "$out"
found=0
for file in "$bundle"/deb/*.deb "$bundle"/rpm/*.rpm "$bundle"/nsis/*.exe "$bundle"/dmg/*.dmg; do
  [ -f "$file" ] || continue
  name=$(basename "$file" | tr '[:upper:] ' '[:lower:]-')
  cp "$file" "$out/$name"
  echo "$out/$name"
  found=1
done
[ "$found" = 1 ] || { echo "No installers in $bundle" >&2; exit 1; }

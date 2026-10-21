#!/bin/sh
# Writes latest.json for the app's updater (tauri.conf.json > plugins > updater): each platform's update file
# in <dir>, with the signature the build left next to it (<file>.sig). Used by .github/workflows/build.yml.
# Usage: scripts/updater-manifest.sh <version> <download URL of the release> <dir>
set -eu
version=$1
base=$2
dir=$3

entry() {
  if [ ! -f "$dir/$1" ] || [ ! -f "$dir/$1.sig" ]; then
    echo "Missing $dir/$1 or its .sig" >&2
    exit 1
  fi
  jq -n --arg url "$base/$1" --rawfile sig "$dir/$1.sig" '{url: $url, signature: ($sig | rtrimstr("\n"))}'
}

# The macOS update is the universal app, so it serves both architectures. Linux updates per package type
# (the updater looks for `linux-x86_64-deb` or `-rpm` first, by how the app was installed).
mac=$(entry code-toolbox.app.tar.gz)
windows=$(entry "code-toolbox_${version}_x64-setup.exe")
deb=$(entry "code-toolbox_${version}_amd64.deb")
rpm=$(entry "code-toolbox-${version}-1.x86_64.rpm")
# shellcheck disable=SC2016 # $mac etc. are jq variables
platforms='{"darwin-aarch64": $mac, "darwin-x86_64": $mac, "windows-x86_64": $windows,'
# shellcheck disable=SC2016
platforms="$platforms"' "linux-x86_64-deb": $deb, "linux-x86_64-rpm": $rpm}'
jq -n --arg version "$version" --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --argjson mac "$mac" --argjson windows "$windows" --argjson deb "$deb" --argjson rpm "$rpm" \
  "{version: \$version, pub_date: \$date, platforms: $platforms}"

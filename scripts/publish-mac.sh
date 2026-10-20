#!/bin/sh
# Builds the universal macOS .dmg and adds it to the GitLab release of the current version, which the
# tag's pipeline must have created first. Needs a GitLab token with the `api` scope, either in
# GITLAB_TOKEN or in the login keychain:
#   security add-generic-password -s code-toolbox-gitlab -a "$USER" -w
set -eu
cd "$(dirname "$0")/.."
# The GitLab instance and project are the `origin` remote's (git@host:group/name.git or https://host/group/name).
remote=$(git remote get-url origin)
address=$(printf %s "$remote" | sed -E 's#^[a-z+]+://##; s#^[^@/]*@##')
host=${address%%[:/]*}
project=$(printf %s "${address#"$host"}" | sed -E 's#^[:/]##; s#\.git$##')
GITLAB=${GITLAB_URL:-https://$host}
API="$GITLAB/api/v4/projects/$(printf %s "$project" | sed 's#/#%2F#g')"
version=$(node -p "require('./app/src-tauri/tauri.conf.json').version")
token=${GITLAB_TOKEN:-$(security find-generic-password -s code-toolbox-gitlab -w 2>/dev/null || true)}
[ -n "$token" ] || { echo "No GitLab token: set GITLAB_TOKEN or add it to the keychain (see the top of $0)" >&2; exit 1; }

status=$(curl -s -o /dev/null -w '%{http_code}' --header "PRIVATE-TOKEN: $token" "$API/releases/v$version")
[ "$status" = 200 ] || { echo "No release v$version yet (HTTP $status): push the tag v$version and let its pipeline finish" >&2; exit 1; }

# Ad-hoc signed (no Apple developer certificate), for both Apple silicon and Intel. CI=true keeps the
# DMG script from scripting Finder to lay out the window, which would ask for Automation access.
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
(cd app && CI=true APPLE_SIGNING_IDENTITY=- npx tauri build --target universal-apple-darwin --bundles dmg)
rm -rf build/mac && mkdir -p build/mac
scripts/collect-bundles.sh target/universal-apple-darwin/release/bundle build/mac
file=$(ls build/mac/*.dmg)
name=$(basename "$file")
url="$API/packages/generic/code-toolbox/$version/$name"

echo "Uploading $name"
curl -fsS --header "PRIVATE-TOKEN: $token" --upload-file "$file" "$url" >/dev/null
existing=$(curl -fsS --header "PRIVATE-TOKEN: $token" "$API/releases/v$version/assets/links" | node -e '
  let s = ""; process.stdin.on("data", (d) => (s += d)).on("end", () => {
    const link = JSON.parse(s).find((l) => l.name === process.argv[1]);
    console.log(link ? link.id : "");
  });' "$name")
if [ -n "$existing" ]; then
  curl -fsS -X DELETE --header "PRIVATE-TOKEN: $token" "$API/releases/v$version/assets/links/$existing" >/dev/null
fi
curl -fsS --header "PRIVATE-TOKEN: $token" --data-urlencode "name=$name" --data-urlencode "url=$url" \
  --data "link_type=package" "$API/releases/v$version/assets/links" >/dev/null
echo "Added $name to $GITLAB/$project/-/releases/v$version"

#!/bin/sh
# Publishes the current commit to the public GitHub repository (remote `github`) and tags it v<version>,
# which starts the release workflow (.github/workflows/build.yml).
#
# GitHub has a history of its own: each publish is one new commit there with this commit's files, so
# earlier commits and their author details stay here. Left out: the paths in EXCLUDE. Refused when any
# word in `git config publish.forbidden` (space-separated, kept out of the repository) appears in what
# would be published. The commit's author is `git config publish.name` / `publish.email`.
set -eu
cd "$(dirname "$0")/.."
EXCLUDE="PLAN.md PLAN-cross-platform.md"

die() { echo "$*" >&2; exit 1; }
name=$(git config publish.name) || die "Set the GitHub author: git config publish.name …; git config publish.email …"
email=$(git config publish.email) || die "Set the GitHub author: git config publish.email …"
[ -z "$(git status --porcelain)" ] || die "Commit or stash your changes first"
version=$(node -p "require('./app/src-tauri/tauri.conf.json').version")

# This commit's files minus EXCLUDE, as a tree, without touching the real index.
index=$(mktemp)
trap 'rm -f "$index"' EXIT
GIT_INDEX_FILE=$index git read-tree HEAD
# shellcheck disable=SC2086
GIT_INDEX_FILE=$index git rm -q --cached --ignore-unmatch -- $EXCLUDE
tree=$(GIT_INDEX_FILE=$index git write-tree)

for word in $(git config publish.forbidden || true); do
  if hits=$(git grep -a -i -F -l -e "$word" "$tree" --); then
    die "\"$word\" appears in:
$hits"
  fi
  case "$(printf '%s %s' "$name" "$email" | tr '[:upper:]' '[:lower:]')" in
    *"$(printf %s "$word" | tr '[:upper:]' '[:lower:]')"*) die "\"$word\" appears in the author" ;;
  esac
done

git fetch -q github
parent=$(git rev-parse -q --verify refs/remotes/github/main || true)
if [ -n "$parent" ] && [ "$(git rev-parse "$parent^{tree}")" = "$tree" ]; then
  commit=$parent
  echo "GitHub main already has these files"
else
  commit=$(GIT_AUTHOR_NAME=$name GIT_AUTHOR_EMAIL=$email GIT_COMMITTER_NAME=$name GIT_COMMITTER_EMAIL=$email \
    git commit-tree "$tree" ${parent:+-p "$parent"} -m "Code Toolbox $version")
  git push -q github "$commit:refs/heads/main"
  echo "Pushed $(git rev-parse --short "$commit") to GitHub main"
fi

if git ls-remote --exit-code --tags github "refs/tags/v$version" >/dev/null; then
  echo "GitHub already has the tag v$version"
else
  git push -q github "$commit:refs/tags/v$version"
  echo "Tagged v$version on GitHub: the release workflow builds it"
fi

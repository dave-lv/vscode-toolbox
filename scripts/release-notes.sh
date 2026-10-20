#!/bin/sh
# Release description for version $1; $2 is the README's URL at that version. Used by the release jobs
# (.github/workflows/release.yml, .gitlab-ci.yml).
cat <<NOTES
Code Toolbox $1: a tray app for opening VS Code projects and profiles. See the [README]($2) for what it does.

| OS | Download | First start |
| --- | --- | --- |
| Windows 10/11 | \`code-toolbox_$1_x64-setup.exe\` | Unsigned: on "Windows protected your PC", choose **More info → Run anyway**. Installs for the current user, no admin needed. |
| macOS 13+ (Apple silicon and Intel) | \`code-toolbox_$1_universal.dmg\` | Unsigned: open it once, then **System Settings → Privacy & Security → Open Anyway**. |
| Linux (Debian/Ubuntu) | \`code-toolbox_$1_amd64.deb\` | \`sudo apt install ./code-toolbox_$1_amd64.deb\` |
| Linux (Fedora/openSUSE) | \`code-toolbox-$1-1.x86_64.rpm\` | \`sudo dnf install ./code-toolbox-$1-1.x86_64.rpm\` |
| Linux (Arch/CachyOS) | \`code-toolbox-$1-1-x86_64.pkg.tar.zst\` | \`sudo pacman -U ./code-toolbox-$1-1-x86_64.pkg.tar.zst\` |

The app installs its VS Code extension (a shell and Claude Code or Codex in every project window) from **Settings → Dev Layout extension**. It's also \`dev-layout-*.vsix\` here, for **Extensions → ⋯ → Install from VSIX…**
NOTES

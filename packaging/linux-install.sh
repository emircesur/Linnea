#!/usr/bin/env bash
# Installs Linnea for this user: ~/.local/share/linnea, the `linnea` and `linnea-app` commands in
# ~/.local/bin, and a menu entry.  ./install.sh --uninstall removes them again (your work stays).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
dest="${XDG_DATA_HOME:-$HOME/.local/share}/linnea"
bin="$HOME/.local/bin"
desktop="${XDG_DATA_HOME:-$HOME/.local/share}/applications/linnea.desktop"

if [ "${1:-}" = "--uninstall" ]; then
  rm -rf "$dest" "$bin/linnea" "$bin/linnea-app" "$desktop"
  command -v claude >/dev/null && claude mcp remove linnea -s user >/dev/null 2>&1 || true
  echo "Linnea was removed. Your films and settings (~/Videos/Linnea, ~/.config/Linnea) stay where they are."
  exit 0
fi

rm -rf "$dest" && mkdir -p "$dest" "$bin" "$(dirname "$desktop")"
cp -R "$here/bin" "$here/library" "$here/ui" "$here/skill" "$here/README.md" "$here/linnea.png" "$dest/"
ln -sf "$dest/bin/linnea" "$bin/linnea"
ln -sf "$dest/bin/linnea-app" "$bin/linnea-app"
cat > "$desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Linnea
Comment=Claude writes the video. You finish it by hand.
Exec=$dest/bin/linnea-app
Icon=$dest/linnea.png
Categories=AudioVideo;Video;
Terminal=false
DESKTOP

echo "Linnea is installed in $dest"
echo "Start it from the menu, or run: linnea-app"
case ":$PATH:" in *":$bin:"*) ;; *) echo "Add $bin to your PATH to use the \`linnea\` command." ;; esac
if command -v claude >/dev/null; then
  read -r -p "Connect Linnea to Claude Code as an MCP server and install its skill? [y/N] " a || a=""
  if [ "$a" = "y" ] || [ "$a" = "Y" ]; then
    claude mcp remove linnea -s user >/dev/null 2>&1 || true
    claude mcp add --scope user linnea -- "$dest/bin/linnea" mcp
    mkdir -p "$HOME/.claude/skills/linnea" && cp "$dest/skill/linnea/SKILL.md" "$HOME/.claude/skills/linnea/"
  fi
fi

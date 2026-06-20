#!/usr/bin/env bash
# Installer for csb — https://github.com/shareefhadid/csb
# Builds the sandbox image and wires `csb` / `csb-doctor` into your shell.
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
IMAGE="${CSB_IMAGE:-claude-box}"

echo "==> Checking prerequisites"
if ! command -v container >/dev/null 2>&1; then
  echo "    Apple 'container' not found. Install it first:" >&2
  echo "      brew install container" >&2
  echo "    (Requires macOS 26 Tahoe or newer.)" >&2
  exit 1
fi

echo "==> Starting the container service"
container system start >/dev/null 2>&1 || true

echo "==> Building image '$IMAGE' (this is also how you update Claude later)"
container build -t "$IMAGE" "$REPO_DIR"
container builder stop >/dev/null 2>&1 || true

echo "==> Wiring shell functions"
LINE="source \"$REPO_DIR/csb.sh\""
WIRED=0
for RC in "$HOME/.zshrc" "$HOME/.bashrc"; do
  [ -f "$RC" ] || continue
  if grep -qF "$LINE" "$RC"; then
    echo "    already sourced in $RC"
    WIRED=1
  else
    printf '\n# csb — sandboxed Claude Code (https://github.com/shareefhadid/csb)\n%s\n' "$LINE" >> "$RC"
    echo "    added to $RC"
    WIRED=1
  fi
done
[ "$WIRED" -eq 1 ] || echo "    no ~/.zshrc or ~/.bashrc found — add this line to your rc manually:
      $LINE"

cat <<EOF

==> Done.
    Open a new terminal (or run: source "$REPO_DIR/csb.sh"), then:
      cd <your-project>
      csb

    First run does a one-time copy-paste login (persists via ~/.claude).
    Override the memory cap with:  export CSB_MEMORY=8g
EOF

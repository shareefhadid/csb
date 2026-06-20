# csb — run Claude Code isolated in an Apple `container` micro-VM.
# Source this from your shell rc:  source /path/to/csb/csb.sh
# Docs: https://github.com/shareefhadid/csb
#
# Config (override before sourcing, or export in your rc):
: "${CSB_IMAGE:=claude-box}"   # image name built from the bundled Dockerfile
: "${CSB_MEMORY:=6g}"          # hard memory cap (dynamic — idle stays small).
                               # The VM has no swap, so this cap is the only cushion;
                               # too low + a heavy build (e.g. `next build`) = OOM freeze.

# Launch Claude Code, sandboxed, in the current project.
#   cd <project> && csb            # interactive
#   csb --continue                 # resume the previous conversation (flags pass through)
#
# Only the current dir and ~/.claude are mounted, so the rest of your Mac is invisible
# to the agent. ~/.claude is mounted at its REAL host path (not /root/.claude) so the
# plugin/marketplace registry — which bakes absolute host paths — resolves inside.
csb() {
  if ! command -v container >/dev/null 2>&1; then
    echo "csb: Apple 'container' not found. Install: brew install container" >&2
    return 1
  fi
  container system start >/dev/null 2>&1
  container run -it --rm \
    -m "$CSB_MEMORY" \
    -v "$PWD:/workspace" -w /workspace \
    -v "$HOME/.claude:$HOME/.claude" \
    -e CLAUDE_CONFIG_DIR="$HOME/.claude" \
    "$CSB_IMAGE" claude --dangerously-skip-permissions "$@"
}

# Diagnose a slow/frozen sandbox from ANOTHER terminal. `container stats` reads from the
# host hypervisor, so it works even when the container's pane is wedged and `exec` hangs.
# If MEM is pinned near the cap, it's OOM: raise CSB_MEMORY, or cap the build's heap
# (NODE_OPTIONS=--max-old-space-size=...). Recover a frozen one: container stop <id>.
csb-doctor() {
  echo "== containers =="; container ls
  echo; echo "== live CPU/MEM (MEM near the cap => OOM, the usual freeze cause) =="
  container stats --no-stream 2>/dev/null || echo "  (none running)"
}

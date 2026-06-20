# claude-box — sandboxed Claude Code image for Apple `container`.
#
# Built by install.sh, or manually:
#   container build -t claude-box .
#   container builder stop      # reclaim the builder VM's RAM afterwards
#
# This is the place to add anything you ALWAYS want available inside the sandbox.

FROM node:22-slim

# Base tooling. Add OS packages you always need here (e.g. python3, build-essential).
# procps gives free/top/ps — essential for diagnosing memory pressure / OOM freezes.
RUN apt-get update && apt-get install -y --no-install-recommends \
      git ca-certificates curl ripgrep less procps \
    && rm -rf /var/lib/apt/lists/*

# Claude Code itself. Rebuilding this image is how you UPDATE Claude's version.
# Pin a version instead of @latest for reproducibility if you prefer.
RUN npm install -g @anthropic-ai/claude-code@latest

# Add global npm tools you always want, e.g.:
# RUN npm install -g pnpm wrangler typescript

# Operational guidance for the in-container agent. Claude auto-loads
# /etc/claude-code/CLAUDE.md as managed-policy memory every session (it's outside the
# mounted dirs, so it's never shadowed). Edit sandbox-guidance.md and rebuild to change it.
COPY sandbox-guidance.md /etc/claude-code/CLAUDE.md

# IS_SANDBOX=1 lets Claude run --dangerously-skip-permissions as container-root.
ENV IS_SANDBOX=1
WORKDIR /workspace

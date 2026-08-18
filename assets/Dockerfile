# claude-box — sandboxed Claude Code image for Apple `container`.
# Compiled into the csb binary and written to a temp build context by `csb build`.
# To customize: edit this file in a clone, `cargo install --path .`, `csb build --force`.

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
# /etc/claude-code/CLAUDE.md as managed-policy memory every session.
COPY sandbox-guidance.md /etc/claude-code/CLAUDE.md

# Staleness tracking — replaced at build time by csb.
LABEL csb.assets.sha256="{{CSB_ASSETS_SHA256}}"

ENV IS_SANDBOX=1
WORKDIR /workspace

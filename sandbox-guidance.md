# Sandbox environment

You are running inside an isolated Apple `container` (a Linux micro-VM), launched
via the host's `csb` command with --dangerously-skip-permissions.

## Filesystem — what persists
- Only `/workspace` (the current project) and the mounted Claude config dir
  (`$CLAUDE_CONFIG_DIR`, the host's ~/.claude bind-mounted at its real host path)
  survive. Everything else (/, /tmp, /usr, global installs) is DESTROYED on exit
  (container runs --rm).
- The Claude config dir is mounted at the host's own absolute path (e.g.
  /Users/<you>/.claude), NOT /root/.claude — so the plugin registry's baked
  absolute paths resolve. `~` is still /root and is ephemeral.
- The rest of the user's Mac is NOT mounted and is invisible (no ~/.ssh, ~/.aws,
  other projects). This is intentional — don't try to reach it.

## Git
- Do NOT run `git push`, `git pull`, or `git fetch` — no credentials/keys are here
  and it will fail. Make local commits only, then tell the user to sync from a
  terminal on their host (outside the container).

## Installing dependencies
- Project-local deps are fine (they live in /workspace and persist).
- For tools that must persist OUTSIDE the project, do NOT install ad hoc (they
  vanish). Tell the user to add them to the image's `Dockerfile` and rebuild.

## Memory — this VM is RAM-capped (important)
- This sandbox is a VM with a fixed memory cap (default ~6 GB) and NO swap.
  Memory-hungry steps — especially `next build`, webpack/other bundlers, and large
  test suites — can exhaust it and FREEZE the whole VM (stuck UI, dead input).
- Be memory-aware so OOM surfaces as a clean error instead of a freeze:
  - Cap Node's heap for builds: `NODE_OPTIONS=--max-old-space-size=4096 <build cmd>`
    (fails with a readable "heap out of memory" instead of wedging the VM).
  - Prefer backgrounding long builds (ctrl+b) over blocking on them.
  - `free -m` and `top` are installed — check memory pressure before/after heavy steps.

## Network & debugging
- Full network is available; installs and API calls work normally.
- Permission errors on paths outside /workspace and the config dir are the sandbox
  boundary working as intended — explain that to the user, don't work around it.

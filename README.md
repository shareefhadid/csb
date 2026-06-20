# csb — sandboxed Claude Code on Apple `container`

Run [Claude Code](https://docs.anthropic.com/en/docs/claude-code) with
`--dangerously-skip-permissions` inside an isolated Linux micro-VM, with one command:

```sh
cd <your-project> && csb
```

`csb` launches Claude Code inside an [Apple `container`](https://github.com/apple/container)
micro-VM (native to macOS 26 Tahoe). Only the current project and `~/.claude` are mounted,
so the rest of your Mac stays invisible to the agent — letting you skip permission prompts
without handing an autonomous agent your whole home directory.

> **Unofficial.** Not affiliated with or endorsed by Anthropic or Apple.

## Security — read this before you trust it

This is a security tool, so be precise about what it does and doesn't do.

**What it protects:** the *filesystem boundary*. Only `$PWD` and `~/.claude` are mounted
into the VM. The rest of your Mac — `~/.ssh`, `~/.aws`, browser data, other projects, the
whole rest of `$HOME` — is simply not present in the container and cannot be read or written,
even by a misbehaving agent running with `--dangerously-skip-permissions`. Files the agent
writes appear owned by you on the host.

**What it does NOT protect against:**

- **Network exfiltration.** The VM has full, unrestricted network egress (you need it for
  `npm`/`pip`/`cargo`/API calls). A malicious repo or prompt injection can send anything it
  can *read* out to the internet. Blast radius = your project files + `~/.claude` (which holds
  your Claude OAuth token). Domain allowlisting helps only marginally, because content-accepting
  trusted domains (GitHub, etc.) are themselves exfil channels.
- **Secrets inside the project.** `.env.local` and friends live in the mounted project dir, so
  they're fully readable and usable by the agent. Use scoped/test API keys in dev, and prefer
  running your real dev server on the host (the agent rarely needs to *run* the project — and
  by default no container port is published to your host anyway).
- **Authed MCP servers.** Any MCP server you log into *inside* the sandbox becomes a live
  capability the agent can invoke without prompting (and its token lands in `~/.claude`). Scope
  them down — read-only / restricted keys, `--scope project` rather than user — so a hijacked
  session can't abuse them.

**The mental model:** csb protects your *machine and other projects*. It does **not** protect
the credentials and capabilities you hand to the current session. Contain those by *scoping
them down*, not by sandboxing. You assume the residual risk.

## Requirements

- **macOS 26 (Tahoe) or newer** — Apple `container` is macOS 26-native.
- **Apple `container`**: `brew install container`
- A Claude Code subscription / login.

## Install

```sh
git clone https://github.com/shareefhadid/csb.git
cd csb
./install.sh
```

`install.sh` builds the sandbox image (`claude-box`) and adds `source .../csb.sh` to your
`~/.zshrc` / `~/.bashrc`. Open a new terminal afterward. Prefer to wire it yourself? Just
`source csb.sh` from your rc.

## Usage

```sh
cd <your-project>
csb                 # launch Claude Code, sandboxed, in this project
csb --continue      # resume the previous conversation (all flags pass through to claude)
csb-doctor          # from another terminal: health/memory check (see Troubleshooting)
```

First run does a one-time copy-paste OAuth login (there's no browser/Keychain in the VM); it
persists via the mounted `~/.claude`, so you won't log in again.

## Configuration

| Env var      | Default       | Purpose                                                   |
| ------------ | ------------- | --------------------------------------------------------- |
| `CSB_MEMORY` | `6g`          | Hard memory cap for the VM (see *OOM* below).             |
| `CSB_IMAGE`  | `claude-box`  | Image name to build/run.                                  |

Export before launching, e.g. `export CSB_MEMORY=8g`. **Add tools you always want** (OS
packages, global npm CLIs, Python, etc.) to the `Dockerfile`, then rebuild:
`container build -t claude-box . && container builder stop`. Rebuilding is also how you
update Claude Code's version.

## How it works (and the gotchas it solves)

A thin wrapper, but it bakes in fixes for several non-obvious sharp edges:

- **`~/.claude` is mounted at its real host path**, not `/root/.claude`, with
  `CLAUDE_CONFIG_DIR` pointed at it. Claude's plugin/marketplace registry stores *absolute
  host paths*; mounting anywhere else makes `claude plugin install` fail with
  "Source path does not exist" even though the files are present. Same-path mount = they
  resolve verbatim, and MCP config / login / plugins all persist across runs.
- **`IS_SANDBOX=1`** lets Claude run `--dangerously-skip-permissions` as the container's root
  (it otherwise refuses). Container-root maps to your host UID, so written files are yours.
- **Operational guidance is baked into the image** at `/etc/claude-code/CLAUDE.md` (a
  managed-policy memory path, outside the mounts, auto-loaded every session). It tells the
  in-container agent how it's running: no `git push`/`pull` (no creds), persist tools via the
  Dockerfile, and stay memory-aware. Edit `sandbox-guidance.md` and rebuild to change it.
- **Memory cap + diagnostics** for the OOM failure mode below.

## Troubleshooting

**Frozen UI / "I can't type" / timer stuck.** Almost always **OOM**. The VM has a fixed memory
cap (default 6 GB) and **no swap**, so a memory-hungry step — especially `next build`,
bundlers, or large test suites — can exhaust it and wedge the whole VM, including the TUI and
even `container exec`. From another terminal:

```sh
csb-doctor          # container ls + `container stats` (reads from the host, works when exec hangs)
```

If MEM is pinned near the cap, it's OOM. Recover the wedged container:

```sh
container stop <id>      # then `container kill <id>` if stop hangs
```

Your file edits are safe (they're on the host via the mount). Relaunch and `csb --continue` to
resume the conversation. To prevent it: raise `CSB_MEMORY`, and/or cap the build's heap inside
the sandbox (`NODE_OPTIONS=--max-old-space-size=4096 npm run build`) so it errors cleanly
instead of freezing.

**`git push`/`pull` fails inside.** By design — no SSH keys/creds are mounted. Let the agent
commit locally, then sync from a host terminal.

**Can't reach the dev server in my browser.** No container port is published to the host by
default, and the container's `localhost` is private to the VM. Run your dev server on the host
instead, or add a publish flag (e.g. `-p 3001:3000`) to the `csb` function.

**Service down after reboot.** `container system start` (or `brew services start container`).

## License

[MIT](./LICENSE)

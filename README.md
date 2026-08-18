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
- **Writes to `~/.claude` that your host executes later.** The config dir is mounted
  read-write, and your *host* Claude reads hooks, settings, and plugins from it. An agent
  inside the sandbox can write a hook there that runs on your Mac the next time you launch
  Claude outside the container. Treat `~/.claude` as inside the blast radius, not as a wall —
  review `settings.json` / hooks if a session did anything you didn't expect.
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
brew install shareefhadid/tap/csb
```

That's a pre-built binary — no Rust toolchain needed. Alternatives:

```sh
cargo install --git https://github.com/shareefhadid/csb   # from source
git clone https://github.com/shareefhadid/csb.git && cd csb && cargo install --path .
```

The Dockerfile and sandbox guidance are compiled into the binary, so there's nothing else to
keep around. The first `csb` run builds the `claude-box` image automatically (a few minutes);
`csb build` does it ahead of time.

### Upgrading from the shell version

The old version was a `csb()` shell function sourced from `csb.sh`. **Remove the
`source .../csb.sh` line from your `~/.zshrc` / `~/.bashrc`** — a shell function shadows the
binary, so you'd silently keep running the old one. `csb doctor` warns if it finds that line.
Your existing image and `~/.claude` carry over; run `csb build --force` once to pick up the
current sandbox guidance.

## Usage

```sh
cd <your-project>
csb                    # launch Claude Code, sandboxed, in this project
csb --continue         # all unrecognized flags pass through to claude
csb -p "explain this"  # non-interactive; safe to pipe in and out
csb -- doctor          # `--` forces a word through to claude instead of csb
csb doctor             # from another terminal: health/memory check
csb build              # build the image if it's missing or out of date
csb build --force      # rebuild unconditionally — this is how you update Claude Code
```

First run does a one-time copy-paste OAuth login (there's no browser/Keychain in the VM); it
persists via the mounted `~/.claude`, so you won't log in again.

`run`, `doctor`, and `build` are reserved subcommand names; anything else is forwarded to
`claude` verbatim. `csb run <args>` is the explicit form of the default behavior.

## Configuration

| Env var      | Default      | Purpose                                        |
| ------------ | ------------ | ---------------------------------------------- |
| `CSB_MEMORY` | `6g`         | Hard memory cap for the VM (see *OOM* below).  |
| `CSB_IMAGE`  | `claude-box` | Image name to build/run.                       |

Export before launching, e.g. `export CSB_MEMORY=8g`.

**Adding tools to the sandbox** (OS packages, global npm CLIs, Python, …): edit
`assets/Dockerfile` in a clone, reinstall (`cargo install --path .`), and run
`csb build --force`. csb hashes the baked-in guidance and stamps it on the image, so it tells
you when a running image is older than your binary.

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
  Dockerfile, and stay memory-aware.
- **TTY handling** — stdin is always forwarded, but a pseudo-TTY is only requested when both
  stdin and stdout are terminals, so `echo x | csb -p ...` and `csb -p ... | grep` both behave.
- **Memory cap + diagnostics** for the OOM failure mode below.

## Troubleshooting

**Frozen UI / "I can't type" / timer stuck.** Almost always **OOM**. The VM has a fixed memory
cap (default 6 GB) and **no swap**, so a memory-hungry step — especially `next build`,
bundlers, or large test suites — can exhaust it and wedge the whole VM, including the TUI and
even `container exec`. From another terminal:

```sh
csb doctor          # container ls + `container stats` (reads from the host, works when exec hangs)
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

**Can't reach the dev server in my browser.** No container port is published to the host, and
the container's `localhost` is private to the VM. Run your dev server on the host instead.

**Service down after reboot.** `container system start` (or `brew services start container`).

**Builder VM eating RAM after a build.** csb stops it automatically; `container builder stop`
if something else started it.

## Development

```sh
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
```

## License

[MIT](./LICENSE)

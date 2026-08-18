# csb — Rust CLI Implementation Plan

## Overview

Convert csb from a sourced shell script (`csb.sh` + `install.sh`) to a standalone Rust CLI binary. The binary embeds all assets (Dockerfile, sandbox-guidance.md), is distributable via `cargo install` and Homebrew, and is architected for extensibility.

---

## 1. Project Structure

```
csb/
├── Cargo.toml
├── build.rs                    # embed version, validate assets exist at compile time
├── src/
│   ├── main.rs                 # entry point, clap parse, dispatch
│   ├── cli.rs                  # clap command/subcommand definitions
│   ├── commands/
│   │   ├── mod.rs
│   │   ├── run.rs              # default: launch sandboxed Claude Code
│   │   ├── doctor.rs           # diagnostics from the host
│   │   └── build.rs            # build/rebuild the sandbox image
│   ├── container.rs            # wrapper for shelling out to `container` CLI
│   ├── config.rs               # env var loading with defaults
│   └── image.rs                # image existence check, staleness detection, asset extraction
├── assets/
│   ├── Dockerfile              # embedded into binary via include_str!
│   └── sandbox-guidance.md     # embedded into binary via include_str!
├── README.md
└── LICENSE
```

Single crate, not a workspace — there's no reason for multiple crates at this scale. The `commands/` module is the extension point: adding a command means adding a file there, a clap subcommand variant, and a match arm.

## 2. CLI Design

### Command structure

```
csb [flags-for-claude...]           # launch Claude Code sandboxed (default command)
csb run [flags-for-claude...]       # explicit form of the above
csb doctor                          # host-side diagnostics
csb build [--force]                 # build/rebuild the claude-box image
csb help                            # clap-generated help
csb --version                       # print version
```

### Subcommand naming and collision

The subcommands `run`, `doctor`, `build` are reserved. If a user wants to pass a word that collides with a subcommand name to claude, they use the explicit separator: `csb -- doctor` passes `doctor` to claude. This is standard CLI convention. The `run` subcommand is always available as the unambiguous explicit form: `csb run --continue` is identical to `csb --continue`.

### Flag passthrough

The default (no subcommand) and `run` subcommand pass all trailing arguments through to `claude` inside the container. Use clap's `TrailingVarArg` / `allow_external_subcommands` so unknown flags aren't rejected:

```rust
#[derive(Parser)]
#[command(name = "csb", version, about = "Sandboxed Claude Code on Apple container")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Arguments passed through to claude (when no subcommand is given)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    claude_args: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Launch Claude Code in the sandbox (same as bare `csb`)
    Run {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        claude_args: Vec<String>,
    },
    /// Host-side diagnostics for a running sandbox
    Doctor,
    /// Build or rebuild the sandbox image
    Build {
        /// Rebuild even if the image already exists
        #[arg(long)]
        force: bool,
    },
}
```

Version is handled by clap's built-in `#[command(version)]` on the `Cli` struct, providing `csb --version` / `csb -V`. No separate `Version` subcommand — that would be redundant.

When `command` is `None` and `claude_args` is non-empty, dispatch to `run`. This preserves the `csb --continue` UX.

### Future extensibility

New commands are added by:
1. Adding a variant to `Commands`
2. Adding a `commands/foo.rs` with an `execute()` function
3. Adding the match arm in `main.rs`

No framework, no plugin system, no trait indirection — just an enum and a module.

## 3. Functionality Mapping

### `csb` / `csb run` (was: `csb()` shell function)

Translates to spawning `container run` with the same arguments:

```
container run -it --rm \
  -m <memory> \
  -v <pwd>:/workspace -w /workspace \
  -v <home>/.claude:<home>/.claude \
  -e CLAUDE_CONFIG_DIR=<home>/.claude \
  <image> claude --dangerously-skip-permissions [claude_args...]
```

Implementation:
- Resolve `$PWD` via `std::env::current_dir()`
- Resolve `$HOME` via `std::env::var("HOME")` (prefer the env var for parity with the shell version)
- Check that `container` is in PATH (`which::which("container")`)
- Start the container service first: `container system start` (ignore failure, same as shell)
- Ensure the image exists: `container image ls` and check for the image name. If missing, print "Sandbox image not found — building now..." and run the full build (including `container builder stop` to reclaim RAM afterward). No interactive prompt — just do it, since the user clearly wants to use csb.
- **TTY detection:** Check `std::io::stdin().is_terminal()` (stable since Rust 1.70). If stdin is a TTY, pass `-it` to `container run` (interactive + pseudo-TTY). If not (piped/redirected), omit `-t` (and potentially `-i`) so non-interactive usage like `csb --print "hello" | grep foo` works without garbled output.
- Spawn `container run` via `std::process::Command`, using `.status()` to inherit stdio (the TUI must be interactive)
- Forward the exit code from the container process

**Signal handling:** `std::process::Command::status()` inherits the signal disposition from the parent, so Ctrl+C is delivered to the container process (the foreground process group). Verify this works correctly during manual testing — the TUI should exit cleanly without csb printing any extra error messages. If needed, install a SIGINT handler with `ctrlc` crate that simply waits for the child to exit.

### `csb doctor` (was: `csb-doctor()` shell function)

Runs `container ls` and `container stats --no-stream`, printing results. Additionally:
- Check for shell function shadowing: look for `csb()` function definition in `~/.zshrc` and `~/.bashrc`. If found, warn: "Shell function 'csb' found in ~/.zshrc — it shadows this binary. Remove the `source .../csb.sh` line to use the Rust CLI."
- Check image staleness (see section 4).

### `csb build` (new, was: part of `install.sh`)

- Create a temp directory (`tempfile` crate)
- Write the embedded Dockerfile as `Dockerfile` and the embedded guidance as `sandbox-guidance.md` (these exact filenames must match the `COPY sandbox-guidance.md` instruction in the Dockerfile)
- Run `container build -t <image> <tempdir>`
- Run `container builder stop` to reclaim RAM
- The temp directory is cleaned up automatically by `tempfile`'s drop

The `--force` flag skips the "image already exists" check. Without it, `csb build` still rebuilds (it's idempotent) but prints a note that it's replacing the existing image.

## 4. Embedded Assets Strategy

The Dockerfile and sandbox-guidance.md are compiled into the binary:

```rust
// src/image.rs
const DOCKERFILE: &str = include_str!("../assets/Dockerfile");
const SANDBOX_GUIDANCE: &str = include_str!("../assets/sandbox-guidance.md");

// The Dockerfile contains `COPY sandbox-guidance.md ...` — the filename written
// to the temp build context MUST match this. Use constants to prevent drift:
const DOCKERFILE_NAME: &str = "Dockerfile";
const GUIDANCE_NAME: &str = "sandbox-guidance.md";
```

When building an image, these are written to a temp directory which serves as the build context. This means:
- `cargo install csb` works without cloning the repo
- Homebrew installs work without keeping source around
- The user can still customize by running `container build` manually with their own Dockerfile

### Image staleness detection

The `assets/Dockerfile` includes a label placeholder:

```dockerfile
LABEL csb.assets.sha256="{{CSB_ASSETS_SHA256}}"
```

This placeholder must be added to the Dockerfile in `assets/` as part of the Rust migration.

**Hash computation (avoiding the chicken-and-egg):** The SHA-256 is computed over the **sandbox-guidance.md content only** — not the Dockerfile. This avoids circularity: the Dockerfile contains the placeholder, and hashing the Dockerfile-with-placeholder would produce a hash that changes when injected. The guidance file is the content most likely to change between versions anyway. `csb build` computes the hash, replaces `{{CSB_ASSETS_SHA256}}` in the Dockerfile string before writing to the temp dir. `csb doctor` and `csb run` (before launch) compare the current binary's guidance hash against the label on the existing image (via `container image inspect`). If they differ, print: "Sandbox image was built with an older version of csb. Run `csb build` to update."

`assets/` contains the canonical copies. The old root-level `Dockerfile` and `sandbox-guidance.md` are removed after migration.

## 5. Configuration

### Env vars (backward compatible)

| Var | Default | Purpose |
|---|---|---|
| `CSB_MEMORY` | `6g` | VM memory cap |
| `CSB_IMAGE` | `claude-box` | Image name |

These are read at runtime via `std::env::var()` with fallback defaults, matching the current shell behavior exactly.

### Config file (future, not in v1)

A `~/.config/csb/config.toml` could be added later for persistent config. Not needed for the initial release — env vars are sufficient and match the current UX. The `config.rs` module is the extension point; it currently just reads env vars.

## 6. Error Handling

Use `anyhow` for the error type — it's the right weight for a CLI that mostly shells out to subprocesses.

Specific error conditions to handle with clear messages:

| Condition | Detection | Message |
|---|---|---|
| `container` not installed | `which::which` fails | "Apple 'container' not found. Install: brew install container (requires macOS 26+)" |
| Container service not running | `container system start` fails | Auto-start silently (matching shell behavior); only error if `container run` itself fails |
| Image not built | `container image ls` doesn't list the image | "Sandbox image not found — building now..." then auto-build |
| Image stale | Asset hash mismatch against image label | "Sandbox image was built with an older csb version. Run `csb build` to update." (warning, not blocking) |
| `container run` fails | Non-zero exit code | Forward the exit code; don't wrap it in extra messaging unless it's a known error pattern |
| OOM / frozen VM | (detected by `csb doctor`, not by `csb run`) | `csb doctor` highlights when MEM is near the cap |
| No `$HOME` | env var missing | "Could not determine home directory" |
| Permission denied on `~/.claude` | `container run` fails with mount error | "Cannot mount ~/.claude — check directory permissions" |
| Shell function shadows binary | `csb doctor` checks rc files | "Shell function 'csb' found in ~/.zshrc — it shadows this binary." |

Exit codes: forward the container's exit code for `csb run`. Use exit code 1 for csb's own errors.

## 7. Distribution

### cargo install

Before publishing, verify the crate name `csb` is available on crates.io. If taken, use `claude-sandbox` or `claude-sb` as the crate name (the binary name stays `csb` via `[[bin]]` in Cargo.toml).

```sh
cargo install csb    # or cargo install claude-sandbox
```

The `include_str!` assets are compiled in, so no post-install step is needed. The user still needs to run `csb build` once to create the container image (or it auto-builds on first `csb` invocation).

### Homebrew — pre-compiled binary (primary strategy)

Use a GitHub Actions release workflow to produce a pre-compiled `aarch64-apple-darwin` binary (the only relevant target — Apple container is macOS-only, Apple Silicon only since Tahoe). The Homebrew formula downloads this binary directly, avoiding a multi-minute compile from source.

Create a tap at `shareefhadid/homebrew-tap`:

```ruby
class Csb < Formula
  desc "Sandboxed Claude Code on Apple container"
  homepage "https://github.com/shareefhadid/csb"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/shareefhadid/csb/releases/download/v0.1.0/csb-aarch64-apple-darwin.tar.gz"
      sha256 "..."
    end
  end

  def install
    bin.install "csb"
  end

  def caveats
    <<~EOS
      csb requires Apple 'container' (macOS 26 Tahoe or newer):
        brew install container

      First run builds the sandbox image automatically.
      Override the memory cap: export CSB_MEMORY=8g
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/csb --version")
  end
end
```

Installation: `brew install shareefhadid/tap/csb`

### GitHub Actions release workflow

On tag push (`v*`):
1. `cargo test` on macOS
2. `cargo build --release --target aarch64-apple-darwin`
3. Strip and tar the binary
4. Create a GitHub Release with the tarball attached
5. Update the Homebrew formula SHA (can be automated with `brew bump-formula-pr` or a manual step)

### Signing

macOS Gatekeeper may quarantine unsigned binaries downloaded from the internet. For v1, document the `xattr -d com.apple.quarantine` workaround in the README. For wider adoption, set up Apple Developer ID signing in the release workflow (or use `ad-hoc` signing which avoids the "unidentified developer" dialog on some configurations).

### GitHub Releases

Tag releases, attach the compiled binary. The Homebrew formula points at the binary tarball.

## 8. Migration Path

### For existing users

1. Remove the `source .../csb.sh` line from `~/.zshrc` / `~/.bashrc` — **important:** if you skip this, the shell function shadows the binary and you'll silently run the old version. `csb doctor` detects this and warns.
2. Install the Rust binary (`brew install shareefhadid/tap/csb` or `cargo install csb`)
3. The existing `claude-box` image works as-is — no rebuild required unless the Dockerfile changed (csb will warn you about staleness)
4. `~/.claude` is used identically — no migration needed
5. `CSB_MEMORY` and `CSB_IMAGE` env vars work identically

### In the repo

- `csb.sh` and `install.sh` are removed
- `Dockerfile` and `sandbox-guidance.md` move to `assets/`
- README is updated with new install/usage instructions
- The first Rust release gets a clear "migrating from the shell version" section

### Behavioral parity checklist

- [ ] `csb` with no args launches Claude interactively
- [ ] `csb --continue` passes `--continue` to claude
- [ ] All unknown flags pass through to claude
- [ ] `container system start` is called before `container run`
- [ ] `~/.claude` is mounted at its real host path (not `/root/.claude`)
- [ ] `CLAUDE_CONFIG_DIR` env var is set inside the container
- [ ] `IS_SANDBOX=1` is set via the Dockerfile (unchanged)
- [ ] `csb doctor` shows `container ls` and `container stats`
- [ ] Exit code from claude is forwarded

## 9. Testing Strategy

### Unit tests

- `config.rs`: env var parsing with defaults, edge cases (empty string, whitespace)
- `cli.rs`: clap parsing — cover these cases specifically:
  - `csb` (no args) → dispatches to run with empty claude_args
  - `csb --continue` → dispatches to run with `["--continue"]`
  - `csb run --continue --verbose` → dispatches to run with `["--continue", "--verbose"]`
  - `csb -- --help` → passes `--help` to claude, does NOT show csb help
  - `csb -p "prompt with spaces"` → passes correctly
  - `csb doctor` → dispatches to doctor
  - `csb build --force` → dispatches to build with force=true
  - `csb --version` → prints version (clap built-in)
  - `csb run -- --help` → explicit `run` subcommand with `--` separator passes `--help` to claude
- `image.rs`: asset extraction writes correct files to temp dir, filenames match Dockerfile COPY expectations
- `image.rs`: auto-build followed by explicit `csb build` is idempotent (no errors on rebuild)

### Integration tests (require `container` CLI — skip in CI if unavailable)

- `csb build` builds an image successfully
- `csb doctor` runs without error (even if no container is running)
- `csb --version` prints version string

### What NOT to test

- Don't test `container` itself — that's Apple's problem
- Don't mock the container CLI in elaborate ways — the integration is simple subprocess calls; real tests against the actual CLI (when available) are more valuable than mocks
- Don't test Claude Code's behavior inside the sandbox

### CI

GitHub Actions on macOS runners. The `container` CLI may not be available in CI (macOS 26 is very new), so integration tests should be gated behind an env var (`CSB_INTEGRATION_TESTS=1`). Unit tests run everywhere.

### Test naming

Use descriptive names following the `<unit>_should_<expected>_when_<condition>` convention:

```rust
#[test]
fn run_should_error_when_container_not_installed() { ... }

#[test]
fn config_should_use_default_when_env_var_missing() { ... }

#[test]
fn cli_should_passthrough_flags_when_no_subcommand() { ... }
```

## 10. Dependencies

```toml
[package]
name = "csb"   # verify availability on crates.io; fallback: "claude-sandbox"
version = "0.1.0"
edition = "2021"
description = "Sandboxed Claude Code on Apple container"
license = "MIT"
repository = "https://github.com/shareefhadid/csb"

[[bin]]
name = "csb"

[dependencies]
clap = { version = "4", features = ["derive"] }
anyhow = "1"
which = "7"
tempfile = "3"
sha2 = "0.10"         # asset hash for image staleness detection

[dev-dependencies]
assert_cmd = "2"
predicates = "3"              # output assertions in integration tests
```

Minimal dependency tree. No async runtime needed — everything is synchronous subprocess calls. Dropped `dirs` crate — `std::env::var("HOME")` is sufficient and matches shell parity. Using `edition = "2021"` (not 2024) to avoid requiring Rust 1.85+ for `cargo install` users with older toolchains.

### Lints

Configure workspace lints in Cargo.toml rather than per-file attributes:

```toml
[lints.clippy]
redundant_clone = "warn"
large_enum_variant = "warn"
needless_collect = "warn"
```

CI runs `cargo clippy --all-targets --all-features -- -D warnings` to fail on any lint. Use `#[expect(clippy::lint)]` (not `#[allow]`) for intentional suppressions, with a justification comment.

### Code conventions

- **No `unwrap()` in production code.** Use `.with_context(|| ...)` from anyhow for subprocess errors and fallible operations. `unwrap()` / `expect()` are fine in tests only.
- **Visibility:** Internal helpers in `container.rs`, `config.rs`, `image.rs` should be `pub(crate)`, not `pub`. Only `commands/mod.rs` dispatch and `main.rs` need truly public items.
- **Borrowing:** Prefer `&str` over `String` and `&[T]` over `Vec<T>` in function parameters where ownership isn't needed.

## 11. Implementation Order

### Phase 1: Core parity (the MVP)
1. Scaffold the Cargo project, add dependencies
2. Implement `cli.rs` with clap definitions
3. Implement `config.rs` — env var loading with defaults
4. Implement `container.rs` — helper to run `container` subprocesses
5. Implement `commands/run.rs` — the main `csb` command with full parity to the shell function
6. Implement `commands/doctor.rs` — including shell-shadow detection
7. Move Dockerfile + sandbox-guidance.md to `assets/`, implement embedded extraction in `image.rs`
8. Implement `commands/build.rs` — with asset hash label injection
9. Implement auto-build on first run (image doesn't exist → build automatically)
10. Implement image staleness warning (hash comparison)
11. Write unit tests for CLI parsing, config, and asset extraction
12. Manual smoke test: verify behavioral parity with the shell version

### Phase 2: Distribution
13. Set up GitHub Actions for CI (cargo test, cargo clippy --all-targets -- -D warnings, cargo fmt --check, cargo audit)
14. Add a release workflow: build aarch64-apple-darwin binary, create GitHub Release with tarball
15. Create the `shareefhadid/homebrew-tap` repo with the binary formula
16. Verify crate name availability, publish to crates.io
17. Update README with new install instructions + migration section

### Phase 3: Cleanup
18. Remove `csb.sh`, `install.sh` from the repo
19. Tag v0.1.0

---

## Review Scores

*Scores from adversarial review — see feedback integration notes below.*

### Round 1 (external reviewer)

| # | Dimension | Score | Notes |
|---|---|---|---|
| 1 | Completeness | 4/5 | Missing install.sh onboarding equivalent, Dockerfile COPY filename assumption |
| 2 | CLI Design | 4/5 | Subcommand name collision not addressed, version redundancy |
| 3 | Architecture | 5/5 | Appropriately simple, right abstraction boundary |
| 4 | Distribution | 3/5 | Source-compile Homebrew formula, crate name unchecked, deprecated syntax |
| 5 | Migration | 4/5 | No shell function shadowing warning |
| 6 | Embedded Assets | 4/5 | No staleness detection, COPY filename not made explicit |
| 7 | Error Handling | 4/5 | Signal handling unaddressed, auto-build UX ambiguous |
| 8 | Testing | 4/5 | No adversarial passthrough tests |
| 9 | Extensibility | 5/5 | Perfect for this scale |
| 10 | Feasibility | 5/5 | Deliverable without yak-shaving |
| | **Average** | **4.2** | **PASS** |

### Round 1 feedback → Round 2 revisions

All feedback items from the adversarial review were addressed in this revision:

- **Distribution:** Pre-compiled binary is now the primary Homebrew strategy; source-compile formula removed. Crate name availability check added. Deprecated `depends_on :macos` removed. Signing/Gatekeeper addressed.
- **CLI Design:** `csb version` subcommand removed in favor of clap's `#[command(version)]` (`csb --version`). Subcommand collision documented with `--` separator convention.
- **Embedded Assets:** Explicit filename constants added. Image staleness detection via SHA-256 label added. `sha2` crate added to dependencies.
- **Migration:** Shell function shadowing detection added to `csb doctor`. Migration instructions warn about it explicitly.
- **Error Handling:** Signal handling (Ctrl+C) explicitly addressed. Auto-build is non-interactive ("building now..." not "should I build?").
- **Testing:** Adversarial clap passthrough test cases added (`csb -- --help`, `csb -p "prompt with spaces"`, etc.).

### Round 2 (external reviewer)

| # | Dimension | Score | Notes |
|---|---|---|---|
| 1 | Completeness | 4/5 | Auto-build path missing `container builder stop`; no LABEL placeholder in Dockerfile |
| 2 | CLI Design | 5/5 | Clean, intuitive, collision documented |
| 3 | Architecture | 5/5 | Single crate, flat modules, no premature abstraction |
| 4 | Distribution | 4/5 | Pre-compiled binary correct; `edition = "2024"` may surprise older Rust users |
| 5 | Migration | 5/5 | Thorough, shell shadowing detection excellent |
| 6 | Embedded Assets | 4/5 | SHA-256 chicken-and-egg problem; no LABEL in current Dockerfile |
| 7 | Error Handling | 4/5 | Missing TTY detection for `-it` flag in non-interactive contexts |
| 8 | Testing | 4/5 | Missing `csb run -- --help` test case; missing `predicates` crate |
| 9 | Extensibility | 5/5 | Correct pattern, no over-engineering |
| 10 | Feasibility | 5/5 | Minimal deps, straightforward implementation |
| | **Average** | **4.5** | **PASS** |

### Round 2 feedback → Final revisions

- **Auto-build:** Now explicitly includes `container builder stop` after the auto-build in `csb run`.
- **SHA-256 chicken-and-egg:** Hash is now computed over **guidance content only** (not the Dockerfile), avoiding circularity. Documented clearly.
- **LABEL placeholder:** Plan now specifies that `LABEL csb.assets.sha256="{{CSB_ASSETS_SHA256}}"` must be added to `assets/Dockerfile` during migration.
- **Rust edition:** Changed from `edition = "2024"` to `edition = "2021"` for broader toolchain compatibility.
- **TTY detection:** Added `std::io::stdin().is_terminal()` check to conditionally include `-it` / `-t` flags on `container run`, enabling non-interactive usage (piped output, scripts).
- **Testing:** Added `csb run -- --help` test case and auto-build idempotency test. Added `predicates = "3"` to dev-dependencies.

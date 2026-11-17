# AGENTS.md

Rust System Sentinel (`sentinel`): a Linux-only monitoring TUI/CLI reading `/proc`, `sysfs`, cgroups, and pressure/thermal files. All system data comes from filesystem reads — no daemons or external collectors.

## Commands

- `make lint` — CI's real gate: `cargo clippy --all-targets -- -D warnings` **and** `cargo fmt --check`. Both must pass; clippy denies warnings.
- `cargo test` — all tests. Single test: `cargo test cli -- --test-threads=1` (or `cargo test <name>`).
- `make release`, `make install` (uses `PREFIX`/`DESTDIR`), `make package` — tarball into `build/`.
- Run without installing: `cargo run -- <command>`; TUI via `cargo run -- tui` (needs a real terminal).

## Testing quirks

- `tests/cli.rs` and `tests/packaging.rs` spawn the **real built binary** via `env!("CARGO_BIN_EXE_sentinel")` — integration tests compile the bin first; a compile error fails as a test failure.
- CLI tests are sequential and shell out; expect them to be slower than unit tests.
- Fixtures live in `tests/fixtures/`.

## Layout (only what isn't obvious from names)

- `src/collect/` — per-source samplers (proc, filesystem, net, sensors); `src/core/` — pure formatting/metrics types shared by CLI and TUI.
- `src/alert.rs` / `policy.rs` / `oom.rs` / `action.rs` — alerting pipeline: thresholds → policy decisions → actions (incl. OOM handling).
- `src/tui/app.rs` is the terminal UI; `examples/dashboard.rs` renders a dashboard snapshot without a TTY — use it to eyeball view changes.
- `src/commands.rs` wires CLI subcommands; `src/config.rs` is the config file layer.

## Rules

- README must stay generic: never include real hardware or OS details from
  the dev machine (hostnames, CPU/GPU models, kernel versions, disk/thermal
  layouts). Describe what the tool reads (`/proc`, `sysfs`, ...), not this machine.

- Commit policy: always ask before committing. When committing, the message
  must be detailed — a bullet list of each change.

- Versioning: the user bumps `Cargo.toml` manually. Agent's job is to notice
  when a bump is warranted and suggest the version number — never bump,
  tag, or create releases without explicit approval of the exact version
  and commit.

## Gotchas

- Linux-specific by design: paths like `/proc/pressure`, `/sys/class/*` are read directly; don't abstract them behind platform traits without cause.
- Never commit or read for context: `build/` (binary + tarball artifacts), `graphify-out/` (3.6 MB generated graph), `.scratch/`. All untracked.
- Version comes from `Cargo.toml`; the Makefile extracts it via `cargo metadata`. Packaging tests assert `CARGO_PKG_VERSION` matches CLI output — bump the version, not hardcoded strings.

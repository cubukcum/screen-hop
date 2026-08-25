# Contributing to screen-hop

Thanks for helping! screen-hop is a cross-platform Rust + Slint utility that lets one PC switch one
selected monitor between two locally confirmed inputs through DDC/CI. Please read
[README.md](README.md) and the product definition in
[docs/PLAN-screen-hop.md](docs/PLAN-screen-hop.md) first.

## Ground rules

screen-hop is **deliberately honest about hardware limits** (see the README "Honest boundaries").
Contributions must keep that posture: never fake a switch, never paper over a per-monitor failure,
and never weaken the soft-brick guard: only the two values observed for the selected monitor may
be written, and a quirk can *restrict* but never *authorize* a write.

## Prerequisites

- Rust **1.92+** (the floor is set by the Slint UI stack; CI builds on current stable).
- Windows is the primary target; the pure-logic crates build and test on any platform.

## Build & test

```sh
cargo build --workspace
cargo test  --workspace
```

Before opening a PR, run what CI runs (see [.github/workflows/ci.yml](.github/workflows/ci.yml)) —
all four must pass:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --all-targets
cargo test  --workspace
```

The M0 hardware spike (interactive; reads/writes a real monitor's input source):

```sh
cargo run -p screenhop-spike            # interactive menu
cargo run -p screenhop-spike -- list    # just enumerate panels
```

## Where things live

| Crate | Responsibility |
|---|---|
| `screenhop-core` | domain types, `MonitorDriver`/`Delayer`/`Clock` traits, actuation state machine + soft-brick guard |
| `screenhop-ddc` | `ddc-hi`-backed `MonitorDriver` (Windows/Linux/macOS) |
| `screenhop-identity` | EDID fingerprint and local monitor collision/labeling helpers |
| `screenhop-quirks` | panel-global quirks DB (merge precedence user > local > shipped) |
| `screenhop-app` | two-source local configuration, persistence, and guarded switch selection |
| `screenhop-ui` | Slint local switch/setup surfaces and the dedicated DDC worker |
| `screenhop-spike` | opt-in real-hardware compatibility and round-trip checks |

## Tests

- Pure-logic crates are unit-tested on every platform; prefer adding a focused test with each change.
- Invariants that must hold across arbitrary inputs (e.g. the soft-brick guard) use **property
  tests** (`proptest`) — see `screenhop-core/src/executor.rs`.
- Behaviour that needs real hardware (especially the one-PC `A -> B -> A` round trip) is
  **manual / opt-in** — document the steps rather than faking it in CI (CI has no DDC hardware).

## Pull requests

1. Branch off `main`; keep PRs focused.
2. Match the surrounding code's style and comment density. CI enforces `rustfmt` + `clippy`.
3. Describe what you changed and how you verified it (paste test output; note any manual hardware
   steps you ran).
4. By contributing you agree your work is licensed under the **MIT License** (see
   [LICENSE](LICENSE)).

## Contributing a panel quirk

Adding behaviour facts for a specific monitor model? That has its own short guide:
[docs/contributing-quirks.md](docs/contributing-quirks.md).

## Reporting security issues

Please follow [SECURITY.md](SECURITY.md) — do not open a public issue for vulnerabilities.

# screen-hop

**Switch one monitor between two inputs from one PC.**

screen-hop is a small cross-platform desktop utility for a simple desk setup: choose one monitor,
teach the app its two input sources, then move back and forth without reaching for the monitor's
physical source button. It works locally through DDC/CI. There is no LAN connection, pairing,
cloud service, account, or second screen-hop installation.

## How it works

Modern monitors often expose their input selector as DDC/CI VCP feature `0x60`. screen-hop keeps
exactly two input values that it observed on the selected monitor and writes only those values.
The app retries slow writes, checks read-back when the monitor supports it, and reports an
inconclusive result honestly when a write was accepted but could not be verified.

The normal flow is deliberately small:

1. Select one locally connected monitor.
2. Capture the input currently showing this PC as Source A.
3. Start listening, then use the monitor's physical control once to move to Source B.
4. screen-hop observes Source B and attempts to return to Source A.
5. Name the two sources and use the compact switch window from then on.

## Important hardware boundary

Local control depends on the monitor continuing to accept DDC/CI commands from this PC's cable
while another input is visible. Some monitor/GPU/cable combinations do; some stop exposing DDC as
soon as the input changes.

The setup round trip is therefore a real compatibility test, not decorative onboarding. If the
monitor cannot complete `A -> B -> A` from this one controlling PC, software cannot guarantee the
return trip. The physical input button remains the recovery path.

Other limits:

- Enable DDC/CI in the monitor's on-screen menu.
- A switch usually takes 1–3 seconds and may retry.
- screen-hop never probes arbitrary input codes.
- A disconnected, sleeping, or re-enumerated display may require reopening setup.
- A DDC call stuck inside an OS/backend cannot be cancelled; reopen the app if an operation hangs.
- Pre-OS, BIOS, and login-screen control are outside the app's reach.

## Status

The guarded DDC engine, retry/read-back behavior, monitor fingerprinting, local persistence, and
two-source application model are covered by automated tests. Real hardware still decides whether
the one-PC round trip works for a particular setup; use the included spike before relying on it.

The current UI is a compact Slint window. A native system-tray icon and global hotkey are separate
future enhancements; they are not silently simulated by the current build.

## Build and run

Requires stable Rust 1.92 or newer.

```sh
cargo build --workspace
cargo test --workspace

# Run the real local app (first run opens setup)
cargo run -p screenhop-ui

# Read-only monitor diagnostics
cargo run -p screenhop-ui -- --monitors

# Guided hardware compatibility tests
cargo run -p screenhop-spike

# Explicit design preview / snapshot modes
cargo run -p screenhop-ui -- --preview
cargo run -p screenhop-ui -- --preview --shot out.png
```

`SCREENHOP_CONFIG_DIR` can override the per-user configuration directory for testing or portable
setups.

## Architecture

```text
crates/
  screenhop-core/      guarded DDC switch executor, retries, timing, outcomes
  screenhop-ddc/       ddc-hi monitor enumeration and VCP 0x60 reads/writes
  screenhop-identity/  stable local monitor fingerprinting and collision helpers
  screenhop-quirks/    per-monitor safety restrictions and timing/read-back behavior
  screenhop-app/       two-source local configuration, persistence, and switching model
  screenhop-ui/        Slint local switch window and guided setup
  screenhop-spike/     interactive, opt-in real-hardware compatibility checks
```

The product definition and remaining hardware checks are documented in
[docs/PLAN-screen-hop.md](docs/PLAN-screen-hop.md) and
[docs/REMAINING-CHECKLIST.md](docs/REMAINING-CHECKLIST.md).

## Safety

The most important invariant is simple: screen-hop writes only the two values captured from the
selected monitor, and a blocked value from the quirks database always wins. Corrupt, incomplete,
duplicate, or out-of-range configuration fails closed with no write. See [SECURITY.md](SECURITY.md).

## License

Licensed under the [MIT License](LICENSE).

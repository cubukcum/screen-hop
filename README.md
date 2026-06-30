# screen-hop

Reassign your desk's physical monitors between several PCs over the LAN — from a tray
menu, in one click, without reaching for each monitor's physical input-source button.

screen-hop runs a small **per-PC tray agent**; the agents form a **serverless peer mesh** over the
LAN. When you tell it to "give Monitor 2 to the laptop," the mesh routes a DDC/CI **Input Select**
write (VCP feature `0x60`) to whichever PC can physically drive that panel — by default the *target*
PC switches the monitor **to itself** (the reliable direction) — then reconciles ownership against
the monitor's live `0x60` value as ground truth.

> **Status:** implementation in place across milestones **M0–M6** (domain core, identity, mesh +
> discovery, orchestration/presets/reconcile, UI controller, CI + packaging). What remains is the
> **verification that needs real hardware, a real LAN, and design sign-off** — pull-to-self on ≥2
> rigs, mDNS on a real LAN, the onboarding/soak numbers, and the Slint live-binding to the
> controller. What's code-complete vs. what still needs you is laid out in
> [docs/REMAINING-CHECKLIST.md](docs/REMAINING-CHECKLIST.md) (with the
> [hardware verdict log](docs/hardware/pull-to-self-verdicts.md)). Cross-platform **Rust + Slint**,
> targeting **Windows → Linux → macOS (best-effort)**. See
> [docs/PLAN-screen-hop.md](docs/PLAN-screen-hop.md) for the full product definition, architecture,
> and decision log.

## Honest boundaries (read these first)

screen-hop is only as good as each monitor's DDC/CI implementation, and it is deliberately honest
about the limits:

- Behavior is **per-monitor** and must be discovered on real hardware (a built-in calibration step).
- It **cannot** touch BIOS / pre-OS / boot / lock screens — DDC/CI needs a running, logged-in session.
- Switching is **not instant** (~1–3 s, occasional retry) and is shown as progress, never faked.
- If the PC that must drive a panel is off/asleep, that monitor is **stranded** — there is no software
  recovery; the physical OSD button is the honest fallback, and the UI says so.
- Multi-monitor presets are **best-effort**, never atomic — per-monitor success/failure is surfaced.

## Workspace layout

```
crates/
  screenhop-core/      domain types, MonitorDriver/Delayer/Clock traits, actuation state machine
  screenhop-ddc/       ddc-hi-backed MonitorDriver (Windows / Linux / macOS)
  screenhop-identity/  EDID fingerprint, collision/labeling, per-(peer,monitor) calibration
  screenhop-net/       AEAD transport (XChaCha20-Poly1305), Ed25519 handshake + TOFU pinning, wire schema
  screenhop-state/     per-monitor lease lock, last-writer-wins ownership map
  screenhop-quirks/    panel-global quirks DB (merge precedence user > local > shipped)
  screenhop-app/       mesh node + orchestration (routing, blind-point, presets, partition guard)
  screenhop-ui/        Slint tray UI + onboarding wizard surfaces
  screenhop-spike/     M0 hardware feasibility spike (enumerate/read/write 0x60)
quirks/quirks.json     shipped community quirks DB
docs/                  plan + Claude Design handoff
```

## Build & test

Requires Rust **1.82+** (the workspace MSRV).

```sh
cargo build --workspace
cargo test  --workspace      # pure-logic crates; DDC/UI need their platform toolchains
```

The M0 hardware spike (interactive; reads/writes a real monitor's input source):

```sh
cargo run -p screenhop-spike            # interactive menu
cargo run -p screenhop-spike -- list    # just enumerate panels
```

UI design-preview / snapshot mode (renders a surface to PNG for design diffing):

```sh
cargo run -p screenhop-ui -- --screen flyout --dark
cargo run -p screenhop-ui -- --shot out.png --screen deskmap
```

## Security model (summary)

A single shared **mesh secret** is stretched with Argon2id into a group key; **every** mesh message
is encrypted + authenticated with XChaCha20-Poly1305. Each install has an **Ed25519 identity** that
is pinned trust-on-first-use, so a changed key for a known peer is refused. Control is **LAN/Private
only** — no WAN, no UPnP. The threat model is denial-of-visibility by an *unpaired* host on a
personal LAN; an already-paired peer run by the same operator is out of scope. See plan §9.

## Contributing & security

- [CONTRIBUTING.md](CONTRIBUTING.md) — build/test, the CI gates, and the workspace map.
- [docs/contributing-quirks.md](docs/contributing-quirks.md) — how to submit a panel quirk.
- [SECURITY.md](SECURITY.md) — threat model summary, how to report a vulnerability, and the
  Windows/DPAPI re-pair caveat.

## License

Dual-licensed under **MIT OR Apache-2.0** — see [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE). You may use either at your option.

# Changelog

## v0.1.0-alpha — first agent build (pre-release)

> ⚠️ **Pre-release / alpha.** The full agent is implemented and all automated tests pass, but it has
> **not yet been verified on real hardware** (a two-PC, shared-monitor switch). For testers and
> contributors — not for production use.

### Added
- **Live agent** (`screenhop-ui --live`): joins the LAN mesh and routes a tray click into a real
  DDC/CI input switch (pull-to-self), with discovery (mDNS + manual hosts), per-monitor lease
  locking, and a tray driven by live mesh state (in-flight feedback, ownership, degraded).
- **Calibration** (`screenhop-ui --calibrate`): learns and persists this PC's input value per panel.
- **Persistence**: per-user config directory with atomic writes — identity, mesh secret, TOFU pins,
  calibration, labels, config.
- **Reconcile sweep**: periodically re-reads each panel's live `0x60` and corrects ownership after an
  external OSD-button change.
- **No-admin Windows installer** (Inno Setup) with opt-in per-user autostart and clean uninstall.
- **Encrypted LAN mesh**: XChaCha20-Poly1305 + Argon2id group key, Ed25519 trust-on-first-use
  pinning, replay/sequence guards. LAN-only.
- **Orchestration**: named presets (best-effort, partial-failure surfaced), blind-point warning,
  stranded + DDC-disabled states, partition guard.
- **Soft-brick guard** with a property test; measurement/soak harness skeleton.
- CI (build / test / clippy / fmt + MSRV-1.82 + binaries + installer, all with SHA-256), MIT
  license, and contributor + security docs.

### Known limitations / not yet done
- **Not verified end-to-end on real hardware** — the core reason this is an alpha.
- In-window onboarding wizard not wired (use the `mesh-secret` file + `--calibrate`).
- No active-console-session guard yet — don't rely on it over RDP or a locked screen (D11).
- `WM_DISPLAYCHANGE` hook not wired (the periodic sweep covers external changes within ~4 s).
- Secrets stored in plaintext on disk (OS-keystore / DPAPI wrapping is a follow-up).
- Binaries are **unsigned** (SHA-256 sums are published instead).

See [docs/REMAINING-CHECKLIST.md](docs/REMAINING-CHECKLIST.md) for the full status, and
[installer/README.md](installer/README.md) to install + run.

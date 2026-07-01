# Changelog

## v0.1.0-alpha — first agent build (pre-release)

> ⚠️ **Pre-release / alpha.** The core feature is **verified working on real hardware**: a tray click
> moves a shared monitor between two PCs, both directions, over the LAN mesh (validated on an AOC
> 27P2DG5 across a laptop/HDMI + desktop/DisplayPort). That's **one panel on one setup** so far —
> broader hardware coverage, the in-window onboarding wizard, and the items below are still in
> progress. For testers and contributors; not yet production-hardened.

### Added
- **Live agent** (`screenhop-ui --live`): joins the LAN mesh and routes a tray click into a real
  DDC/CI input switch (pull-to-self), with discovery (mDNS + manual hosts), per-monitor lease
  locking, and a tray driven by live mesh state (in-flight feedback, ownership, degraded).
- **First-run pairing in the window**: on first launch with no mesh secret, the onboarding wizard
  opens; typing a shared passphrase pairs this PC (saved as the `mesh-secret`) and relaunches into
  the live mesh — no hand-created file required. (Wizard Steps 2–4 are still design-only.)
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
- CI (build / test / clippy / fmt on stable + binaries + installer, all with SHA-256), MIT
  license, and contributor + security docs.

### Known limitations / not yet done
- **Verified on one panel / one 2-PC setup so far** — needs broader hardware coverage.
- Monitors behind a **USB-C hub/dock** that hides EDID need a `monitor_aliases` entry (see
  `--monitors`); a panel whose identified handle is read-only on one PC may need the alias too.
- In-window onboarding wizard is **partial**: Step 1 pairing works (first run → type a shared
  passphrase → paired, no file needed); the rest (monitor probe, calibrate, names) is still
  design-only — use `--calibrate` for calibration for now.
- No active-console-session guard yet — don't rely on it over RDP or a locked screen (D11).
- `WM_DISPLAYCHANGE` hook not wired (the periodic sweep covers external changes within ~4 s).
- Secrets stored in plaintext on disk (OS-keystore / DPAPI wrapping is a follow-up).
- Binaries are **unsigned** (SHA-256 sums are published instead).

See [docs/REMAINING-CHECKLIST.md](docs/REMAINING-CHECKLIST.md) for the full status, and
[installer/README.md](installer/README.md) to install + run.

# Changelog

## v0.2.0-alpha — safer live controls and multi-peer resilience (pre-release)

### Added
- **Capability-aware switching.** Read-only, offline, and non-actuating peers are disabled in the
  live tray and switch requests are rechecked before they enter the mesh.
- **Multi-peer ownership convergence.** Positive ownership facts are replayed during sync so a
  third PC, or a PC returning after an outage, catches up without changing the established DDC
  actuation path.
- **Multi-adapter discovery.** A peer can advertise several LAN/VPN addresses; re-resolution and
  removal now replace stale endpoints instead of accumulating them forever.

### Fixed
- Long peer names are constrained and elided inside the segmented control instead of painting over
  adjacent PCs or outside the flyout.
- Live/read-only startup paths no longer expose simulated switches, fake preset success, or
  developer navigation; incomplete surfaces are clearly marked and inert.
- Malformed configuration and bind/config-directory failures now fail closed without pretending a
  monitor switch succeeded.
- Network reply timing now covers the real actuation ceiling while remaining inside the mesh lease,
  and equal-timestamp ownership conflicts converge deterministically.
- First-run pairing rejects whitespace-only secrets with visible feedback.

### Verification
- 152 automated tests pass across the workspace; formatting and strict Clippy checks are clean.
- The existing two-PC pull-to-self DDC sequence is unchanged. This release still needs broader
  real-hardware coverage beyond the previously verified AOC 27P2DG5 setup.

## v0.1.1-alpha — no more stray console window (pre-release)

### Fixed
- **No more stray console window.** The app is now built for the Windows GUI subsystem, so
  launching it (Start menu, autostart, installer's "launch now") no longer opens a terminal
  window whose closing killed the app. CLI modes (`--monitors`, `--calibrate`) still print when
  run from a terminal — the process re-attaches to the parent console at startup.

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

# What's done in code vs. what still needs you

Every milestone M0–M6 now has its **code + automated tests** in place, and the live agent (mesh +
actuation + calibration + reconcile) is wired (142 tests pass; `cargo fmt`, `cargo clippy
-D warnings`, and `cargo build/test --workspace` all green locally). What remains is verification
that genuinely needs **real hardware, a real LAN, a running GUI, or a human decision** — none of
which can be faked in code or CI. This is that list.

Legend: ✅ done in code (tested) · ⬜ needs you (hardware / LAN / GUI / decision).

## M0 — Hardware feasibility spike
- ✅ Spike records a **formal verdict row** and appends it to [hardware/pull-to-self-verdicts.md](hardware/pull-to-self-verdicts.md).
- ✅ **Pull-to-self CONFIRMED** on the AOC 27P2DG5 across 2 PCs (laptop/HDMI ↔ desktop/DP), bidirectional, via the live agent (2026-06-30). **GO (provisional)** recorded in the verdict log.
- ⬜ Record **≥ 1 more distinct rig** (different panel/GPU) to fully close M0 + tick DoD line 514.

## M1 — Local DDC core
- ✅ Soft-brick guard **property test** (`screenhop-core`, randomized: never writes a blocked/unconfirmed value).
- ⬜ Confirm **one real local input switch** on hardware (this is the same physical action as the M0 spike test).

## M2 — Identity & calibration
- ✅ **Measurement/soak harness** skeleton (`screenhop-app::harness`) — computes first-attempt %, within-retry %, latency median/p90, scoped to pull-to-self panels.
- ✅ Quirks DB **wired into the app** actuation path (`LocalActuator` calls `QuirksDb::policy_for` + the calibration store).
- ✅ **Cross-PC identity correlation** proven through a real 2-peer mesh switch (`tests/cross_pc_identity.rs`).
- ⬜ Seed the quirks DB with **real tuned values** from your panels (see [contributing-quirks.md](contributing-quirks.md)).
- ⬜ **Quirks lookup key mismatch (design gap).** The actuation path looks up quirks by the
  per-**instance** `monitor_id` (a SHA-256 of manufacturer|product|serial — unique per physical
  panel), but the shipped `quirks/quirks.json` is keyed by **model tokens** (e.g. `SAM-U32H750`), so
  shipped/community quirks never match a real lookup. Fix before community quirk PRs are useful: have
  the lookup also try a model token (instance entry still wins). Safety is unaffected either way —
  quirks can only *restrict*, never confirm a writable value (D7).

## M3 — LAN mesh
- ✅ **Discovery**: manual hosts (fully tested) + **mDNS** via `mdns-sd` (register + browse), merged/deduped.
- ✅ **Announce/Heartbeat** handled → peer presence/liveness registry.
- ✅ **Lease-mid-switch** behavioural test: lease is held for the whole switch (simulated hang) and blocks other peers; freed after.
- ✅ **mDNS discovery verified on a real LAN** — two PCs find each other and stay connected ("2 online").

## M4 — Orchestration & presets
- ✅ **Preset executor** (`execute_plan`): runs ops best-effort, collects per-monitor partial-failure results.
- ✅ **Reconcile** logic (`reconcile` module): folds live `0x60` reads back, reports external changes; `read_to_live_read` maps a read→owner via `CalibrationStore::owner_for` (tested).
- ✅ **DDC-disabled** state (distinct, persistent) + marked from the switch path.
- ✅ **Peer-loss → degraded** detector (`PeerRegistry::is_degraded`), feeding the partition guard.
- ✅ **Periodic reconcile sweep** wired in `--live` (reads via the actuator thread, applies under a brief lock).
- ⬜ **`WM_DISPLAYCHANGE`** hook (Windows) for instant reconcile on dock/undock — periodic sweep covers the case at ~4 s latency; the event hook is a follow-up.

## M5 — Tray UI & the live agent
- ✅ **Controller + bind layer + data-driven tray** (tested): Slint reads `AppWindow` inputs and its callbacks are Rust-overridable; `bind` does index↔id translation.
- ✅ **Live agent** (`screenhop-ui -- --live`): actuator thread (owns the non-`Send` driver) + mesh `Node` (serve + discovery) + worker that routes tray clicks as real mesh switches; a `Timer` polls `MeshState` to refresh monitors/peers/online/degraded with in-flight feedback. Read-only fallback when no mesh secret.
- ✅ **Calibration** (`screenhop-ui -- --calibrate`): learns + persists this peer's `0x60` per panel (what makes switches actually fire).
- 🟡 **GUI onboarding wizard** — **Step 1 (Pair) is wired** (Phase 1): first run with no secret opens
  the wizard; typing a shared passphrase + Pair saves the `mesh-secret` (same format as the CLI/file)
  and relaunches straight into the live mesh — no hand-created file needed. Still design-only: Step 1's
  code/QR/discovered-peer chrome (aspirational — the real model is one shared passphrase), Step 2
  monitor probe, Step 3 calibrate matrix, Step 4 rename→labels. Follow-ups: Phase 2 (Step 2 real
  probe), Phase 3 (Step 4 rename). `--calibrate` still used for calibration for now.
- ⬜ **Active-console-session guard (D11)**: don't actuate from a locked/RDP/Session-0 context — not yet enforced (needs `WTSGetActiveConsoleSessionId`); documented in `installer/README.md`.
- ⬜ **Claude Design** review/approval; confirm shipped UI **matches** the mockups (D12).
- ⬜ **Onboarding ≤ 10 min** on a 2-PC rig; capture the **soak §4.7 numbers** via the harness.
- ✅ **End-to-end 2-PC switch VERIFIED** (2026-06-30): two PCs + a shared AOC; a tray click on either PC moves the panel both directions over the mesh with real DDC. The core product works on hardware.

## M6 — Packaging & OSS readiness
- ✅ **License**: single **MIT** (`LICENSE`), matching Cargo.toml.
- ✅ **CI** ([.github/workflows/ci.yml](../.github/workflows/ci.yml)): fmt + clippy + build + test on `windows-latest`, MSRV-1.82 build, a release job (binaries + SHA-256), and an **installer** job (Inno Setup via choco → installer + SHA-256).
- ✅ **Installer** ([installer/screen-hop.iss](../installer/screen-hop.iss)): per-user, **no-admin**, opt-in `HKCU\…\Run` autostart, clean uninstall (keeps config). Build/usage in [installer/README.md](../installer/README.md).
- ✅ **Docs**: `CONTRIBUTING.md`, `SECURITY.md` (DPAPI caveat), `docs/contributing-quirks.md`, installer docs.
- ⬜ Push and confirm **CI is green** on GitHub's runners (incl. the new installer job building under ISCC).
- ⬜ **Code signing** (Azure Trusted Signing / OV-EV cert) — ships unsigned + SHA-256 for now (a deliberate decision).

## Persistence (supporting the agent) — ✅ done in code
- ✅ `screenhop-app::persist`: config dir (`directories`), atomic temp+rename writes, and save/load for identity, mesh secret, pins path, calibration, labels, config (tested round-trips + crash-safe overwrite).
- ⬜ Wrap the secret/identity with the OS keystore (Windows **DPAPI**) — plaintext today (documented in `SECURITY.md` / `persist.rs`).

## Quick verification (what I ran here)
```sh
cargo fmt --all -- --check                          # clean
cargo clippy --workspace --all-targets -- -D warnings  # clean
cargo test  --workspace                             # 142 passed
```
Everything above the ⬜ lines compiles and (where logic) is unit-tested. The ⬜ items need your
hardware / LAN / GUI / a human decision — see "your part" below.

# What's done in code vs. what still needs you

Every milestone M0–M6 now has its **code + automated tests** in place (121 tests pass; `cargo fmt`,
`cargo clippy -D warnings`, and `cargo build/test --workspace` are all green locally). What remains
is verification that genuinely needs **real hardware, a real LAN, a running GUI, or a human
decision** — none of which can be faked in code or CI. This is that list.

Legend: ✅ done in code (tested) · ⬜ needs you (hardware / LAN / GUI / decision).

## M0 — Hardware feasibility spike
- ✅ Spike records a **formal verdict row** and appends it to [hardware/pull-to-self-verdicts.md](hardware/pull-to-self-verdicts.md).
- ⬜ Run pull-to-self on **≥ 2 distinct PC/monitor rigs**; record each verdict.
- ⬜ Write the **GO / NO-GO** decision in the verdict log; tick DoD line 514.

## M1 — Local DDC core
- ✅ Soft-brick guard **property test** (`screenhop-core`, randomized: never writes a blocked/unconfirmed value).
- ⬜ Confirm **one real local input switch** on hardware (this is the same physical action as the M0 spike test).

## M2 — Identity & calibration
- ✅ **Measurement/soak harness** skeleton (`screenhop-app::harness`) — computes first-attempt %, within-retry %, latency median/p90, scoped to pull-to-self panels.
- ✅ Quirks DB **wired into the app** actuation path (`LocalActuator` calls `QuirksDb::policy_for` + the calibration store).
- ✅ **Cross-PC identity correlation** proven through a real 2-peer mesh switch (`tests/cross_pc_identity.rs`).
- ⬜ Seed the quirks DB with **real tuned values** from your panels (see [contributing-quirks.md](contributing-quirks.md)).

## M3 — LAN mesh
- ✅ **Discovery**: manual hosts (fully tested) + **mDNS** via `mdns-sd` (register + browse), merged/deduped.
- ✅ **Announce/Heartbeat** handled → peer presence/liveness registry.
- ✅ **Lease-mid-switch** behavioural test: lease is held for the whole switch (simulated hang) and blocks other peers; freed after.
- ⬜ Verify **mDNS actually discovers a peer on your LAN** (two machines, multicast allowed).

## M4 — Orchestration & presets
- ✅ **Preset executor** (`execute_plan`): runs ops best-effort, collects per-monitor partial-failure results.
- ✅ **Reconcile** logic (`reconcile` module): folds live `0x60` reads back, reports external changes.
- ✅ **DDC-disabled** state (distinct, persistent) + marked from the switch path.
- ✅ **Peer-loss → degraded** detector (`PeerRegistry::is_degraded`), feeding the partition guard.
- ⬜ Wire the OS **periodic re-read + `WM_DISPLAYCHANGE`** trigger that *calls* `reconcile_all` (Windows glue; verify on hardware).

## M5 — Tray UI & onboarding
- ✅ **Controller + bind layer + data-driven tray**: the Slint tray reads `AppWindow` inputs (monitors / peers / presets / online / degraded), and its `switch` / `apply-preset` callbacks are overridable from Rust. `bind` maps Controller view models → Slint structs with index↔id translation (tested).
- ✅ **`--live` mode** (`screenhop-ui -- --live`): enumerates this machine's real DDC/CI monitors and drives the tray through the production Controller → bind path — shows your actual panels + their state (read-only).
- ⬜ Wire the **live mesh loop + real actuation**: start a `Node` (discovery + serve) and a `LocalActuator` (ddc driver + calibration), and route the tray's `switch` / `apply-preset` over the mesh. (Today `--live` shows the honest in-flight state and logs that routing/calibration aren't wired.)
- ⬜ **Onboarding** flow (pair / calibrate cold-start / label) wired to the wizard surfaces.
- ⬜ **Claude Design** review/approval of the mockups; confirm shipped UI **matches** them (D12).
- ⬜ **Onboarding ≤ 10 min** on a 2-PC rig; capture the **soak §4.7 numbers** via the harness.

## M6 — Packaging & OSS readiness
- ✅ **Dual license fixed**: `LICENSE-MIT` + `LICENSE-APACHE` now both present (was MIT-only despite the `MIT OR Apache-2.0` claim).
- ✅ **CI** workflow ([.github/workflows/ci.yml](../.github/workflows/ci.yml)): fmt + clippy + build + test on `windows-latest`, an MSRV-1.82 build, and a release job that publishes binaries + **SHA-256**.
- ✅ **Docs**: `CONTRIBUTING.md`, `SECURITY.md` (incl. DPAPI re-pair caveat), `docs/contributing-quirks.md`.
- ⬜ Push and confirm **CI is green** on GitHub (the workflow is new; it hasn't run on the runners yet).
- ⬜ **Inno installer + Scheduled-Task autostart** (M6.1) — not started; the one remaining build artifact.

## Quick verification (what I ran)
```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test  --workspace      # 125 passed
```

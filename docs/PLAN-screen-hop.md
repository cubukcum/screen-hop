# screen-hop — Product Definition & Implementation Plan

> **Status:** Implementation started (M0 ✅, pivoting to Rust) · **Date:** 2026-06-26
> **Project type:** Cross-platform desktop tray application — **Rust + Slint**. Targets **Windows → Linux → macOS (best-effort)**.
> ⚠️ **Stack revised** from the original ".NET 8 / Windows-only" to **Rust + Slint, cross-platform** — see **§3 D13**. .NET/WPF/NSec/Inno references in later sections map to their Rust equivalents (table in §5.5 / D13); the *algorithms and architecture* (state machine, lease lock, EDID fingerprint, LAN protocol) are language-agnostic and unchanged.

---

## 1. Overview

**screen-hop** lets one person reassign their desk's physical monitors between several PCs over the LAN, from a tray menu, without touching a cable or the monitor's physical input-source button.

The setup it serves: multiple PCs (Windows, Linux, and best-effort macOS), several monitors, each monitor cabled to more than one PC at once (e.g. HDMI→PC A, DisplayPort→PC B). The user constantly wants to change which PC drives which monitor — sometimes all monitors on one PC, sometimes split. Today that means pressing each monitor's OSD input button repeatedly. screen-hop makes it a single click from whichever PC the user is at.

### How it works (one paragraph)
Modern monitors expose their input-source control over the video cable itself via **DDC/CI** — specifically VCP feature code **`0x60` ("Input Select")**. screen-hop runs a small **per-PC tray agent**; the agents form a **serverless peer mesh** over the LAN. When you click "give Monitor 2 to the laptop," the mesh routes the actual `0x60` write to **whichever PC can physically drive that panel's DDC link** (by default the *target* PC switches the monitor **to itself** — the reliable direction), then reconciles ownership against the monitor's live `0x60` value as ground truth.

### The differentiator
Plenty of tools switch *one* monitor's input from the PC it's plugged into (ControlMyMonitor, Monitorian, ddcswitch, display-switch), and plenty share *keyboard/mouse* across PCs (Synergy, Deskflow, Mouse Without Borders). **No packaged open-source project does first-class, network-coordinated DDC/CI input switching across multiple PCs.** That gap is the reason screen-hop exists.

### Honest boundaries (stated up front, repeated in Non-Goals)
- screen-hop is only as good as each monitor's DDC/CI implementation. Behavior is **per-monitor** and must be discovered on real hardware.
- It **cannot** touch BIOS/pre-OS/boot screens or anything before login (DDC/CI needs a running OS + a logged-in user session).
- It is **not instant** — a switch typically takes ~1–3s and occasionally needs a retry.
- If the PC that must drive a monitor is **off/asleep/locked-pre-logon**, that monitor is **stranded** — there is no software recovery; the physical OSD button is the guaranteed fallback. screen-hop says so plainly rather than pretending otherwise.

---

## 2. Project Type & Constraints (locked)

| Dimension | Decision |
|---|---|
| **Platforms** | **Windows → Linux → macOS (best-effort)**, in that priority order. macOS is experimental — Apple-Silicon DDC limits (see §3 D13 / §7). |
| **Language / runtime** | **Rust** — single static binary per OS |
| **UI** | **Slint** (all-native cross-platform GUI) + system tray (`tray-icon`; `ksni` on Linux) |
| **DDC access** | **`ddc-hi`** crate (backends `ddc-winapi` / `ddc-i2c` / `ddc-macos`) — one API across all three OSes |
| **EDID access** | `ddc-hi` `edid_data` (raw on Linux/macOS); on Windows, parsed identity fields + registry EDID (Win32 exposes no raw EDID via the monitor API) |
| **Topology** | Symmetric **peer** agents — **no always-on server/hub** |
| **Product intent** | **Open-source-grade**: works on strangers' unknown hardware; robust onboarding, quirk handling, clean installer, docs, sane defaults |
| **v1 control surface** | Tray menu on each PC (web/phone UI, hotkeys = roadmap) |
| **v1 functional scope** | Arbitrary per-monitor split + named presets, **video-only** |
| **v2** | Keyboard/mouse follow via Deskflow/Synergy; web/phone UI; hotkeys; scheduling; Wake-on-LAN |

---

## 3. Resolved Decisions (decision log)

These were genuine forks surfaced during design review; each is now **canonical** for the whole project. Do not relitigate without updating this table.

| # | Fork | **Decision** | Rationale |
|---|---|---|---|
| D1 | Serialization: per-monitor lock **vs** elected coordinator | **Per-monitor lease-based lock only. No elected coordinator.** Presets acquire **all involved per-monitor locks up front**, then execute best-effort. | Contention is ~nil (one human). A lease lock has no leader-election failure point. Matches the per-panel resource model. |
| D2 | Trust model: shared group secret **vs** per-peer pairwise keys | **v1 = single shared mesh secret → group key** (Argon2id) used for an AEAD channel + per-message auth, **plus per-install Ed25519 identity keys pinned (TOFU) for naming/revocation.** Pairwise-key compartmentalization is a **v1.x** hardening option. | Personal-LAN, single-operator, denial-of-visibility threat model. One passphrase keeps onboarding ≤10 min. Honest cost: a leaked secret compromises the mesh until rotated (documented). |
| D3 | Transport security primitive | **libsodium/NSec AEAD (XChaCha20-Poly1305) over raw TCP**, key = group key from D2. **TLS-PSK via `SslStream` is rejected** (managed .NET/SChannel does not expose external PSK). | Delivers "authenticate + encrypt every message" without SChannel limits and without a TOFU first-contact MITM window. A transport spike is an explicit **M3 exit gate**. |
| D4 | Calibration data replication scope | **Per-`(peer,monitor)` `0x60` write values are NEVER shared or used by another peer.** Only **panel-global learned facts** replicate: `workingDirection`, `readbackUnreliable`, `settleMs`/`sleepMultiplier`, `ddcOffByDefault`, `requiresActiveInput`, `blockedInputValues`. | A value confirmed on PC A is the selector for *A's* port; PC B writing it could pick the wrong input or risk a soft-brick. Enforced in the data model (see §7, §5). |
| D5 | Lease TTL vs switch duration | **Lock lease = 30 s, renewable; heartbeat = 3 s; peer-dead after ~10 s; per-monitor switch hard-ceiling = 15 s.** The lock holder **renews the lease before entering a known-slow push-release**. Invariant: `lease_TTL > switch_ceiling + margin`. | Prevents a lease expiring mid-switch (a DP push-release hang can eat ~10 s) and admitting a second actuator. |
| D6 | Autostart mechanism | **Per-user Scheduled Task "At log on"** is the canonical default (no admin needed, robust, supports delayed start/restart). HKCU `Run` key is the documented fallback. | One default the installer writes; referenced everywhere. |
| D7 | Quirks DB authority over writes | **Self-calibration is ALWAYS mandatory before any `0x60` write.** Shipped/community quirks `inputValues` are **display/seed hints only** and can **never** authorize a write. The DB *can* authoritatively contribute **safety/behavior** facts (blocked values to avoid, direction default, settle timing, ddc-off). | Keeps the soft-brick guard absolute and makes the community DB safe to accept PRs into. |
| D8 | ControlMyMonitor bundling | **Do NOT bundle** (NirSoft EULA restricts redistribution). Primary path = native `dxva2` high-level then low-level retry. ControlMyMonitor is an **optional user-supplied** fallback (path picker in settings). | No licensing blocker, no offline-install hole. |
| D9 | Wake-on-LAN in v1 | **v1 stranded UI is purely informational** ("owner unreachable — press the monitor's input button"). A best-effort "Wake owner" button ships in **v2** with WoL orchestration. | Avoids a half-working feature that can't be guaranteed. |
| D10 | PBP/PIP (picture-by-picture) | **v1 = warn-only via a quirk flag; no multi-source state tracking.** `OwnershipRecord` stays single-owner. | Keeps the data model simple; multi-source ownership is a later concern. |
| D11 | Session model | **Ship Model A only: per-user interactive tray app** (autostart at logon). The actuator is the **active interactive desktop session's** agent (Windows: active console session). A privileged-service + session-helper variant is designed-for via an `ActuationGate` seam but **not built in v1**. | Per-user desktop session is required to drive DDC on every OS; Session-0/headless services can't. |
| D12 | UI/UX design | **The tray UI, onboarding wizard, and all dialogs/states are designed in Claude Design FIRST**, from the written brief (**Appendix A**). The **Slint** implementation (M5) matches the returned designs — design is the source of truth, not the code. | This app has several non-obvious states (stranded, blind-warning, partial-failure, calibration cold-start) that deserve deliberate design before any UI code is committed. |
| **D13** | **Stack: cross-platform** (supersedes the .NET / Windows-only constraint) | **Rust + Slint**, single static binary per OS. DDC via **`ddc-hi`** (Win/Linux/macOS incl. Apple Silicon). Mesh via **tokio + rustls + RustCrypto (`chacha20poly1305`, `argon2`, `ed25519-dalek`) + `mdns-sd`**. Platform priority **Windows → Linux → macOS (best-effort/experimental)**. Remaps earlier decisions: **D3** NSec→**RustCrypto AEAD**; **D8** ControlMyMonitor **dropped** (ddc-hi is the cross-platform primary; no Windows-only shell-out); **D6** autostart becomes per-OS (Win Scheduled Task / Linux systemd-user or XDG autostart / mac launchd). | The hardest cross-platform layer is **macOS DDC**; `ddc-hi`/`ddc-macos` already solve it (proven by **display-switch**) whereas .NET would mean hand-maintaining undocumented Apple IOKit FFI forever. Slint keeps it all-native, smallest footprint. macOS limits (no DDC over Apple-Silicon built-in HDMI, no DisplayLink/most hubs, **write-only / no read-back**) → best-effort. |

---

## 4. Product Definition

### 4.1 Target user & primary scenarios
**Target user:** a power user / developer / trader / home-lab owner with **multiple PCs sharing one set of monitors** on a single desk (personal-grade LAN, single operator). Comfortable installing a tray app; not comfortable crawling under the desk for the OSD joystick ten times a day.

**Primary scenarios:**
- **Whole-desk handoff** — "I'm done on the work tower, give all four monitors to the gaming rig / Linux box / laptop." One action, every panel follows.
- **Arbitrary per-monitor split** — "Keep Monitors 1–2 on the work PC, throw 3–4 to the second PC," saved as a reusable named preset (*Work*, *Trading*, *Couch Mode*, *Pair Programming*).

### 4.2 Feature list (v1)
- Symmetric per-PC tray agent; **any peer can initiate** a switch. No central server.
- **Per-monitor reassignment** to any online peer, and **whole-desk handoff** in one click.
- **Named presets**: capture a full monitor→PC layout and re-apply it.
- **First-run pairing** (shared mesh secret) + **discovery** (mDNS with mandatory manual-host fallback).
- **Self-calibration**: each PC learns its own true `0x60` value per panel by reading it while active.
- **Monitor labeling** to disambiguate identical-model panels that collide on EDID.
- **Empirically-confirmed switching only** — never writes guessed/probed VCP values (soft-brick guard).
- **Pull-to-self by default, push-release fallback**, chosen **per monitor** from what actually works.
- **Retry + backoff + configurable inter-command delay**; **`0x60` read-back reconciliation** treats the live value as ground truth.
- **Loud "you'll be blind" warning** before a handoff that would leave the initiator with no visible screen.
- **First-class "stranded — owner unreachable" state** with the honest OSD-button instruction.
- **Partial-failure UX** for multi-monitor presets (best-effort batch — no false atomic/rollback promise).
- **Authenticated, encrypted LAN-only control** (Private profile, no WAN/UPnP).

### 4.3 Out of scope for v1 (deferred, not rejected)
Keyboard/mouse follow (v2) · web/phone dashboard (v2) · global hotkeys (v2) · scheduling (v2) · Wake-on-LAN (v2, per D9) · non-Windows agents · automatic window-layout save/restore (v1 *recommends* PersistentWindows; native integration later) · multi-source/PBP ownership tracking (warn-only per D10).

### 4.4 Non-goals (explicit)
- **Not a hardware-KVM replacement.** No USB/peripheral switching in v1; no guarantee of instant deterministic switching.
- **No BIOS / pre-OS / boot / lock-screen switching.**
- **No always-on central server/hub.**
- **No WAN / internet exposure, no UPnP.**
- **No promise of software recovery for a stranded monitor.**
- **No claim of atomic/transactional multi-monitor switching with rollback.**
- **No guessing of VCP input values** — only self-calibrated values are ever written.

### 4.5 Roadmap
| Version | Theme | Contents |
|---|---|---|
| **v1** | Coordinated video switching | Tray-driven per-monitor split + named presets; pairing/discovery/self-calibration/labeling; pull-to-self + push-release; retry/reconcile; blind warning; stranded + DDC-disabled UX; authenticated LAN-only transport. Video-only. |
| **v1.x** | Hardening & polish | Richer per-model quirks DB (community PRs); per-peer pairwise-key option (D2); optional native window-layout save/restore; better partial-failure recovery; diagnostics/logs; installer/auto-update refinement. |
| **v2** | Beyond video | K/M follow via Deskflow/Synergy; web/phone dashboard; global hotkeys; scheduling; best-effort Wake-on-LAN (D9). |

### 4.6 Key UX flows

**A. First-run: pairing + discovery**
1. Install on PC-A; it autostarts at logon (Scheduled Task) and appears in the tray.
2. Windows Defender Firewall first-run prompt → UI instructs **allow on Private networks**.
3. Tray → **Set up screen-hop** → generates/shows a **mesh secret** and lists this PC's name/IP.
4. Install on PC-B → **Join existing desk** → enter the secret. If mDNS discovery fails, **Add host manually** (IP/hostname) — offered up front.
5. Peers authenticate (D2/D3), exchange identities, replicate state. Tray shows "Connected: PC-A, PC-B."

**B. Monitor calibration & labeling onboarding** (resolves the cold-start chicken-and-egg)
1. Each PC enumerates panels and builds a **composite EDID fingerprint** per monitor (§7).
2. Wizard: "We found N monitors. Confirm them." For each panel **this PC is currently showing**, the agent **reads its own live `0x60`** → records *this PC's* value for that panel (self-calibration, per-`(peer,monitor)`, D4). Panels this PC has never driven are **"value unknown until first active."**
3. **Cold-start reality (stated honestly):** a PC can only learn a panel's pull-to-self value *while it is the active source*. To seed a `(PC, panel)` pair you intend to use, that PC must be the active input once. The guided flow follows what you're already looking at: calibrate PC-A's visible panels while A is shown; **press the monitor's OSD button (or use push-release where it works) to switch a panel to PC-B**, then calibrate on PC-B; repeat. Budget **≈ one OSD press per uncalibrated `(PC, panel)` cell** you care about. The ≤10-minute onboarding target assumes a 2-PC setup calibrating the panels currently in use; calibrating *every* cell of a large matrix takes longer and is disclosed.
4. **Labeling:** if two panels share an indistinguishable fingerprint, the wizard requires the user to **name/label** them (with a "flash this monitor" helper where the panel supports it) and binds by `monitorDevicePath`/position. Re-cabling or layout changes **flag the binding for re-confirmation** (not silently re-bound).

**C. The tray-menu switch** — Tray → **Monitors** → pick a monitor → pick a target PC. Progress shown ("Switching… ~2s"). On success the panel shows the target; ownership updates.

**D. Apply a named preset** — Tray → **Presets** → *Trading*. screen-hop acquires all involved per-monitor locks, then executes the batch best-effort, ordering writes to keep the operator visible as long as possible (§8.7). Partial failures are surfaced per-monitor.

**E. "You'll be blind" warning** — If a switch/preset would remove the **last panel currently owned by the initiating PC**, a modal warns: "This will leave **this PC** with no visible screen. Continue?" Whole-desk self-handoff requires explicit confirm.

**F. Stranded state** — If the PC that must actuate is unreachable, the monitor shows **"Stranded — owner PC unreachable. Press the monitor's input button to recover."** (v1: informational only, D9.)

**G. DDC-disabled error** — If a panel's DDC/CI is off or unresponsive, the UI says so and links to "enable DDC/CI in your monitor's OSD," with the optional ControlMyMonitor fallback (D8).

### 4.7 Success criteria (measurable, honestly scoped)
> Numbers below are evaluated by the measurement harness in §11.4 against a **declared panel population**; they are **conditional on DDC-cooperative, pull-to-self-capable panels** and are not coverage guarantees for arbitrary hardware.

| Criterion | Target | Scope/caveat |
|---|---|---|
| First-attempt single-switch success | ≥ 95% | On pull-to-self-capable panels that pass first-run calibration. **Push-release-only panels excluded** from this number. |
| Switch success within retry budget | ≥ 99% | Same scope as above. |
| Switch latency (median / p90) | ≤ 3 s / ≤ 6 s | On fast-settling panels; cranky panels (long `settleMs`) are slower and disclosed. |
| Onboarding: 2 PCs paired + in-use panels calibrated + first handoff | ≤ 10 min | Assumes monitors start on usable inputs; cold cells need ~1 OSD press each (§4.6-B). |
| Identical-model collision handling | Detected + user-labeled; **post-recable re-binding flagged for re-confirmation** | Not an absolute "never mis-targets." |
| Soft-brick | **Code-enforced invariant** (never write unconfirmed/blocked values) + post-release telemetry | Reframed from an unfalsifiable pre-ship gate to a design invariant + monitoring. |
| Security | Every control message authenticated on the **agent** side; LAN/Private-only; no WAN/UPnP | TOFU is avoided via D3; same-user paired-malicious-peer is out of threat model. |

---

## 5. System Architecture & Tech Stack

### 5.1 High-level
Every PC runs one **symmetric peer agent**. Each agent contains the same modules:

```
┌──────────────────────────── screen-hop agent (per PC) ────────────────────────────┐
│  Tray UI (WPF + tray icon)                                                          │
│        │                                                                            │
│  App/Orchestration  ──►  Locking (per-monitor lease)  ──►  Actuation (DDC/CI)       │
│        │                         │                                 │                │
│  Replicated Store  ◄──►  LAN Node (discovery, AEAD transport, gossip, RPC)          │
│        │                         ▲                                                  │
│  Identity & Calibration  ────────┘   Quirks DB (shipped + community + local)        │
│        │                                                                            │
│  Hosting (autostart, single-instance, session gate, logging)                       │
└────────────────────────────────────────────────────────────────────────────────────┘
        ▲ Win32/WinRT: dxva2, user32 (EnumDisplayMonitors/QueryDisplayConfig), WinRT DisplayMonitor, WMI
        ▼ TCP (libsodium AEAD) to peer agents on the LAN
```

### 5.2 Process & session model (D11)
- **Per-user interactive tray app**, autostarted via a **per-user Scheduled Task "At log on"** (D6). A Session-0 service **cannot** enumerate/drive the logged-in user's panels, so the agent must live in the interactive session.
- **Actuator = the active console session's agent** (`WTSGetActiveConsoleSessionId`). On **fast-user-switch / RDP disconnect**, a disconnected-but-alive session's agent marks itself **"cannot actuate"** and relinquishes any lock authority; the active console session's agent takes over. No actuation at the lock/secure desktop or pre-logon.
- **Single instance per user** (named mutex). The `ISessionActuationGate` seam keeps the Model-B (service+helper) upgrade open without building it.

### 5.3 Module breakdown
| Module | Responsibility | Key APIs / libs |
|---|---|---|
| **Tray UI** | Tray menu, onboarding wizard, warnings, status | WPF, `H.NotifyIcon` |
| **App/Orchestration** | Turn user intent into ordered switch operations; preset execution; blind-point logic | — |
| **Actuation** | `0x60` read/write, direction selection, retry/backoff, read-back reconciliation, soft-brick guards | `dxva2.dll` P/Invoke (`GetPhysicalMonitorsFromHMONITOR`, `GetVCPFeatureAndVCPFeatureReply`, `SetVCPFeature`, `CapabilitiesRequestAndCapabilitiesReply`) |
| **Identity & Calibration** | Enumerate panels, composite EDID fingerprint, self-calibration, collision/labeling | `EnumDisplayMonitors`, `GetMonitorInfo`, `QueryDisplayConfig` + `DisplayConfigGetDeviceInfo`, WinRT `DisplayMonitor`, WMI `WmiMonitorRawEEdidV1Block` |
| **Locking** | Per-monitor lease lock; acquire/renew/release; preset multi-lock acquisition | over LAN RPC |
| **LAN Node** | Discovery, AEAD transport, gossip, request/response | `System.Net.Sockets`, `NSec.Cryptography` (libsodium), `Makaretu.Dns`/`Zeroconf` for mDNS |
| **Replicated Store** | LWW-merged state (inventory, ownership cache, presets, replicated quirk facts); reconciliation | — |
| **Quirks DB** | Shipped + community + local-learned panel-global facts (D7) | JSON resource + local override |
| **Hosting** | Autostart registration, single-instance, session gate, config/log paths | `Microsoft.Win32.TaskScheduler`, `Serilog` |

### 5.4 Workspace structure (Cargo)
```
screen-hop/
├─ Cargo.toml                  # workspace manifest
├─ crates/
│  ├─ screenhop-core/          # domain types, MonitorDriver trait, actuation state machine — no OS deps
│  ├─ screenhop-ddc/           # ddc-hi-backed MonitorDriver (Win/Linux/macOS)
│  ├─ screenhop-identity/      # enumeration, composite EDID fingerprint, calibration
│  ├─ screenhop-net/           # discovery (mdns-sd), AEAD transport, gossip, locking RPC
│  ├─ screenhop-state/         # replicated store, reconciliation
│  ├─ screenhop-quirks/        # quirks DB + merge precedence
│  ├─ screenhop-app/           # orchestration, presets, blind logic, hosting
│  ├─ screenhop-ui/            # Slint tray UI + onboarding wizard
│  └─ screenhop-spike/         # M0 hardware spike (ddc-hi enumerate/read/write + pull-to-self test)
├─ quirks/quirks.json          # shipped community DB
├─ packaging/                  # Win (MSI/Inno) · Linux (AppImage/deb/Flatpak + udev rule) · mac (.app)
├─ docs/                       # this plan, README, CONTRIBUTING, quirks-contrib guide
└─ .github/workflows/          # build/test/package matrix (windows · linux · macos)
```
Unit tests live in each crate (`#[cfg(test)]` + `tests/`); the real-hardware matrix is a manual/opt-in harness in `screenhop-spike`/`screenhop-ddc`.

### 5.5 Recommended crates (with rationale)
| Need | Choice | Why |
|---|---|---|
| DDC/CI | **`ddc-hi`** (+ `ddc-winapi` / `ddc-i2c` / `ddc-macos`) | One API across Win/Linux/macOS incl. Apple Silicon; proven by display-switch; we still own the actuation state machine + soft-brick guards |
| UI | **Slint** | All-native cross-platform GUI; no web toolchain; small footprint; declarative `.slint` markup |
| Tray | **`tray-icon`** (+ **`ksni`** on Linux) | Cross-platform tray; ksni gives clean SNI on modern GNOME/KDE |
| Async / net | **`tokio`** | De-facto async runtime; TCP + timers for the mesh |
| Crypto | **RustCrypto: `chacha20poly1305` + `argon2` + `ed25519-dalek`** | AEAD + KDF + identity keys (avoid the deprecated `sodiumoxide`) |
| mDNS | **`mdns-sd`** | Pure-Rust DNS-SD; manual host entry is the fallback regardless |
| Serialization | **`serde` + `serde_json`** | State, quirks DB, wire messages (JSON for debuggability) |
| Logging | **`tracing` + `tracing-subscriber`** | Structured diagnostics for unknown field hardware |
| Tests | built-in `#[test]` + **`mockall`** / hand fakes | Mockable `MonitorDriver` trait |
| Packaging | **`cargo-dist`** + per-OS bundlers | Reproducible cross-platform release artifacts |

### 5.6 Core data model
| Entity | Key fields | Notes |
|---|---|---|
| **PhysicalMonitor** | `MonitorId` (fingerprint-derived), `Label`, `Fingerprint`, `LastSeenUtc`, `HmonitorDevicePath`, `PbpCapable` | Per-PC view; `MonitorId` is the cross-PC join key |
| **MonitorFingerprint** | `PnpMfrId`, `ProductCode`, `NumericSerial`, `AsciiSerial`, `Week`, `Year`, `RawEdidSha256` | Composite; see §7 |
| **Peer** | `PeerId` (Ed25519 pubkey), `Name`, `Endpoints[]`, `LastHeartbeatUtc`, `Online`, `CanActuate` | `CanActuate=false` when not the active console session |
| **OwnershipRecord** | `MonitorId`, `OwnerPeerId`, `ObservedFrom`, `UpdatedUtc`, `State` (Owned/Unknown/Stranded) | **Cache** of the panel's live `0x60`; `Lock*` fields are **advisory only** — authority is the lease RPC (§8.5) |
| **CalibrationRecord** | **key `(PeerId, MonitorId)`**, `InputValue` (the `0x60` to write for *this* peer's pull-to-self), `Confirmed`, `ConfirmedUtc` | **Never cross-used by another peer** (D4) |
| **Quirk** | key `MonitorModel`/fingerprint, `WorkingDirection`, `ReadbackUnreliable`, `SettleMs`, `SleepMultiplier`, `DdcOffByDefault`, `RequiresActiveInput`, `BlockedInputValues[]`, `PbpCapable`, `Source` | **Panel-global**, replicated; never authorizes a write (D7) |
| **Preset** | `Name`, `Assignments[] {MonitorId → PeerId}` | Applied as a best-effort locked batch |

### 5.7 UI/UX design workflow (Claude Design) — D12
All user-facing surfaces are **designed in Claude Design before any WPF code is written**:
1. The maintainer hands the **Claude Design brief (Appendix A)** to Claude Design.
2. Claude Design returns mockups for every surface in the brief (tray menu, onboarding wizard, dialogs/states, desk-map, settings).
3. Only then does `ScreenHop.Tray` implement against the approved designs (milestone M5).

The Claude Design output is the **source of truth**; the WPF build matches it. The `ScreenHop.Tray` ↔ `ScreenHop.App` boundary (commands/queries/state events) is defined so UI work can proceed against designs without blocking on core internals.

---

## 6. DDC/CI Actuation Subsystem

### 6.1 State machine (single switch)
```
Idle
 └─► ValidateCapability ── DDC off / no 0x60 ──► Fail(DdcUnavailable)
       └─► ChooseDirection (pull-to-self default; push-release only if quirk says so & pull won't work)
             └─► ResolveValue ── value unknown for this (peer,monitor) ──► Fail(NeedsCalibration)
                   └─► GuardValue ── value ∈ BlockedInputValues or unconfirmed ──► Fail(BlockedValue)   ◄─ soft-brick guard
                         └─► Write0x60 ──► Settle(settleMs) ──► VerifyReadback
                               ├─ readback == target ──► Commit(success)
                               ├─ readback inconclusive (read fails) ──► Commit(assumed-success, flagged)   ◄─ NOT a failure
                               └─ readback != target ──► Retry (backoff) ──► … ──► Fail(after budget/ceiling)
```

### 6.2 Direction selection (D4, per-monitor)
- **Default = pull-to-self**: the *target* PC writes **its own** calibrated `0x60` value over its own cable. This is the reliable real-world direction.
- **Fallback = push-release**: the *current owner* writes the target's value. Used only where pull-to-self is known not to work for that panel. push-release on DisplayPort can hang ~10 s and fails ~50% of the time — it is the last resort, and panels that are push-release-only are excluded from the headline reliability numbers (§4.7).
- `WorkingDirection` is a **panel-global** learned fact (replicated). The **write value** is strictly per-`(peer,monitor)` (never replicated).

### 6.3 Retry / timing
- `~40–50 ms` per DDC call assumed; **configurable inter-command delay** (`SleepMultiplier`, default 1.0, range 0.1–3.0, à la ddcutil).
- Default `SettleMs = 1500` (range 750–4000; `NeedsLongerSettle` quirk bumps it).
- Retry with exponential backoff; **per-monitor hard ceiling = 15 s** (D5). Lock lease (30 s) always exceeds it.
- **Read-back failure is INCONCLUSIVE, not failure** — committing "assumed success, flagged" avoids re-issuing writes that cause flapping.

### 6.4 Soft-brick guards (absolute, D7)
1. **Only ever write a value that is `Confirmed` for this `(peer,monitor)` via self-calibration.** No exceptions.
2. **Never probe arbitrary values.** Capability strings are advisory display-only, never authoritative for writes.
3. **Honor `BlockedInputValues`** from the quirks DB (safety-only; can only *restrict*).
4. Validate the value is in the panel's confirmed set before every write.

### 6.5 DDC-disabled & fallback
- Detect "DDC/CI off / unresponsive" (no `0x60`, repeated NAK) → surface the OSD-enable guidance (§4.6-G).
- Optional **user-supplied** ControlMyMonitor fallback (path picker), checked only after native `dxva2` high-level then low-level retry both fail (D8). Not bundled.

### 6.6 Quirks DB
- JSON: **shipped** (`quirks/quirks.json`) ◄ merged with ► **local-learned cache** ◄ merged with ► **user override**. Precedence: user override > local-learned > shipped.
- Examples to seed: Samsung U32H750 (`0x05/0x06` actual vs `0x11/0x12` advertised), Dell USB-C (`0x1b`), DDC-off-by-default models, post-set Save-Settings on some Iiyama, PBP-capable flags.
- **Community contribution flow** (open-source): export a quirk snippet → PR into `quirks/quirks.json`. PRs add only **panel-global behavior** facts; they can never inject a write-authorizing value (D7).

### 6.7 PBP/PIP (D10)
v1: if a panel is `PbpCapable`, **warn before actuating** ("this monitor may be in split-source mode") — no multi-source state tracking. `OwnershipRecord` stays single-owner.

---

## 7. Monitor Identity, Enumeration & Calibration

### 7.1 Enumeration → raw EDID chain
1. `EnumDisplayMonitors` → `HMONITOR`s; `GetMonitorInfo` → `szDevice` + rect.
2. `QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS)` + `DisplayConfigGetDeviceInfo(GET_TARGET_NAME)` → `DISPLAYCONFIG_TARGET_DEVICE_NAME` (EDID mfr/product IDs + `monitorDevicePath`).
3. Join `DISPLAYCONFIG_SOURCE_DEVICE_NAME.viewGdiDeviceName` ↔ `HMONITOR.szDevice`.
4. Raw EDID via WinRT `DisplayMonitor.FromInterfaceIdAsync(monitorDevicePath).GetDescriptor(Edid)` (fallback WMI `WmiMonitorRawEEdidV1Block`).
5. `GetNumberOfPhysicalMonitorsFromHMONITOR` + `GetPhysicalMonitorsFromHMONITOR` → DDC handle (one `HMONITOR` may map to several physical monitors on clone — de-dupe by fingerprint).

**Join-stability edge case:** `szDevice` (`\\.\DISPLAY1`) is reassigned on topology changes. On `WM_DISPLAYCHANGE`, **debounce ~500 ms, re-enumerate, retry the join** up to N times; if still ambiguous, mark the monitor **"needs re-confirm"** rather than guessing.

### 7.2 Composite EDID fingerprint
`Fingerprint = PnpMfrId + ProductCode + NumericSerial(32-bit) + AsciiSerial(0xFF descriptor) + Week + Year`, plus an advisory `RawEdidSha256`. **No single field suffices** — serials are frequently blank/zero/model-constant. Surfaced `MonitorId` = first 12 hex of the composite hash.

### 7.3 Collision handling (identical-model panels)
- Detect identical fingerprints → **require user labeling** (with a "flash/identify this monitor" helper where supported) and bind by `monitorDevicePath` + position.
- **Re-cabling / layout change flags the binding for re-confirmation** — never silently re-binds (success-criteria caveat, §4.7).
- Docks / MST / USB-C / KVM may present **virtualized/emulated EDID** (helps stability, hurts uniqueness, may block DDC passthrough) → detect inconsistency and fall back to labeling; warn the user when a virtualized-EDID path is suspected.

### 7.4 Self-calibration (D4)
- A PC reads its own `0x60` **only while it is the active source** on a panel → records `CalibrationRecord(PeerId, MonitorId).InputValue = Confirmed`.
- A PC that has never been active on a panel = **"value unknown until first active"** and **cannot pull-to-self** there until seeded (see cold-start, §4.6-B).
- Another peer's published **label** for a colliding fingerprint may be reused as a name, but the local binding is **confirmed locally** before trust.

---

## 8. LAN Protocol, Discovery & State-Sync

### 8.1 Discovery
- **mDNS/DNS-SD** (`_screenhop._tcp`) advertising name + endpoints + a mesh-secret hash for grouping.
- **Mandatory manual host entry** (IP/hostname[:port]) offered up front — mDNS is flaky/blocked on Windows & enterprise.

### 8.2 Pairing & trust (D2)
- One **mesh secret** (passphrase). `GMK = Argon2id(secret, salt)`. Derives the AEAD key + per-message auth key.
- Each install has an **Ed25519 identity keypair**; public keys are **pinned (TOFU)** at first contact for naming/revocation. Revoke a peer = drop its pinned key + rotate the mesh secret.
- Honest cost (documented): a leaked mesh secret compromises the mesh until rotated. Per-peer pairwise keys are a **v1.x** option.

### 8.3 Transport & message security (D3)
- **TCP + libsodium AEAD (XChaCha20-Poly1305)**, key from §8.2. Every message is encrypted + authenticated **on the agent side**.
- Replay protection: per-message nonce + monotonic seq + a recent-msg-id window (tune depth vs clock skew).
- Bind to **LAN/Private profile only**; never WAN/UPnP.

### 8.4 Message schema
| Message | Dir | Key fields | Purpose |
|---|---|---|---|
| `Announce`/`Heartbeat` | bcast | peerId, name, endpoints, canActuate, stateVersion, **3 s** interval | liveness, presence |
| `InventoryGossip` | sync | monitors[] (fingerprint, label, pbpCapable) | replicate inventory |
| `QuirkGossip` | sync | panel-global quirk facts (D4/D7) | share learned behavior |
| `OwnershipGossip` | sync | monitorId, ownerPeerId, observedFrom, updatedUtc, state | cache of live `0x60` |
| `LockRequest` | →holder | monitorId, requesterPeerId, leaseSecs | request actuation right |
| `LockGrant`/`LockDeny` | ←holder | monitorId, granted, leaseExpiresUtc | grant/deny lease |
| `SwitchCommand` | →actuator | monitorId, targetPeerId, direction | perform the `0x60` write |
| `SwitchResult` | ←actuator | monitorId, outcome, observedValue, flags | success/inconclusive/fail |
| `PresetApply` | initiator-local | assignments[] | drives a locked batch |

### 8.5 Serialization (D1, D5)
- **Per-monitor lease lock**, no elected coordinator. Lock **authority is the lease RPC** (held by the believed owner/actuator); the store's `Lock*` fields are **advisory cache only** (LWW-merging a lock is unsafe).
- **Lease = 30 s, renewable; heartbeat = 3 s.** The holder **renews before a known-slow push-release** so a lease can't expire mid-switch (`lease_TTL > 15 s ceiling + margin`).
- **Preset** = acquire **all** involved per-monitor locks up front; if any can't be acquired, report and proceed only with the acquirable subset (no false atomicity).

### 8.6 Reconciliation (ground truth = live `0x60`)
- The ownership map is a **cache**. Periodic `0x60` re-read + `WM_DISPLAYCHANGE`/display add-remove events correct it.
- If **no online peer can read a panel** (the stranded case), ownership is **"Stranded/Unknown-until-owner-returns"** — a distinct, persistent state, not transient unknown.
- **Partition guard:** on peer-loss/partition detection, **pause disruptive ops** ("degraded — peers unreachable"). Post-heal, the panel's **live `0x60` deterministically wins** and the map is corrected to match hardware.

### 8.7 Preset ordering & blind-point
- "Operator's panels" = panels currently owned by the initiating PC.
- Order writes so the operator's own visible panels are handed away **last**.
- **Whole-desk self-handoff** (every visible panel leaves the operator) → fire the **blind warning for the whole batch**; on confirm, proceed; after the operator's last panel switches, the operator is intentionally blind and the target shows everything.

---

## 9. Security & Trust (summary)
- **Threat model:** denial-of-visibility / screen-reroute abuse on a personal LAN by an unpaired host; **out of scope:** a malicious already-paired peer run by the same user, and physical attackers.
- Pairing (D2) + **per-message auth on the agent side** (D3); **LAN/Private-only**, no WAN/UPnP.
- **Key storage:** group key + identity private key via **DPAPI (CurrentUser)** in `%LOCALAPPDATA%`. **Documented consequence:** a backed-up/copied config does **not** carry DPAPI-bound keys → **re-pair required after profile migration / re-image** (the "back up your config" docs must say this).
- **Disruptive-op rate-limit:** token-bucket min-switch-interval to prevent flap/abuse (defaults tuned not to annoy a single operator).
- Windows Defender Firewall first-run prompt handled in onboarding (allow Private).

---

## 10. Deployment / Packaging
- **Autostart:** per-user **Scheduled Task "At log on"** (D6); installer writes it; uninstall removes it.
- **Installer:** **Inno Setup** (recommended; per-user, no-admin, low friction). WiX/MSI is a later option for enterprise silent-deploy.
- **Code signing:** start with **published SHA-256 hashes** for early releases; move to **Azure Trusted Signing** or an OV/EV Authenticode cert when identity/cost allow. (Maintainer decision — see §15.)
- **Self-update:** **notify-only** in v1 (link to the GitHub release); winget manifest published. Auto-apply is later. Update path must **never re-enable autostart against the user's setting** and must leave the single autostart artifact (Scheduled Task) intact.
- **Paths:** config/keys/logs under `%LOCALAPPDATA%\screen-hop\`. Uninstall removes autostart + (optionally) config; offers "keep my calibration."
- **ControlMyMonitor:** not bundled (D8); optional user-supplied path.

---

## 11. Testing Strategy

### 11.1 Unit (mockable, no hardware)
- Actuation **state machine** against a mock `IMonitorDriver` (success / inconclusive-readback / NAK / hang / blocked-value).
- **Fingerprint & collision** logic (blank/constant serials, identical models).
- **Locking & serialization** (lease grant/deny, renewal, **lease-cannot-expire-mid-switch under simulated DP hang**, preset multi-lock up-front acquisition). *No coordinator-election tests* (D1).
- **Protocol** (AEAD round-trip, replay rejection, message (de)serialization).
- **Calibration replication split** (assert per-`(peer,monitor)` values never leave the peer; only panel-global facts gossip — D4).

### 11.2 Integration (mock transport, two in-proc peers)
- Pairing → discovery (manual host) → inventory/ownership gossip → a switch routed to the correct actuator → reconciliation after a simulated external `0x60` change.

### 11.3 Real-hardware matrix (manual / opt-in `ScreenHop.HardwareMatrix`)
- Per panel model, record: DDC/CI on?; `0x60` readable?; **pull-to-self works?**; push-release works?; settle time; read-back reliable?; soft-brick-safe value set. Feeds the quirks DB. **This is where per-monitor truth is established** — contributors run it on their hardware and submit results.

### 11.4 Measurement harness (makes §4.7 numbers testable)
- A **soak runner** over a **declared panel population** (the maintainer's matrix + CI-reported community panels), fixed **sample size** (e.g. 200 switches/panel), recording first-attempt%, within-retry%, latency median/p90 per panel and per direction. Reliability numbers are reported **scoped to pull-to-self-capable panels**.

### 11.5 CI (GitHub Actions)
- Build + unit + integration on `windows-latest`; package the installer; attach SHA-256 hashes; (later) sign. Hardware-matrix and soak are **manual/opt-in** (no DDC hardware in CI).

---

## 12. Risk Register
| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Per-monitor DDC unreliability (flaky/slow/no-op) | High | Med | Retry/backoff, settle tuning, read-back-inconclusive handling, quirks DB, real-hardware matrix |
| **Stranded monitor** (owner off/asleep) | Med | High | First-class stranded state; OSD-button is the honest fallback; WoL is v2 (D9) |
| **Soft-brick** from bad `0x60` write | Low | **Severe** | Absolute guard: only self-confirmed values, never probe, honor `BlockedInputValues` (D7) |
| EDID collisions / virtualized EDID (docks/MST/KVM) | Med | Med | Composite fingerprint + mandatory labeling + re-confirm-on-recable (§7.3) |
| push-release-only panels | Med | Med | Excluded from reliability headline; pull-to-self default; disclosed in UI |
| mDNS blocked/flaky | Med | Low | Manual host entry as a first-class path |
| Session-0 / lock-screen can't actuate | Certain | Med | Documented; actuate only in active console session (D11) |
| Lease expiring mid-switch → double-actuation | Low | High | `lease_TTL(30s) > ceiling(15s)+margin`; renew before slow push-release (D5) |
| Network partition double-write/flap | Low | Med | Pause-on-partition guard; live-`0x60`-wins reconcile (§8.6) |
| Leaked mesh secret | Low | Med | Rotate secret + re-pair; per-peer keys in v1.x (D2) |
| ControlMyMonitor licensing | Resolved | — | Not bundled; optional user-supplied (D8) |
| Transport primitive unavailable (TLS-PSK) | Resolved | — | libsodium AEAD over TCP; spike gates M3 (D3) |

---

## 13. Milestone Roadmap
| ID | Milestone | Deliverable | Exit criteria |
|---|---|---|---|
| **M0** | **Hardware feasibility spike** | Throwaway tool: enumerate panels, read/write `0x60`, test **pull-to-self** on ≥2 real PCs/monitors | **Pull-to-self confirmed on the target hardware** (or a documented per-monitor verdict). Go/no-go before committing the architecture. |
| **M1** | Local DDC core | `ScreenHop.Ddc` + state machine + soft-brick guards, against mock + one real panel | Unit tests green; a single local input switch works on real hardware; blocked-value never written |
| **M2** | Identity & calibration | `ScreenHop.Identity` + fingerprint + self-calibration + labeling + quirks load; **measurement harness skeleton** | Same panel correlated across 2 PCs; collisions force labeling; per-`(peer,monitor)` values stored; harness records a panel's stats |
| **M3** | LAN mesh | Discovery (mDNS + manual), **transport spike (exit gate)**, AEAD transport, gossip, **per-monitor lease lock** | 2 peers pair, replicate state, route a switch to the correct actuator; replay rejected; **lease-mid-switch test passes** |
| **M4** | Orchestration & presets | App layer: per-monitor switch, named presets (up-front multi-lock), blind warning, stranded + DDC-disabled states, reconciliation | Arbitrary split + a saved preset apply best-effort with partial-failure UX; reconcile after external OSD change |
| **M5** | Tray UI & onboarding | **Claude Design mockups (Appendix A) → ** WPF tray + onboarding wizard (pairing, calibration cold-start flow, labeling); diagnostics/logs | Designs approved in Claude Design first; implemented UI **matches the approved mockups**; a new user pairs 2 PCs + calibrates in-use panels + does a first handoff via tray only; success-criteria soak run produces numbers |
| **M6** | Packaging & OSS readiness | Inno installer + Scheduled-Task autostart, published hashes, README/CONTRIBUTING/quirks-contrib guide, CI | Clean install/uninstall; autostart works; CI builds+tests+packages; docs let a stranger install and a contributor submit a quirk |
| **v1** | **Ship** | Tagged open-source v1 | All above; risk register reviewed; honesty caveats in UI + docs |
| **v2** | Beyond video | K/M (Deskflow), web/phone UI, hotkeys, scheduling, WoL | (separate plan) |

---

## 14. Task Breakdown (INPUT → OUTPUT → VERIFY)

> Agent/skill mappings are adapted from the kit (web-oriented) to this .NET desktop app. `clean-code` applies to **every** task.

### M0 — Hardware feasibility spike
| Task | Agent / Skill | INPUT → OUTPUT → VERIFY |
|---|---|---|
| M0.1 Spike: enumerate + read/write `0x60` | `debugger` / clean-code | dxva2 P/Invoke signatures → console app that lists panels and switches one input → **input visibly changes on a real monitor** |
| M0.2 Spike: pull-to-self across 2 PCs | `debugger` / clean-code | 2 PCs cabled to 1 monitor → PC-B switches the monitor **to itself** while PC-A shows it → **succeeds, or a per-monitor verdict recorded**; go/no-go documented |

### M1 — Local DDC core
| Task | Agent / Skill | INPUT → OUTPUT → VERIFY |
|---|---|---|
| M1.1 `IMonitorDriver` + dxva2 wrapper | `backend-specialist` / clean-code | API spec → driver with return-code/`GetLastError` checks + low-level fallback → unit tests for each return path |
| M1.2 Actuation state machine | `backend-specialist` / clean-code | §6.1 → state machine → tests: success / inconclusive / NAK / hang / blocked-value |
| M1.3 Soft-brick guards | `security-auditor` / clean-code | D7 rules → guard layer → test: never writes an unconfirmed/blocked value (property test) |

### M2 — Identity & calibration
| Task | Agent / Skill | INPUT → OUTPUT → VERIFY |
|---|---|---|
| M2.1 Enumeration → raw EDID chain | `backend-specialist` / clean-code | §7.1 → identity module → returns fingerprint for each panel; `WM_DISPLAYCHANGE` debounce/retry covered |
| M2.2 Composite fingerprint + collision | `backend-specialist` / clean-code | §7.2/7.3 → fingerprint+collision detector → tests with blank/constant serials and identical models |
| M2.3 Self-calibration + per-`(peer,monitor)` store | `backend-specialist` / clean-code | D4 → calibration store → test: values never cross peers; "unknown until first active" enforced |
| M2.4 Quirks DB load + merge precedence | `backend-specialist` / clean-code | §6.6 → quirks loader → precedence test (user>local>shipped); DB value can't authorize a write |
| M2.5 Measurement-harness skeleton | `test-engineer` / clean-code | §11.4 → soak runner stub → records stats for one panel |

### M3 — LAN mesh
| Task | Agent / Skill | INPUT → OUTPUT → VERIFY |
|---|---|---|
| M3.1 **Transport spike (exit gate)** | `security-auditor` / clean-code | D3 → libsodium AEAD-over-TCP PoC → 2 processes exchange authenticated/encrypted msgs; TLS-PSK formally rejected |
| M3.2 Discovery (mDNS + manual host) | `backend-specialist` / clean-code | §8.1 → discovery module → peers find each other; manual host works when mDNS off |
| M3.3 Pairing + identity pinning | `security-auditor` / clean-code | D2 → pairing flow → join with secret; pinned key; replay rejected |
| M3.4 Gossip + replicated store | `backend-specialist` / clean-code | §8.4/8.6 → LWW store → inventory/ownership/quirk converge across 2 peers |
| M3.5 Per-monitor lease lock | `backend-specialist` / clean-code | D1/D5 → locking RPC → tests: grant/deny/renew; **lease can't expire mid-switch under simulated hang**; no coordinator |

### M4 — Orchestration & presets
| Task | Agent / Skill | INPUT → OUTPUT → VERIFY |
|---|---|---|
| M4.1 Switch orchestration + routing | `backend-specialist` / clean-code | intent → routes to correct actuator (pull-to-self default) → in-proc 2-peer test switches the right panel |
| M4.2 Presets (up-front multi-lock, best-effort) | `backend-specialist` / clean-code | §8.5/8.7 → preset engine → partial-failure surfaced; ordering keeps operator visible longest |
| M4.3 Blind-point + warning logic | `backend-specialist` / clean-code | §8.7 → blind detector → warns iff last operator panel leaves |
| M4.4 Reconciliation + stranded/partition states | `backend-specialist` / clean-code | §8.6 → reconciler → corrects to live `0x60`; stranded persists; partition pauses disruptive ops |

### M5 — Tray UI & onboarding
| Task | Agent / Skill | INPUT → OUTPUT → VERIFY |
|---|---|---|
| **M5.0 Claude Design brief → designs** (D12) | maintainer + `frontend-specialist` / frontend-design | **Appendix A brief** → Claude Design mockups for tray menu, onboarding wizard, dialogs/states (blind/stranded/DDC-disabled/progress/partial-failure), desk-map, settings → **designs reviewed & approved before any WPF code** |
| M5.1 Tray menu (monitors, targets, presets, status) | `frontend-specialist` / clean-code (+ frontend-design) | **approved M5.0 mockups** + flows §4.6 → WPF tray → click-through switches a monitor; status reflects ownership; **matches mockups** |
| M5.2 Onboarding wizard (pair, calibrate cold-start, label) | `frontend-specialist` / clean-code | §4.6-A/B → wizard → 2-PC pair + in-use calibration ≤10 min on test rig |
| M5.3 Warning/stranded/DDC-disabled UX | `frontend-specialist` / clean-code | flows E/F/G → dialogs/states → each state reachable and accurate |
| M5.4 Diagnostics/logging + soak run | `test-engineer` / clean-code | Serilog + harness → produces §4.7 numbers scoped to pull-to-self panels |

### M6 — Packaging & OSS readiness
| Task | Agent / Skill | INPUT → OUTPUT → VERIFY |
|---|---|---|
| M6.1 Inno installer + Scheduled-Task autostart | `devops-engineer` / clean-code | D6 → installer → clean install/uninstall; autostart at logon; no leftovers |
| M6.2 CI (build/test/package/hash) | `devops-engineer` / clean-code | §11.5 → GitHub Actions → green on `windows-latest`; artifact + hashes |
| M6.3 Docs (README, CONTRIBUTING, quirks-contrib, security note) | `backend-specialist` / clean-code | this plan → docs → a stranger can install; a contributor can submit a quirk PR; DPAPI re-pair caveat documented |

---

## 15. Open Decisions Deferred to the Maintainer
These are genuine choices that don't block M0–M2 and can be made before the milestone that needs them:
1. **Code-signing path** (published hashes now → Azure Trusted Signing vs OV/EV cert later). *Needed by:* M6.
2. **Supported ceiling** to officially test (e.g. up to 4 PCs × 6 monitors). *Needed by:* M5 soak.
3. **Wire format** JSON (debuggable, recommended for OSS) vs MessagePack. *Needed by:* M3.4.
4. **Community quirks hosting** — PR-into-repo (recommended) vs a submission endpoint. *Needed by:* M6.
5. **Window-reflow strategy** — recommend PersistentWindows (v1) vs native save/restore (v1.x). *Needed by:* v1.x.
6. **Final shipping name** — confirm `screen-hop` (affects namespaces, installer, repo).

---

## Phase X — Verification Checklist (Definition of Done for v1)
> Mark `[x]` only after actually running the check.

- [ ] M0 go/no-go recorded (pull-to-self verdict on real hardware)
- [ ] Unit suite green (state machine, fingerprint/collision, locking incl. lease-mid-switch, protocol/replay, calibration-split)
- [ ] Integration: 2-peer pair → switch routed correctly → reconcile after external change
- [ ] Real-hardware matrix filled for the maintainer's panels; quirks seeded
- [ ] Soak harness produces §4.7 numbers (scoped to pull-to-self panels)
- [ ] Soft-brick guard property test passes (never writes unconfirmed/blocked value)
- [ ] Security: every message authenticated agent-side; LAN/Private-only; no WAN/UPnP; rate-limit active
- [ ] Onboarding ≤10 min on a 2-PC test rig (in-use panels), cold-start documented
- [ ] Claude Design mockups (Appendix A) produced and approved before UI coding; shipped UI matches them
- [ ] Blind warning, stranded state, DDC-disabled state all reachable and accurate
- [ ] Installer: clean install/uninstall; Scheduled-Task autostart; DPAPI re-pair caveat in docs
- [ ] CI green on `windows-latest`; artifact + SHA-256 published
- [ ] README / CONTRIBUTING / quirks-contrib / security note complete
- [ ] All §3 resolved decisions reflected in code; no coordinator-election code; no bundled ControlMyMonitor

---

## Appendix A — Claude Design brief (hand this to Claude Design)

> Paste the block below into Claude Design before UI coding begins. It is intentionally self-contained.

```
Design the UI for "screen-hop", a Windows 10/11 desktop utility (system-tray app, .NET 8 / WPF).

WHAT IT DOES
screen-hop lets one person reassign their desk's physical monitors between several
PCs on the same LAN, in one click, without pressing each monitor's physical input
button. Each monitor is cabled to multiple PCs at once (e.g. HDMI to the work PC,
DisplayPort to the gaming PC); the app switches which PC drives which monitor over the
video cable (DDC/CI). The same app runs on every PC as a peer — there is no server.

WHO USES IT
A single power user / developer / trader / home-lab owner with 2-4 PCs sharing 2-6
monitors on one desk. Fast, keyboard-friendly, no-nonsense. They live in the system tray.

PLATFORM & TONE
Windows 11 native feel (Fluent), light + dark themes, compact and dense (power-tool, not
consumer-cute). Trustworthy and honest — it must clearly communicate states that can go
wrong. No purple/violet as an accent color (project convention).

SURFACES TO DESIGN
1. Tray icon + tray flyout (PRIMARY surface). Shows each monitor with a friendly label,
   which PC currently drives it, and a quick way to send it to another online PC. Shows
   named presets (one click to apply). Shows connection status (which peers are online).
   Must stay fast to operate with mouse from the tray.
2. "Desk map" view — a visual layout of monitors as tiles, each tile showing which PC owns
   it, drag-or-click to reassign, with online PCs as targets. This is the hero view.
3. Onboarding wizard:
   a. Pair PCs — generate/show a mesh secret on one PC; enter it on another; plus an
      "Add host manually (IP/hostname)" path for when auto-discovery fails.
   b. Discover & confirm monitors.
   c. Calibration (important + non-obvious): the app can only learn a PC's input value for a
      monitor WHILE that PC is the one showing on it. Guide the user through making each PC
      active on each monitor once (they may need to press the monitor's physical input button
      to bootstrap). Show per-(PC, monitor) calibration status: Confirmed / Unknown-until-active.
   d. Labeling — when two identical monitors can't be told apart, ask the user to name them,
      with a "flash/identify this monitor" helper.
4. Named presets — capture the current monitor->PC layout as a named preset (e.g. "Work",
   "Trading", "Couch", "Pair"); list/apply/edit/delete.
5. Critical state screens / dialogs (design these deliberately, they define the product's honesty):
   - SWITCHING progress (a switch takes ~1-3s and can occasionally retry).
   - "YOU'LL BE BLIND" confirmation — shown before a switch/preset that would leave THIS PC
     with no visible screen ("This will leave this PC with no display. Continue?").
   - "STRANDED" state — a monitor whose owning PC is off/asleep/unreachable can't be switched
     by software; tell the user plainly to press the monitor's physical input button.
   - "DDC/CI DISABLED" error — the monitor isn't responding; guide the user to enable DDC/CI
     in the monitor's on-screen menu.
   - PRESET PARTIAL FAILURE — a multi-monitor preset is best-effort, not atomic; show per-monitor
     success/failure clearly (some switched, some didn't).
   - PARTITION / DEGRADED — some peers unreachable; disruptive actions paused.
6. Settings — paired peers (with revoke), manual hosts, autostart toggle, theme, optional
   external fallback tool path, advanced DDC timing.

HARD UX TRUTHS TO REFLECT (do not hide these)
- Switching is not instant (~1-3s, occasional retry) — show progress, never fake instant.
- Some monitors only work one direction or not at all — per-monitor status, not a uniform promise.
- A stranded monitor has no software fix — the physical button is the honest fallback.
- Presets are best-effort — never imply guaranteed all-or-nothing.

DELIVERABLES
Mockups for: tray flyout, desk-map hero view, the onboarding wizard steps, the preset
manager, each critical-state dialog above, and settings — in light and dark, Windows 11 style.
```

---
*Generated from a structured brainstorm → research → adversarially-reviewed design pass. The plan is deliberately honest about DDC/CI's hard limits: per-monitor variability, no atomicity, no stranded-monitor software recovery, and no pre-OS coverage.*

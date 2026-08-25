# screen-hop — local two-input product plan

> Product direction reset: 2026-08-25

## 1. Goal

screen-hop lets one person use one PC to switch one selected monitor back and forth between two
known physical inputs. It is a local DDC/CI utility, not a distributed monitor-routing system.

The primary user has a monitor connected to the controlling PC and another source—such as a
laptop, console, dock, or second computer—and wants one obvious switch action instead of navigating
the monitor's on-screen menu.

## 2. Locked scope

### In scope

- One running app on one PC.
- One selected local monitor.
- Exactly two user-named, locally observed input values.
- One-click toggle plus explicit Source A / Source B selection.
- Guided first-run capture and a real `A -> B -> A` compatibility test.
- Bounded retry, read-back verification when available, and honest inconclusive results.
- Stable local monitor identity, persisted configuration, quirks, and a physical-button fallback.
- Windows first; Linux and macOS remain best-effort through the existing cross-platform DDC layer.

### Out of scope

- LAN discovery, pairing, sockets, encryption, peer identity, or remote control.
- Multiple screen-hop agents coordinating with one another.
- Distributed ownership, leases, reconciliation, partitions, stranded-peer states, or presets.
- Multiple selected monitors in v1.
- Arbitrary input-code probing.
- Keyboard/mouse sharing, window-layout restoration, scheduling, or phone/web control.
- Native tray integration and global hotkeys until they are implemented as explicit follow-ups.

## 3. Product flow

### First run

1. Enumerate locally reachable DDC displays, including anonymous handles that are usable locally.
2. The user selects one monitor.
3. Read live VCP `0x60` and capture it as Source A while this PC is visible.
4. Ask the user to start listening and physically move the monitor to the other desired input.
5. Poll for a distinct observed value and capture it as Source B.
6. With explicit user consent, write the already observed Source A value and verify the return.
7. Let the user name A and B, validate, persist atomically, and enter the switch surface.

Failure to observe B or return to A means the setup is not proven. Do not create a ready
configuration; explain how to recover with the physical source button.

### Normal use

- Read the live value when possible and show A, B, or Unknown.
- The primary action targets the opposite of a known A/B source.
- When inactive-input reads are unavailable, the last successfully requested source may determine
  the opposite toggle. The UI labels this as last-known, never as verified live state.
- If neither a live nor last-known source exists, hide/disable blind toggle and show two explicit
  source buttons. Both still resolve only to captured values.
- While a write is running, disable duplicate actions and show progress.
- Update verified state only after matching read-back. Treat an accepted write with failed
  read-back as effective but inconclusive.

## 4. Safety invariants

1. The actuation boundary—not the UI—resolves `SourceSlot::A/B` to a value.
2. Both values must be present, distinct, and within `u16`; otherwise no write occurs.
3. The selected monitor must still resolve locally; otherwise no write occurs.
4. The executor's confirmed allow-list contains exactly A and B for that monitor.
5. Blocked quirks are additive and always override captured values.
6. Unknown raw values are never copied into a write request outside the guided capture flow.
7. Retry count and elapsed time remain bounded.
8. No automated test or normal startup performs a real monitor write.

## 5. Architecture

```text
Slint local window
      |
      v
UI controller / source-index guard
      |
      v
Dedicated local DDC worker
      |
      +--> LocalConfig + atomic persistence
      +--> LocalSwitcher
              |
              +--> SwitchExecutor (guards / retry / verify)
              +--> DdcHiDriver (enumerate / read / write 0x60)
              +--> QuirksDb (blocked values / timing / read-back policy)
```

`DdcHiDriver` is constructed and owned inside its worker thread. UI callbacks enqueue commands and
never block the Slint event loop on DDC. Results are polled back to the event loop. Monitor
enumeration has a bounded startup wait; an individual OS/backend DDC call still cannot be cancelled
and may require restarting the app if it stalls.

## 6. Configuration

The versioned local configuration contains:

- selected backend-local monitor address;
- optional normalized model token used only for model-wide quirks;
- optional local-handle aliases;
- Source A label and confirmed `u16` value;
- Source B label and confirmed `u16` value;
- optional last requested source.

The retired LAN configuration is not trusted as a local two-source setup. An old or incompatible
file opens guided setup and is replaced only when the new round trip succeeds. Old peer calibration
must never be promoted automatically. Retired credential files are left untouched and unused so
the refactor does not destroy user data.

## 7. Verification

### Automated

- A live A toggles only to B; live B toggles only to A.
- An unknown/unreadable source without safe last-known context issues no write.
- Explicit selection accepts only A/B source indexes.
- Duplicate, absent, blocked, corrupt, or out-of-range values issue no write.
- Read-back-inconclusive behavior remains visible and does not retry into flapping.
- Config round-trips atomically and malformed JSON fails closed.
- Local startup has no mesh secret, identity key, socket bind, mDNS, or peer requirement.
- Formatting, unit/integration tests, clippy with warnings denied, and all-target builds pass.

### Manual hardware gate

On the intended monitor/GPU/cable path:

1. Start with Source A visible.
2. Command A to B from the controlling PC.
3. Confirm the PC retains a usable DDC handle while B is visible.
4. Command B back to A from that same PC.
5. Repeat enough times to expose intermittent failures and record read-back behavior.

The local product is hardware-ready only after this exact sequence passes. The earlier two-agent
pull-to-self result is useful history but does not prove this one-controller path.

## 8. Follow-ups

- Native system-tray icon with hide/show lifecycle.
- Global keyboard shortcut.
- Broader real-hardware matrix and per-model quirks.
- Installer/signing polish.
- Optional support for more monitors or more than two inputs only if real usage justifies it.

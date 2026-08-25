# Remaining checklist — local two-input screen-hop

Legend: ✅ complete in code · ⬜ requires hardware, packaging, or follow-up work.

## Local product

- ✅ No LAN discovery, pairing, sockets, peer identity, distributed state, or mesh dependency.
- ✅ Versioned local configuration selects one monitor and exactly two named input sources.
- ✅ Incomplete, corrupt, duplicate, out-of-range, or blocked input configuration fails closed.
- ✅ Toggle resolves A/B inside the backend; UI indexes and raw file data cannot bypass guards.
- ✅ Bounded retry and read-back handling retained from the tested DDC executor.
- ✅ Dedicated DDC worker keeps hardware calls off the UI event loop.
- ✅ Compact local switch UI and guided setup replace peer routing, presets, and desk-map surfaces.
- ✅ Normal startup is the local product; preview is explicit.
- ✅ Installer launches local mode without a `--live` mesh flag.

## Automated verification

- ✅ A→B and B→A toggle selection tests.
- ✅ Unknown/unreadable/no-last-known causes no write.
- ✅ Safe last-requested fallback covers monitors whose inactive input cannot be read.
- ✅ Blocked-value precedence and exact two-value allow-list tests.
- ✅ Persistence round-trip, missing-file defaults, and corrupt-file failure tests.
- ✅ Full workspace format, test, clippy, and all-target build on the final combined tree.
- ✅ Render and inspect local flyout/setup/settings in both light and dark modes.

## Real hardware — required before release

- ⬜ Run the guided one-PC round-trip on the intended setup.
- ⬜ Confirm Source A → Source B succeeds from the controlling PC.
- ⬜ While Source B is visible, confirm the same app can write Source A and recover the display.
- ⬜ Record whether inactive-input reads work, fail, or become intermittent.
- ⬜ Repeat the complete round trip enough times to expose retry/hang behavior.
- ⬜ Verify physical OSD recovery instructions on every failure path.
- ⬜ Test a clean first run and setup with one and multiple enumerated display handles.

## Packaging and polish

- ⬜ Clean-install/uninstall test on supported Windows versions.
- ⬜ Verify per-user autostart launches the local app.
- ⬜ Code signing and release hashes.
- ⬜ Native tray icon and hide/show lifecycle (not implemented yet).
- ⬜ Optional global hotkey (not implemented yet).
- ⬜ Expand the hardware matrix before making broad compatibility claims.

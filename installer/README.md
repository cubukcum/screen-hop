# Windows installer & autostart

[`screen-hop.iss`](screen-hop.iss) is an [Inno Setup](https://jrsoftware.org/isinfo.php) script that
packages screen-hop for Windows.

## Build it

```sh
cargo build --release -p screenhop-ui -p screenhop-spike
"C:\Program Files (x86)\Inno Setup 6\ISCC.exe" installer\screen-hop.iss
```

Output: `installer\dist\screen-hop-setup.exe`. CI builds this on every push (the `installer` job in
[.github/workflows/ci.yml](../.github/workflows/ci.yml)) and publishes it plus a SHA-256.

## What it does

- **Per-user, no admin.** Installs to `%LOCALAPPDATA%\Programs\screen-hop` with
  `PrivilegesRequired=lowest` — no UAC prompt.
- **Autostart (opt-in).** A checkbox adds a per-user `HKCU\…\Run` entry that launches
  `screenhop-ui.exe --live` at sign-in. This is the admin-free alternative to a Scheduled Task.
- **Clean uninstall.** Removes the binaries, the Start-menu entries, and the autostart registry
  value. It deliberately **keeps** your config (calibration, pins, mesh secret) so a reinstall
  resumes where you left off; delete the config dir by hand if you want a clean slate.

## First run

1. Put the same **mesh secret** on each PC: write it to a file named `mesh-secret` (no extension) in
   the config dir `%APPDATA%\screen-hop\config`. Easiest, from PowerShell:
   ```powershell
   Set-Content "$env:APPDATA\screen-hop\config\mesh-secret" -Value "your-shared-passphrase" -Encoding ascii -NoNewline
   ```
   (A pairing UI is a follow-up; today this is a file.)
2. **Calibrate**, with this PC shown on the panels: `screenhop-ui --calibrate`.
3. Launch `screenhop-ui --live` (autostart does this for you).

## Config location

`%APPDATA%\screen-hop\config` — `identity.key`, `mesh-secret`, `pins.json`, `calibration.json`,
`labels.json`, `config.json`. Override the whole location with `SCREENHOP_CONFIG_DIR`. (`--live`
also prints this exact path if no `mesh-secret` is found.)

## Code signing

The installer and binaries ship **unsigned** for now; CI publishes SHA-256 sums so you can verify
integrity. SmartScreen will warn on first run. Signing (Azure Trusted Signing, or an OV/EV
certificate) is a planned follow-up — see the plan's decision log (§15).

## Known limitation: active-console session (D11)

screen-hop should only actuate DDC from the **active, interactive console session** (not a locked
screen, RDP, or a service/Session-0 context). That guard is **not yet enforced in code** — if you
autostart it and then lock/RDP, a switch could still be attempted. Tracked as a follow-up in
[docs/REMAINING-CHECKLIST.md](../docs/REMAINING-CHECKLIST.md).

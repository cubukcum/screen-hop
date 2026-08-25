# Windows installer

The Inno Setup package installs screen-hop per user without administrator rights.

## Build

```powershell
cargo build --release -p screenhop-ui
& "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" installer\screen-hop.iss
```

Output: `installer\dist\screen-hop-setup.exe`.

## Installed behavior

- Installs under `%LOCALAPPDATA%\Programs\screen-hop`.
- Adds a Start-menu shortcut for the normal local app.
- Optionally registers the executable in the current user's `Run` key for sign-in startup.
- Removes installed binaries and the autostart value on uninstall.
- Keeps the user's local monitor/source configuration so reinstalling does not force setup again.

No network ports, firewall rules, services, pairing secrets, identities, or peer pins are created.

The developer-only `screenhop-spike` hardware diagnostic is intentionally not bundled. Build and
run it from a source checkout when performing an opt-in hardware compatibility test; normal users
should use the app's guided setup.

## Configuration

The app uses the OS-standard per-user configuration directory and stores `config.json` plus
optional local/user quirk files. `SCREENHOP_CONFIG_DIR` overrides the directory for diagnostics or
portable testing.

## Signing

The package is currently unsigned. Release artifacts should include a published SHA-256 until a
code-signing path is configured.

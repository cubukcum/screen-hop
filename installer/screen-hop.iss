; Inno Setup script for screen-hop (Windows).
;
; Admin-free by design: installs per-user to %LOCALAPPDATA%\Programs\screen-hop and registers
; autostart via the per-user HKCU\...\Run key (no UAC, no Scheduled Task elevation). Build with:
;   cargo build --release -p screenhop-ui -p screenhop-spike
;   "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" installer\screen-hop.iss
; Output: installer\dist\screen-hop-setup.exe
;
; Code-signing: this ships UNSIGNED for now; publish the SHA-256 (CI does this) so users can verify.
; A signing pass (Azure Trusted Signing or an OV/EV cert) is a documented follow-up — see
; installer\README.md.

#define AppName "screen-hop"
#define AppVersion "0.1.0"

[Setup]
AppId={{8E5F4B2A-7C3D-4E1F-9A6B-screen-hop0001}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=screen-hop contributors
DefaultDirName={localappdata}\Programs\{#AppName}
DefaultGroupName={#AppName}
; Per-user install — no administrator rights required.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
DisableProgramGroupPage=yes
OutputDir=dist
OutputBaseFilename=screen-hop-setup
Compression=lzma2
SolidCompression=yes
ArchitecturesInstallIn64BitMode=x64
WizardStyle=modern
UninstallDisplayIcon={app}\screenhop-ui.exe

[Files]
Source: "..\target\release\screenhop-ui.exe"; DestDir: "{app}"; Flags: ignoreversion
; The hardware spike is handy for calibration/troubleshooting; include it if it was built.
Source: "..\target\release\screenhop-spike.exe"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist

[Icons]
Name: "{group}\screen-hop"; Filename: "{app}\screenhop-ui.exe"; Parameters: "--live"
Name: "{group}\Uninstall screen-hop"; Filename: "{uninstallexe}"

[Tasks]
Name: "autostart"; Description: "Start screen-hop automatically when I sign in"; GroupDescription: "Startup:"

[Registry]
; Per-user autostart (no admin). Removed automatically on uninstall.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; \
    ValueName: "screen-hop"; ValueData: """{app}\screenhop-ui.exe"" --live"; \
    Flags: uninsdeletevalue; Tasks: autostart

[Run]
Filename: "{app}\screenhop-ui.exe"; Parameters: "--live"; \
    Description: "Launch screen-hop now"; Flags: nowait postinstall skipifsilent

; Note: user config (calibration, pins, mesh secret) lives in the app's config dir and is
; intentionally NOT removed on uninstall, so a reinstall keeps your pairing/calibration.

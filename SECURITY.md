# Security policy

## Reporting a vulnerability

Please do not open a public issue for an unpatched vulnerability. Use GitHub's private security
advisory flow for this repository, or contact the maintainer privately when that is unavailable.
Include affected versions, reproduction steps, impact, and any suggested mitigation.

## Security model

screen-hop is a local desktop application. It does not listen on the network, discover peers,
accept remote commands, or store pairing credentials.

Its main safety boundary is the monitor write path:

- Only DDC/CI VCP feature `0x60` (Input Select) is written.
- Only the two values observed and confirmed during setup for the selected monitor are eligible.
- The values must be distinct and fit the protocol's 16-bit range.
- A panel quirk may block additional values; a blocked value is never overridden by configuration.
- A malformed local or user quirk layer disables monitor writes until the file is fixed or removed.
- Missing, incomplete, stale, or malformed configuration fails closed without a write.
- The executor bounds retry count and app-controlled waits, and surfaces unverified writes as
  inconclusive. A DDC call blocked inside the OS/backend cannot be interrupted by that ceiling.

Configuration is stored in the current user's standard application-config directory. It contains
monitor identifiers, friendly source names, the two captured input values, and the last requested
source. These are not secrets, but another process running as the same user may be able to modify
them. The app validates the configuration again at the actuation boundary rather than trusting the
UI or file contents.

## Hardware recovery

DDC/CI behavior varies by monitor, GPU, cable, dock, and operating system. A monitor may stop
accepting commands after switching away from the controlling PC. The physical monitor input button
is always the recovery path; screen-hop must never imply that software recovery is guaranteed. A
driver call can also stall despite the dedicated worker thread; close and reopen the app if an
operation never returns.

## Supported versions

This project is pre-1.0. Security fixes are applied to the latest revision only.

# Security Policy

## Reporting a vulnerability

Please report security issues **privately** — do not open a public GitHub issue.

- Use **GitHub → Security → "Report a vulnerability"** (private advisory) on this repository, or
- email the maintainer (see the `repository` owner on the GitHub page).

Include what you found, how to reproduce it, and the impact. We aim to acknowledge within a few days
and will coordinate a fix and disclosure timeline with you.

## Threat model (summary)

The full model is in [docs/PLAN-screen-hop.md](docs/PLAN-screen-hop.md) §9. In short:

- **Scope:** screen-hop is **LAN / Private-network only** — no WAN, no UPnP, no port-forwarding.
  The threat it defends against is **denial-of-visibility by an *unpaired* host** on a personal LAN
  (a stranger making your monitors switch). An already-paired peer run by the same operator is
  **out of scope** — pairing is the trust boundary.
- **Group secret:** a single shared **mesh secret** is stretched with **Argon2id** (pinned
  parameters) into a 32-byte group key. **Every** mesh message is sealed with
  **XChaCha20-Poly1305** (AEAD) bound to a protocol AAD; replay and out-of-sequence frames are
  rejected.
- **Peer identity:** each install has an **Ed25519** identity, **pinned trust-on-first-use (TOFU)**.
  A changed key for a previously-pinned peer is **refused** (MITM / impersonation guard). The pin
  store is persisted so this guarantee survives restarts.
- **Soft-brick guard (D7):** the actuator only ever writes a value the peer has **self-calibrated**
  on that panel. Quirk data (including from community PRs) can only **restrict** behaviour
  (e.g. add blocked values); it can **never authorize** a `0x60` write. This is what makes accepting
  community quirk contributions safe.
- **Actuation surface:** switching uses DDC/CI in a running, logged-in console session only. It
  cannot touch BIOS/pre-OS/lock screens, and an unpaired host cannot trigger a write.

## Operational caveat: secret storage & re-pairing (Windows / DPAPI)

The mesh secret and pinned-peer state are stored **locally**. On Windows, secrets protected with
**DPAPI** are tied to the **current Windows user account on that machine**. Consequently:

- Moving the install to a **different user account**, resetting the Windows profile, or restoring to
  a **different machine** makes the protected secret unreadable — you will need to **re-pair** that
  node (re-enter the mesh secret; peers will re-pin its identity).
- This is expected behaviour, not data loss: re-pairing re-establishes trust via the normal TOFU
  flow. Keep your mesh secret somewhere you can re-enter it.

## Supported versions

screen-hop is pre-1.0; security fixes target `main` and the latest tag.

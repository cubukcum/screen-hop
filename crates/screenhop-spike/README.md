# M0 — DDC/CI feasibility spike (Rust / ddc-hi)

A **throwaway** cross-platform CLI (see [`docs/PLAN-screen-hop.md`](../../docs/PLAN-screen-hop.md), milestone **M0**) that answers screen-hop's go/no-go question on real hardware, on **Windows, Linux, and macOS**:

> Can a machine switch a shared monitor **to itself** over DDC/CI (VCP `0x60`) while it is **not** the currently-shown input? (the "pull-to-self" path)

It uses the same `ddc-hi` crate the product will, so it doubles as a per-OS validation of that dependency.

## Build & run

```sh
# from the repo root
cargo run -p screenhop-spike -- list      # read-only: list monitors + current input
cargo run -p screenhop-spike              # interactive menu (read / set / guided test)
```

Build a standalone binary to copy to another machine:

```sh
cargo build -p screenhop-spike --release
# binary at: target/release/screenhop-spike[.exe]
```

## Per-OS prerequisites
- **Windows** — none (uses the Monitor Configuration API / NVIDIA backend).
- **Linux** — load `i2c-dev` (`sudo modprobe i2c-dev`) and grant `/dev/i2c-*` access (add your user to the `i2c` group + a udev rule). Without this, enumeration silently finds nothing.
- **macOS** — Apple Silicon must drive the monitor over **USB‑C / Thunderbolt (DP Alt Mode)**; the built‑in HDMI port (M1 / base M2) and DisplayLink/most hubs can't do DDC. Reads are unreliable on Apple Silicon (writes are fire‑and‑forget).

## The pull-to-self test (run per monitor, across 2 machines)

Interactive menu → **option 3**, on the machine reachable via the *non-active* input:
1. **STEP 1** — make the monitor show **this** machine; the tool reads this machine's `0x60` value.
2. **STEP 2** — press the monitor's physical button to switch it to the **other** machine.
3. **STEP 3** — the tool writes this machine's value (while not shown) and asks whether the monitor came back.

**PASS** → primary path works on this panel. **FAIL** → that panel needs the push-release fallback; record the model.

At the end of the test the spike offers to record a **formal verdict row** and appends it to
[`docs/hardware/pull-to-self-verdicts.md`](../../docs/hardware/pull-to-self-verdicts.md) — the
authoritative M0 go/no-go log. M0 closes only once **≥ 2 distinct rigs** are recorded there and a
go/no-go is written.

> Prior informal result: **PASS** on the maintainer's Windows + AOC 27P2DG5 setup — now tracked
> (and pending formal re-capture) in the verdict log linked above.

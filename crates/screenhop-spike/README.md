# Local DDC/CI compatibility spike (Rust / ddc-hi)

A **developer-only** cross-platform CLI for validating screen-hop's one-PC behavior on real
hardware, on **Windows, Linux, and macOS**:

> Can one PC switch a monitor from input A to B and automatically bring it back to A?

It uses the same `ddc-hi` crate as the product. The end-user installer does not include this
diagnostic binary; normal app setup provides the supported configuration flow.

## Build & run

```sh
# from the repo root
cargo run -p screenhop-spike -- list      # read-only: list monitors + current input
cargo run -p screenhop-spike              # interactive menu (read / guided local test)
cargo run -p screenhop-spike -- local-round-trip  # guided Option A one-PC test
```

Build a standalone developer diagnostic:

```sh
cargo build -p screenhop-spike --release
# binary at: target/release/screenhop-spike[.exe]
```

## Per-OS prerequisites

- **Windows** — none (uses the Monitor Configuration API / NVIDIA backend).
- **Linux** — load `i2c-dev` (`sudo modprobe i2c-dev`) and grant `/dev/i2c-*` access (add your user to the `i2c` group + a udev rule). Without this, enumeration silently finds nothing.
- **macOS** — Apple Silicon must drive the monitor over **USB‑C / Thunderbolt (DP Alt Mode)**; the built‑in HDMI port (M1 / base M2) and DisplayLink/most hubs can't do DDC. Reads are unreliable on Apple Silicon (writes are fire‑and‑forget).

## The Option A local round-trip test (one PC)

Run `cargo run -p screenhop-spike -- local-round-trip`, or choose **option 2** in the interactive
menu. Keep the terminal visible on another display or through a remote session if changing the
tested monitor's source will hide it.

The guided flow is deliberately conservative:

1. Put the monitor on the original input connected to this PC (**A**). The tool captures A only
   from a successful live VCP `0x60` read.
2. Use the monitor's **physical OSD/input button** to switch it to the intended alternate source
   (**B**). The tool requires two consecutive matching live reads of a value distinct from A. If
   reads fail, remain unchanged, or disagree, it aborts without writing.
3. After explicit confirmation, the tool writes only captured A to prove this PC can recover the
   monitor from B. It requires visual confirmation and a live A read-back before continuing.
4. Optionally, after another explicit confirmation, it writes captured B, waits about 3.5 seconds,
   and automatically writes captured A. A **PASS** requires successful writes, user confirmation
   that the screen visibly went A → B → A, and a final live read of A.

The local test never accepts a typed input code, scans values, or guesses a value. If the picture
does not return, use the monitor's **physical OSD/input button** to select the original port (A).
The test never writes files or modifies the app's persisted setup. A skipped or partially confirmed
sequence is never reported as passing.

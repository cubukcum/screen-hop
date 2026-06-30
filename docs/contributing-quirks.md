# Contributing a panel quirk

Quirks are **panel-global behaviour facts** for a specific monitor model — how it behaves over
DDC/CI. They live in [`quirks/quirks.json`](../quirks/quirks.json), replicate across the mesh, and
are merged with precedence **user > local-learned > shipped**.

> **Safety invariant (D7):** a quirk can only ever *restrict* behaviour (block values, slow timing,
> hint a direction). It can **never authorize** a `0x60` write — only a peer's own self-calibrated
> value is ever written. That is what makes accepting these PRs safe.

## 1. Measure your panel

Run the M0 spike on the machine cabled to the monitor and record what you observe:

```sh
cargo run -p screenhop-spike            # option 3 = guided pull-to-self test
```

Note: is DDC/CI readable? does pull-to-self work? how long does it take to settle? is read-back
reliable? are any input values unsafe to write? If you're recording a feasibility verdict, the
spike can append it to [docs/hardware/pull-to-self-verdicts.md](hardware/pull-to-self-verdicts.md).

## 2. Find the key

The key is the panel's `monitor_id` (the stable 12-hex id the spike prints in the `MonitorId`
column) or a model token. Use the same key other entries use for that model if one exists.

## 3. Add the entry

Each entry is a JSON object; **every field is optional** — set only what you actually measured.

| Field | Type | Meaning |
|---|---|---|
| `working_direction` | `"pull_to_self"` \| `"push_release"` | Which direction is known to work (advisory hint). |
| `readback_unreliable` | bool | `true` if the panel's `0x60` read-back can't be trusted (skip verify). |
| `settle_ms` | int | Delay after a write before reading back (slow panels need more). |
| `sleep_multiplier` | float | Scale factor for timing on especially slow panels. |
| `ddc_off_by_default` | bool | DDC/CI ships disabled in the OSD on this model. |
| `requires_active_input` | bool | Only honours DDC over its currently-active input. |
| `blocked_input_values` | int[] | Values that must **never** be written to this panel (safety). **Additive** across layers. |
| `pbp_capable` | bool | Supports picture-by-picture. |
| `source` | string | Where the fact came from (e.g. `"shipped"`, your handle, a forum link). |

Example (mirrors the shipped entries):

```json
"SAM-U32H750": {
  "readback_unreliable": false,
  "settle_ms": 2000,
  "blocked_input_values": [],
  "source": "shipped"
}
```

## 4. Verify and open a PR

```sh
cargo test -p screenhop-quirks   # confirms the DB still parses and merges correctly
```

Then open a PR describing the panel (make/model), how you measured it, and your setup. Please
**don't** add a `blocked_input_values` entry you haven't confirmed is genuinely unsafe — blocking a
valid input degrades that panel for everyone.

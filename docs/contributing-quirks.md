# Contributing a panel quirk

Quirks are **panel-global behaviour facts** for a specific monitor model — how it behaves over
DDC/CI. They live in [`quirks/quirks.json`](../quirks/quirks.json) and are merged with precedence
**user > local-learned > shipped**.

> **Safety invariant:** a quirk can only ever *restrict* behaviour (block values or slow timing).
> It can **never authorize** a `0x60` write — only the two values observed locally during setup may
> be written. That is what makes accepting these PRs safe.

## 1. Measure your panel

Run the hardware spike on the controlling machine and record what you observe:

```sh
cargo run -p screenhop-spike            # choose the guided local round-trip test
```

Note: is DDC/CI readable on both active and inactive inputs? Can the same PC complete `A -> B -> A`?
How long does it take to settle? Is read-back reliable? Are any input values unsafe to write?

## 2. Find the key

Use the normalized manufacturer/model token for shipped or community quirks, for example
`SAM-U32H750`. This safely applies the restriction to every unit of that model. An exact local
override may instead use the backend-specific `local id` printed by `screenhop-ui --monitors`, but
that address is machine-specific and does not belong in the shipped database. The 12-hex
fingerprint printed by the developer spike is diagnostic only; it is not used for DDC addressing.

## 3. Add the entry

Each entry is a JSON object; **every field is optional** — set only what you actually measured.

| Field | Type | Meaning |
|---|---|---|
| `working_direction` | `"pull_to_self"` \| `"push_release"` | Historical direction hint retained for database compatibility. |
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

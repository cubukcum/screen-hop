# M0 — Pull-to-self hardware verdicts

This is the **formal record** for screen-hop's M0 go/no-go gate. The architecture commits to
**pull-to-self** as the primary switching direction (PLAN §6.2); M0 exists to confirm that bet on
real hardware *before* the rest of the system depends on it.

> **M0 exit criterion** (PLAN-screen-hop.md, milestone table + DoD line 514):
> pull-to-self confirmed on **≥ 2 distinct real PC/monitor combinations**, each with a recorded
> per-monitor verdict, and a written **go/no-go** decision below.

A single panel passing is *not* enough to close M0 — the point is to see the behavior across at
least two different rigs (different GPU/cable/panel) so we know the primary path isn't a one-off.

## How to record a verdict

Run the M0 spike on the machine reachable via the monitor's **non-active** input and use the
guided test:

```sh
cargo run -p screenhop-spike      # interactive menu → option [3] Guided pull-to-self test
```

At the end of the test the spike asks whether to record a formal verdict. If you say yes it prints
a ready-to-paste table row **and appends it directly to this file** (below the marker) when run from
the repo root. If it can't find this file, paste the printed row in by hand.

## Recorded verdicts

| Date | Rig (PC · OS · GPU) | Monitor (mfr · model) | monitor_id | Cable/port | 0x60 | Result | Settle | Notes |
|------|--------------------|-----------------------|------------|------------|------|--------|--------|-------|
<!-- VERDICT-ROWS: the spike inserts new rows directly below this line -->
| _prior_ | maintainer · Windows · (unrecorded GPU) | AOC · 27P2DG5 | (not captured) | (not captured) | — | PASS | — | Informal pre-spike result migrated from the spike README; **re-run via the spike to formalize** (capture monitor_id, cable, value). |

> `Result` legend: **PASS** = monitor switched back to this machine while not shown ·
> **FAIL** = pull-to-self did not work (panel needs push-release fallback) ·
> **PARTIAL** = intermittent / required retries (note specifics).

## Go/no-go decision

**Status: ⏳ PENDING** — needs **≥ 2 distinct rigs** recorded above with non-`_prior_` verdicts.

When the bar is met, write the decision here, e.g.:

> **GO** — pull-to-self confirmed on _N_ rigs (list them). Push-release retained as the
> per-monitor fallback for panels that FAIL. Decision recorded by _name_ on _date_.

…or, if the hardware says otherwise:

> **NO-GO / revise** — pull-to-self failed on _…_; reconsider default direction. _Rationale._

## Maintainer checklist (the part only you can do)

- [ ] Cable **two** PCs to the same monitor (or use two different monitors across two PCs).
- [ ] Enable DDC/CI in each monitor's OSD; on Linux load `i2c-dev` and grant `/dev/i2c-*` access.
- [ ] On **Rig A**: run the spike, option 3 — record the verdict row (auto-appended above).
- [ ] On **Rig B** (different GPU/cable/panel where possible): repeat — record its verdict row.
- [ ] Replace the `_prior_` AOC row with a real spike-captured row (or add a fresh one alongside it).
- [ ] Write the **GO / NO-GO** decision in the section above.
- [ ] Tick **`[x] M0 go/no-go recorded`** in the PLAN-screen-hop.md DoD checklist (line 514).

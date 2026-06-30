//! Orchestration decision logic (milestone M4): who actuates a switch, whether an operation
//! would blind the operator, and how to order a preset's switches. Pure functions over the
//! ownership map — no I/O — so they are exhaustively unit-tested.

use std::collections::{HashMap, HashSet};

use screenhop_core::{SwitchDirection, SwitchOutcome};
use screenhop_state::OwnershipMap;

pub type PeerId = String;
pub type MonitorId = String;

/// A fully resolved switch: which monitor goes to which target, and which peer must perform the
/// DDC write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchOp {
    pub monitor_id: MonitorId,
    pub target: PeerId,
    pub actuator: PeerId,
    pub direction: SwitchDirection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActuationError {
    /// No reachable peer can perform the switch (owner offline for push-release, or target
    /// offline for pull-to-self) — the monitor is stranded; the physical button is the fallback.
    Stranded {
        monitor_id: MonitorId,
        owner: Option<PeerId>,
    },
    /// The mesh is partitioned / degraded (peers unreachable). Disruptive ops are paused so a peer
    /// acting on a stale ownership cache cannot cause a double-write/flap (§8.6 partition guard).
    Degraded { monitor_id: MonitorId },
}

/// Decide who actuates a switch of `monitor_id` to `target`.
///
/// - `PullToSelf` (default): the **target** actuates (writes its own input value).
/// - `PushRelease`: the **current owner** actuates (hands the panel away); requires the owner
///   to be known and online.
///
/// When `degraded` is true (the mesh has lost contact with peers), the switch is refused with
/// [`ActuationError::Degraded`] rather than acted on against a possibly-stale cache (§8.6).
pub fn resolve_actuation(
    monitor_id: &str,
    target: &str,
    direction: SwitchDirection,
    ownership: &OwnershipMap,
    online: &HashSet<String>,
    degraded: bool,
) -> Result<SwitchOp, ActuationError> {
    if degraded {
        return Err(ActuationError::Degraded {
            monitor_id: monitor_id.to_owned(),
        });
    }
    let actuator =
        match direction {
            SwitchDirection::PullToSelf => target.to_owned(),
            SwitchDirection::PushRelease => ownership
                .owner(monitor_id)
                .map(str::to_owned)
                .ok_or_else(|| ActuationError::Stranded {
                    monitor_id: monitor_id.to_owned(),
                    owner: None,
                })?,
        };

    if !online.contains(&actuator) {
        return Err(ActuationError::Stranded {
            monitor_id: monitor_id.to_owned(),
            owner: Some(actuator),
        });
    }

    Ok(SwitchOp {
        monitor_id: monitor_id.to_owned(),
        target: target.to_owned(),
        actuator,
        direction,
    })
}

/// True if applying `assignments` (monitor → new owner) would leave `me` — who currently owns at
/// least one monitor — owning **none**, i.e. with no visible screen (§8.7 blind warning).
pub fn would_go_blind(
    me: &str,
    assignments: &[(MonitorId, PeerId)],
    ownership: &OwnershipMap,
    all_monitors: &[MonitorId],
) -> bool {
    let assigned: HashMap<&str, &str> = assignments
        .iter()
        .map(|(m, p)| (m.as_str(), p.as_str()))
        .collect();

    let owns_before = all_monitors.iter().any(|m| ownership.owner(m) == Some(me));
    let owns_after = all_monitors.iter().any(|m| {
        let final_owner = assigned
            .get(m.as_str())
            .copied()
            .or_else(|| ownership.owner(m));
        final_owner == Some(me)
    });

    owns_before && !owns_after
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedSwitch {
    pub monitor_id: MonitorId,
    pub target: PeerId,
}

/// An ordered, best-effort preset plan plus whether it would blind the operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchPlan {
    pub ops: Vec<PlannedSwitch>,
    pub blinds_operator: bool,
    /// True when the mesh is partitioned/degraded: `ops` is empty and the caller must not actuate.
    pub degraded: bool,
}

/// Plan a preset for operator `me`: order the switches so the operator's own currently-visible
/// panels are handed away **last** (§8.7), and flag whether the whole batch would blind them.
///
/// When `degraded` is true the plan is empty and `degraded` is set, so a caller that ignores the
/// flag still does nothing — disruptive ops pause during a partition (§8.6).
pub fn plan_preset(
    me: &str,
    assignments: &[(MonitorId, PeerId)],
    ownership: &OwnershipMap,
    all_monitors: &[MonitorId],
    degraded: bool,
) -> SwitchPlan {
    if degraded {
        return SwitchPlan {
            ops: Vec::new(),
            blinds_operator: false,
            degraded: true,
        };
    }

    let mut ordered = assignments.to_vec();
    // Stable sort: monitors currently owned by `me` (key 1) move after the rest (key 0).
    ordered.sort_by_key(|(m, _)| u8::from(ownership.owner(m) == Some(me)));

    SwitchPlan {
        ops: ordered
            .into_iter()
            .map(|(monitor_id, target)| PlannedSwitch { monitor_id, target })
            .collect(),
        blinds_operator: would_go_blind(me, assignments, ownership, all_monitors),
        degraded: false,
    }
}

/// Per-op result of executing a preset (best-effort): which switch, and how it turned out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchOpResult {
    pub monitor_id: MonitorId,
    pub target: PeerId,
    pub outcome: SwitchOutcome,
}

/// The outcome of executing a whole preset, with per-monitor results so partial failure is visible.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PresetOutcome {
    pub results: Vec<SwitchOpResult>,
}

impl PresetOutcome {
    /// True only if at least one op ran and every op effectively succeeded.
    pub fn all_succeeded(&self) -> bool {
        !self.results.is_empty()
            && self
                .results
                .iter()
                .all(|r| r.outcome.is_effective_success())
    }

    /// The ops that did not effectively succeed.
    pub fn failures(&self) -> Vec<&SwitchOpResult> {
        self.results
            .iter()
            .filter(|r| !r.outcome.is_effective_success())
            .collect()
    }

    /// True when the batch is a *partial* failure: some ops succeeded and some failed (§8.7 — the
    /// case the UI must surface per-monitor rather than as an all-or-nothing result).
    pub fn partial_failure(&self) -> bool {
        let any_ok = self
            .results
            .iter()
            .any(|r| r.outcome.is_effective_success());
        let any_fail = self
            .results
            .iter()
            .any(|r| !r.outcome.is_effective_success());
        any_ok && any_fail
    }
}

/// Execute an ordered preset [`SwitchPlan`] **best-effort**: perform every op — a failure does NOT
/// abort the batch — and collect per-monitor outcomes so partial failure is surfaced (§8.5/§8.7).
/// `perform` actuates one switch (locally, or by routing over the mesh) and returns its outcome;
/// because the plan already orders the operator's own panels last, a mid-batch blind is deferred as
/// long as possible. A degraded plan carries no ops, so nothing is actuated.
pub fn execute_plan(
    plan: &SwitchPlan,
    mut perform: impl FnMut(&PlannedSwitch) -> SwitchOutcome,
) -> PresetOutcome {
    let mut results = Vec::with_capacity(plan.ops.len());
    for op in &plan.ops {
        let outcome = perform(op);
        results.push(SwitchOpResult {
            monitor_id: op.monitor_id.clone(),
            target: op.target.clone(),
            outcome,
        });
    }
    PresetOutcome { results }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn online(peers: &[&str]) -> HashSet<String> {
        peers.iter().map(|s| (*s).to_owned()).collect()
    }

    fn ownership(pairs: &[(&str, &str)]) -> OwnershipMap {
        let mut o = OwnershipMap::new();
        for (i, (m, owner)) in pairs.iter().enumerate() {
            o.merge(m, Some((*owner).to_owned()), (i + 1) as u64);
        }
        o
    }

    fn mons(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn assign(list: &[(&str, &str)]) -> Vec<(MonitorId, PeerId)> {
        list.iter()
            .map(|(m, p)| ((*m).to_owned(), (*p).to_owned()))
            .collect()
    }

    #[test]
    fn pull_to_self_actuator_is_the_target() {
        let own = ownership(&[("m1", "A")]);
        let op = resolve_actuation(
            "m1",
            "B",
            SwitchDirection::PullToSelf,
            &own,
            &online(&["A", "B"]),
            false,
        )
        .unwrap();
        assert_eq!(op.actuator, "B");
        assert_eq!(op.target, "B");
    }

    #[test]
    fn pull_to_self_with_offline_target_is_stranded() {
        let own = ownership(&[("m1", "A")]);
        let r = resolve_actuation(
            "m1",
            "B",
            SwitchDirection::PullToSelf,
            &own,
            &online(&["A"]),
            false,
        );
        assert_eq!(
            r,
            Err(ActuationError::Stranded {
                monitor_id: "m1".into(),
                owner: Some("B".into())
            })
        );
    }

    #[test]
    fn push_release_actuator_is_the_current_owner() {
        let own = ownership(&[("m1", "A")]);
        let op = resolve_actuation(
            "m1",
            "B",
            SwitchDirection::PushRelease,
            &own,
            &online(&["A", "B"]),
            false,
        )
        .unwrap();
        assert_eq!(op.actuator, "A");
    }

    #[test]
    fn push_release_strands_when_owner_offline_or_unknown() {
        let own = ownership(&[("m1", "A")]);
        assert!(matches!(
            resolve_actuation(
                "m1",
                "B",
                SwitchDirection::PushRelease,
                &own,
                &online(&["B"]),
                false
            ),
            Err(ActuationError::Stranded { .. })
        ));
        let empty = OwnershipMap::new();
        assert_eq!(
            resolve_actuation(
                "m1",
                "B",
                SwitchDirection::PushRelease,
                &empty,
                &online(&["A", "B"]),
                false
            ),
            Err(ActuationError::Stranded {
                monitor_id: "m1".into(),
                owner: None
            })
        );
    }

    #[test]
    fn degraded_mesh_refuses_switches_and_empties_presets() {
        let own = ownership(&[("m1", "A"), ("m2", "A")]);
        let all = mons(&["m1", "m2"]);
        // A single switch is refused outright when the mesh is partitioned.
        assert_eq!(
            resolve_actuation(
                "m1",
                "B",
                SwitchDirection::PullToSelf,
                &own,
                &online(&["A", "B"]),
                true
            ),
            Err(ActuationError::Degraded {
                monitor_id: "m1".into()
            })
        );
        // A preset produces no ops while degraded, so a caller ignoring the flag still does nothing.
        let plan = plan_preset("A", &assign(&[("m1", "B"), ("m2", "B")]), &own, &all, true);
        assert!(plan.degraded);
        assert!(plan.ops.is_empty());
    }

    #[test]
    fn blind_only_when_handing_away_the_last_owned_monitor() {
        let own = ownership(&[("m1", "A"), ("m2", "A")]);
        let all = mons(&["m1", "m2"]);
        assert!(would_go_blind(
            "A",
            &assign(&[("m1", "B"), ("m2", "B")]),
            &own,
            &all
        ));
        assert!(!would_go_blind("A", &assign(&[("m1", "B")]), &own, &all)); // keeps m2
    }

    #[test]
    fn not_blind_when_operator_owns_nothing_to_begin_with() {
        let own = ownership(&[("m1", "B")]);
        let all = mons(&["m1"]);
        assert!(!would_go_blind("A", &assign(&[("m1", "B")]), &own, &all));
    }

    #[test]
    fn preset_moves_operator_panels_last_and_flags_blind_correctly() {
        let own = ownership(&[("m1", "B"), ("m2", "A")]);
        let all = mons(&["m1", "m2"]);
        // Operator A's own panel (m2) is listed FIRST but must be scheduled LAST.
        let plan = plan_preset("A", &assign(&[("m2", "B"), ("m1", "A")]), &own, &all, false);
        assert_eq!(plan.ops.last().unwrap().monitor_id, "m2");
        assert!(!plan.blinds_operator); // ends owning m1
    }

    #[test]
    fn whole_desk_handoff_flags_blind() {
        let own = ownership(&[("m1", "A"), ("m2", "A")]);
        let all = mons(&["m1", "m2"]);
        let plan = plan_preset("A", &assign(&[("m1", "B"), ("m2", "B")]), &own, &all, false);
        assert!(plan.blinds_operator);
    }

    #[test]
    fn execute_plan_runs_every_op_in_order_and_collects_partial_failure() {
        let own = ownership(&[("m1", "B"), ("m2", "A")]);
        let all = mons(&["m1", "m2"]);
        // A hands its own panel (m2) away last; m1 first.
        let plan = plan_preset("A", &assign(&[("m2", "B"), ("m1", "A")]), &own, &all, false);

        // m1 fails (panel unsupported), m2 succeeds — best-effort must still run BOTH.
        let mut order = Vec::new();
        let outcome = execute_plan(&plan, |op| {
            order.push(op.monitor_id.clone());
            if op.monitor_id == "m1" {
                SwitchOutcome::Unsupported
            } else {
                SwitchOutcome::Success
            }
        });

        assert_eq!(
            order,
            vec!["m1".to_string(), "m2".to_string()],
            "ops run in plan order"
        );
        assert_eq!(outcome.results.len(), 2);
        assert!(outcome.partial_failure());
        assert!(!outcome.all_succeeded());
        assert_eq!(outcome.failures().len(), 1);
        assert_eq!(outcome.failures()[0].monitor_id, "m1");
    }

    #[test]
    fn execute_plan_reports_all_succeeded_and_does_nothing_when_degraded() {
        let own = ownership(&[("m1", "A")]);
        let all = mons(&["m1"]);
        let ok = execute_plan(
            &plan_preset("A", &assign(&[("m1", "B")]), &own, &all, false),
            |_| SwitchOutcome::Success,
        );
        assert!(ok.all_succeeded());
        assert!(!ok.partial_failure());

        // Degraded -> empty plan -> nothing performed.
        let mut calls = 0;
        let degraded = execute_plan(
            &plan_preset("A", &assign(&[("m1", "B")]), &own, &all, true),
            |_| {
                calls += 1;
                SwitchOutcome::Success
            },
        );
        assert_eq!(calls, 0, "a degraded plan actuates nothing");
        assert!(degraded.results.is_empty());
        assert!(!degraded.all_succeeded());
    }
}

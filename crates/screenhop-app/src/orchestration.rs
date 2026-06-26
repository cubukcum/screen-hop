//! Orchestration decision logic (milestone M4): who actuates a switch, whether an operation
//! would blind the operator, and how to order a preset's switches. Pure functions over the
//! ownership map — no I/O — so they are exhaustively unit-tested.

use std::collections::{HashMap, HashSet};

use screenhop_core::SwitchDirection;
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
}

/// Decide who actuates a switch of `monitor_id` to `target`.
///
/// - `PullToSelf` (default): the **target** actuates (writes its own input value).
/// - `PushRelease`: the **current owner** actuates (hands the panel away); requires the owner
///   to be known and online.
pub fn resolve_actuation(
    monitor_id: &str,
    target: &str,
    direction: SwitchDirection,
    ownership: &OwnershipMap,
    online: &HashSet<String>,
) -> Result<SwitchOp, ActuationError> {
    let actuator = match direction {
        SwitchDirection::PullToSelf => target.to_owned(),
        SwitchDirection::PushRelease => {
            ownership
                .owner(monitor_id)
                .map(str::to_owned)
                .ok_or_else(|| ActuationError::Stranded {
                    monitor_id: monitor_id.to_owned(),
                    owner: None,
                })?
        }
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
        let final_owner = assigned.get(m.as_str()).copied().or_else(|| ownership.owner(m));
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
}

/// Plan a preset for operator `me`: order the switches so the operator's own currently-visible
/// panels are handed away **last** (§8.7), and flag whether the whole batch would blind them.
pub fn plan_preset(
    me: &str,
    assignments: &[(MonitorId, PeerId)],
    ownership: &OwnershipMap,
    all_monitors: &[MonitorId],
) -> SwitchPlan {
    let mut ordered = assignments.to_vec();
    // Stable sort: monitors currently owned by `me` (key 1) move after the rest (key 0).
    ordered.sort_by_key(|(m, _)| u8::from(ownership.owner(m) == Some(me)));

    SwitchPlan {
        ops: ordered
            .into_iter()
            .map(|(monitor_id, target)| PlannedSwitch { monitor_id, target })
            .collect(),
        blinds_operator: would_go_blind(me, assignments, ownership, all_monitors),
    }
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
        list.iter().map(|(m, p)| ((*m).to_owned(), (*p).to_owned())).collect()
    }

    #[test]
    fn pull_to_self_actuator_is_the_target() {
        let own = ownership(&[("m1", "A")]);
        let op = resolve_actuation("m1", "B", SwitchDirection::PullToSelf, &own, &online(&["A", "B"])).unwrap();
        assert_eq!(op.actuator, "B");
        assert_eq!(op.target, "B");
    }

    #[test]
    fn pull_to_self_with_offline_target_is_stranded() {
        let own = ownership(&[("m1", "A")]);
        let r = resolve_actuation("m1", "B", SwitchDirection::PullToSelf, &own, &online(&["A"]));
        assert_eq!(
            r,
            Err(ActuationError::Stranded { monitor_id: "m1".into(), owner: Some("B".into()) })
        );
    }

    #[test]
    fn push_release_actuator_is_the_current_owner() {
        let own = ownership(&[("m1", "A")]);
        let op = resolve_actuation("m1", "B", SwitchDirection::PushRelease, &own, &online(&["A", "B"])).unwrap();
        assert_eq!(op.actuator, "A");
    }

    #[test]
    fn push_release_strands_when_owner_offline_or_unknown() {
        let own = ownership(&[("m1", "A")]);
        assert!(matches!(
            resolve_actuation("m1", "B", SwitchDirection::PushRelease, &own, &online(&["B"])),
            Err(ActuationError::Stranded { .. })
        ));
        let empty = OwnershipMap::new();
        assert_eq!(
            resolve_actuation("m1", "B", SwitchDirection::PushRelease, &empty, &online(&["A", "B"])),
            Err(ActuationError::Stranded { monitor_id: "m1".into(), owner: None })
        );
    }

    #[test]
    fn blind_only_when_handing_away_the_last_owned_monitor() {
        let own = ownership(&[("m1", "A"), ("m2", "A")]);
        let all = mons(&["m1", "m2"]);
        assert!(would_go_blind("A", &assign(&[("m1", "B"), ("m2", "B")]), &own, &all));
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
        let plan = plan_preset("A", &assign(&[("m2", "B"), ("m1", "A")]), &own, &all);
        assert_eq!(plan.ops.last().unwrap().monitor_id, "m2");
        assert!(!plan.blinds_operator); // ends owning m1
    }

    #[test]
    fn whole_desk_handoff_flags_blind() {
        let own = ownership(&[("m1", "A"), ("m2", "A")]);
        let all = mons(&["m1", "m2"]);
        let plan = plan_preset("A", &assign(&[("m1", "B"), ("m2", "B")]), &own, &all);
        assert!(plan.blinds_operator);
    }
}

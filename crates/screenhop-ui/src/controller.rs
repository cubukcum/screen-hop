//! UI controller / view-model bridge (M5): the real data path between [`screenhop_app`]'s live
//! mesh state and the Slint tray surfaces.
//!
//! It reads the replicated [`MeshState`] (ownership, peer presence) into UI-facing view models, and
//! turns user intents (switch, preset) into backend calls — replacing the design-preview's
//! hardcoded mock data. Everything here is pure/testable; the Slint property binding and the live
//! mesh event loop live in `main.rs` (and are verified on the 2-PC rig per the M5 checklist).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use screenhop_app::{
    execute_plan, plan_preset, would_go_blind, MeshState, PlannedSwitch, PresetOutcome,
};
use screenhop_core::SwitchOutcome;
use screenhop_state::OwnershipState;

/// UI-facing ownership state for a monitor — the backend states plus "this is mine".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorUiState {
    /// This machine is the believed live source.
    Mine,
    /// Another peer drives it.
    OwnedByPeer,
    /// Not owned / not yet observed.
    Unowned,
    /// Owner unreachable, no software recovery — physical input button only.
    Stranded,
    /// DDC/CI is disabled in the OSD — re-enable it in the monitor menu.
    DdcDisabled,
}

/// One monitor as the tray menu / desk-map renders it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorView {
    pub id: String,
    pub label: String,
    pub owner: Option<String>,
    pub state: MonitorUiState,
}

/// One peer as the tray / settings renders it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerView {
    pub id: String,
    pub name: String,
    pub online: bool,
    pub can_actuate: bool,
}

/// Bridges live mesh state to the tray UI.
pub struct Controller {
    me: String,
    state: Arc<Mutex<MeshState>>,
    /// Friendly labels per monitor_id (from identity/labeling); falls back to the id.
    labels: HashMap<String, String>,
    /// Liveness TTL used to decide peer online/degraded.
    peer_ttl_ms: u64,
}

impl Controller {
    pub fn new(me: impl Into<String>, state: Arc<Mutex<MeshState>>, peer_ttl_ms: u64) -> Self {
        Self {
            me: me.into(),
            state,
            labels: HashMap::new(),
            peer_ttl_ms,
        }
    }

    /// Set a friendly label for a monitor (e.g. from the onboarding labeling step).
    pub fn set_label(&mut self, monitor_id: impl Into<String>, label: impl Into<String>) {
        self.labels.insert(monitor_id.into(), label.into());
    }

    fn lock(&self) -> MutexGuard<'_, MeshState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// View models for the given monitors, in the order supplied.
    pub fn monitor_views(&self, monitor_ids: &[String]) -> Vec<MonitorView> {
        let st = self.lock();
        monitor_ids
            .iter()
            .map(|id| {
                let owner = st.ownership.owner(id).map(str::to_owned);
                let state = match st.ownership.state(id) {
                    OwnershipState::Owned if owner.as_deref() == Some(self.me.as_str()) => {
                        MonitorUiState::Mine
                    }
                    OwnershipState::Owned => MonitorUiState::OwnedByPeer,
                    OwnershipState::Unknown => MonitorUiState::Unowned,
                    OwnershipState::Stranded => MonitorUiState::Stranded,
                    OwnershipState::DdcDisabled => MonitorUiState::DdcDisabled,
                };
                MonitorView {
                    id: id.clone(),
                    label: self.labels.get(id).cloned().unwrap_or_else(|| id.clone()),
                    owner,
                    state,
                }
            })
            .collect()
    }

    /// View models for all known peers (online computed against `now_ms` and the TTL).
    pub fn peer_views(&self, now_ms: u64) -> Vec<PeerView> {
        let st = self.lock();
        let mut views: Vec<PeerView> = st
            .peers
            .ids()
            .into_iter()
            .map(|id| {
                let presence = st.peers.get(&id);
                PeerView {
                    online: st.peers.is_online(&id, now_ms, self.peer_ttl_ms),
                    name: presence.map(|p| p.name.clone()).unwrap_or_default(),
                    can_actuate: presence.is_some_and(|p| p.can_actuate),
                    id,
                }
            })
            .collect();
        views.sort_by(|a, b| a.id.cmp(&b.id)); // deterministic for the UI list
        views
    }

    /// True if the mesh is degraded (a known peer is silent) — the tray should pause disruptive ops.
    pub fn is_degraded(&self, now_ms: u64) -> bool {
        self.lock().peers.is_degraded(now_ms, self.peer_ttl_ms)
    }

    /// Would applying `assignments` leave this operator with no visible screen? (Blind warning.)
    pub fn would_blind(&self, assignments: &[(String, String)], all_monitors: &[String]) -> bool {
        let st = self.lock();
        would_go_blind(&self.me, assignments, &st.ownership, all_monitors)
    }

    /// Plan and apply a preset best-effort. `perform` actuates one switch (locally, or by routing
    /// over the mesh) and returns its outcome; the returned [`PresetOutcome`] carries per-monitor
    /// results so the UI surfaces partial failure. A degraded mesh actuates nothing.
    pub fn apply_preset(
        &self,
        assignments: &[(String, String)],
        all_monitors: &[String],
        now_ms: u64,
        perform: impl FnMut(&PlannedSwitch) -> SwitchOutcome,
    ) -> PresetOutcome {
        let degraded = self.is_degraded(now_ms);
        let plan = {
            let st = self.lock();
            plan_preset(&self.me, assignments, &st.ownership, all_monitors, degraded)
        };
        execute_plan(&plan, perform)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TTL: u64 = 10_000;

    fn state() -> Arc<Mutex<MeshState>> {
        Arc::new(Mutex::new(MeshState::default()))
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn monitor_views_map_every_state_and_use_labels() {
        let st = state();
        {
            let mut s = st.lock().unwrap();
            s.ownership.observe("mine", Some("A".into()), 100);
            s.ownership.observe("theirs", Some("B".into()), 100);
            s.ownership.mark_stranded("lost", 100);
            s.ownership.mark_ddc_disabled("off", 100);
            // "unseen" is never recorded -> Unowned.
        }
        let mut c = Controller::new("A", Arc::clone(&st), TTL);
        c.set_label("mine", "Center 27\"");

        let views = c.monitor_views(&ids(&["mine", "theirs", "lost", "off", "unseen"]));
        assert_eq!(views[0].state, MonitorUiState::Mine);
        assert_eq!(views[0].label, "Center 27\"");
        assert_eq!(views[1].state, MonitorUiState::OwnedByPeer);
        assert_eq!(views[1].owner.as_deref(), Some("B"));
        assert_eq!(views[2].state, MonitorUiState::Stranded);
        assert_eq!(views[3].state, MonitorUiState::DdcDisabled);
        assert_eq!(views[4].state, MonitorUiState::Unowned);
        assert_eq!(views[4].label, "unseen", "label falls back to the id");
    }

    #[test]
    fn peer_views_and_degraded_reflect_presence() {
        let st = state();
        {
            let mut s = st.lock().unwrap();
            s.peers
                .observe_announce("A", "My PC".into(), vec![], true, 1, 0);
            s.peers
                .observe_announce("B", "Laptop".into(), vec![], false, 1, 0);
        }
        let c = Controller::new("A", Arc::clone(&st), TTL);

        // At 5s both online -> not degraded.
        let views = c.peer_views(5_000);
        assert_eq!(views.len(), 2);
        assert!(views.iter().all(|p| p.online));
        assert!(!c.is_degraded(5_000));

        // At 15s both silent -> degraded.
        assert!(c.is_degraded(15_000));
        let stale = c.peer_views(15_000);
        assert!(stale.iter().all(|p| !p.online));
        assert_eq!(stale[0].name, "My PC");
    }

    #[test]
    fn would_blind_delegates_to_orchestration() {
        let st = state();
        {
            let mut s = st.lock().unwrap();
            s.ownership.observe("m1", Some("A".into()), 100);
        }
        let c = Controller::new("A", Arc::clone(&st), TTL);
        let all = ids(&["m1"]);
        assert!(c.would_blind(&[("m1".into(), "B".into())], &all));
        assert!(!c.would_blind(&[], &all));
    }

    #[test]
    fn apply_preset_executes_best_effort_and_pauses_when_degraded() {
        let st = state();
        {
            let mut s = st.lock().unwrap();
            s.ownership.observe("m1", Some("A".into()), 100);
            s.ownership.observe("m2", Some("A".into()), 100);
        }
        let c = Controller::new("A", Arc::clone(&st), TTL);
        let all = ids(&["m1", "m2"]);
        let assignments = vec![
            ("m1".to_string(), "B".to_string()),
            ("m2".to_string(), "B".to_string()),
        ];

        // Healthy: m1 fails, m2 succeeds -> partial failure surfaced, both attempted.
        let outcome = c.apply_preset(&assignments, &all, 0, |op| {
            if op.monitor_id == "m1" {
                SwitchOutcome::Failed
            } else {
                SwitchOutcome::Success
            }
        });
        assert_eq!(outcome.results.len(), 2);
        assert!(outcome.partial_failure());

        // Now make the mesh degraded by introducing a silent known peer.
        st.lock()
            .unwrap()
            .peers
            .observe_announce("B", "B".into(), vec![], true, 1, 0);
        let mut calls = 0;
        let degraded = c.apply_preset(&assignments, &all, 1_000_000, |_| {
            calls += 1;
            SwitchOutcome::Success
        });
        assert_eq!(calls, 0, "degraded mesh actuates nothing");
        assert!(degraded.results.is_empty());
    }
}

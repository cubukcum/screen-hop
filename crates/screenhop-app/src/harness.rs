//! Measurement / soak harness (M2.5, plan §11.4): runs a fixed number of switches per panel and
//! records the reliability numbers §4.7 is *defined against* — first-attempt %, within-retry %,
//! and latency median / p90, per panel and per direction.
//!
//! This is the **skeleton**: the stats engine is complete and unit-tested over injected samples.
//! The production wiring (drive a real [`crate::actuator::LocalActuator`] against the maintainer's
//! declared panel population, timing each switch with a real clock) is M5.4 — [`soak_panel`] takes
//! a sampler closure precisely so that wiring is a thin adapter, not a rewrite. Reliability
//! headline numbers are reported **scoped to pull-to-self-capable panels** ([`SoakReport::pull_to_self_panels`]).

use screenhop_core::{SwitchDirection, SwitchOutcome};

/// One recorded switch attempt.
#[derive(Debug, Clone, Copy)]
pub struct SoakSample {
    pub outcome: SwitchOutcome,
    /// Number of write attempts the executor made (1 = succeeded first try).
    pub attempts: u32,
    /// Wall-clock latency of the whole switch in milliseconds.
    pub latency_ms: u64,
}

impl SoakSample {
    pub fn new(outcome: SwitchOutcome, attempts: u32, latency_ms: u64) -> Self {
        Self {
            outcome,
            attempts,
            latency_ms,
        }
    }
}

/// Reliability stats for one `(panel, direction)`, computed over its recorded samples.
#[derive(Debug, Clone)]
pub struct PanelStats {
    pub panel: String,
    pub direction: SwitchDirection,
    samples: Vec<SoakSample>,
}

impl PanelStats {
    pub fn new(panel: impl Into<String>, direction: SwitchDirection) -> Self {
        Self {
            panel: panel.into(),
            direction,
            samples: Vec::new(),
        }
    }

    pub fn record(&mut self, sample: SoakSample) {
        self.samples.push(sample);
    }

    pub fn count(&self) -> usize {
        self.samples.len()
    }

    fn pct(n: usize, total: usize) -> f64 {
        if total == 0 {
            0.0
        } else {
            (n as f64) * 100.0 / (total as f64)
        }
    }

    /// % of switches that succeeded on the **first** attempt (effective success, attempts ≤ 1).
    pub fn first_attempt_pct(&self) -> f64 {
        let n = self
            .samples
            .iter()
            .filter(|s| s.outcome.is_effective_success() && s.attempts <= 1)
            .count();
        Self::pct(n, self.count())
    }

    /// % of switches that succeeded **within the retry budget** (any effective success).
    pub fn within_retry_pct(&self) -> f64 {
        let n = self
            .samples
            .iter()
            .filter(|s| s.outcome.is_effective_success())
            .count();
        Self::pct(n, self.count())
    }

    /// Latency percentile (nearest-rank) over all samples, in ms. `None` if there are no samples.
    pub fn latency_percentile_ms(&self, pct: f64) -> Option<u64> {
        if self.samples.is_empty() {
            return None;
        }
        let mut lat: Vec<u64> = self.samples.iter().map(|s| s.latency_ms).collect();
        lat.sort_unstable();
        let n = lat.len();
        // Nearest-rank: rank = ceil(p/100 * n), 1-based; index = rank - 1.
        let rank = ((pct.clamp(0.0, 100.0) / 100.0) * n as f64).ceil() as usize;
        let idx = rank.saturating_sub(1).min(n - 1);
        Some(lat[idx])
    }

    pub fn latency_median_ms(&self) -> Option<u64> {
        self.latency_percentile_ms(50.0)
    }

    pub fn latency_p90_ms(&self) -> Option<u64> {
        self.latency_percentile_ms(90.0)
    }

    /// True if this panel is **pull-to-self-capable** (≥ 1 effective success on the pull-to-self
    /// direction) — the scope the §4.7 reliability headline numbers are reported over.
    pub fn is_pull_to_self_capable(&self) -> bool {
        self.direction == SwitchDirection::PullToSelf
            && self
                .samples
                .iter()
                .any(|s| s.outcome.is_effective_success())
    }
}

/// Run a soak of `sample_size` switches for one `(panel, direction)`, pulling each sample from
/// `sampler` (which in production performs a real switch and times it). Returns the panel's stats.
pub fn soak_panel(
    panel: impl Into<String>,
    direction: SwitchDirection,
    sample_size: usize,
    mut sampler: impl FnMut(usize) -> SoakSample,
) -> PanelStats {
    let mut stats = PanelStats::new(panel, direction);
    for i in 0..sample_size {
        stats.record(sampler(i));
    }
    stats
}

/// A full soak report across a declared panel population.
#[derive(Debug, Clone, Default)]
pub struct SoakReport {
    pub panels: Vec<PanelStats>,
}

impl SoakReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, stats: PanelStats) {
        self.panels.push(stats);
    }

    /// The panels the headline reliability numbers are scoped to (pull-to-self-capable).
    pub fn pull_to_self_panels(&self) -> impl Iterator<Item = &PanelStats> {
        self.panels.iter().filter(|p| p.is_pull_to_self_capable())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(outcome: SwitchOutcome, attempts: u32, latency_ms: u64) -> SoakSample {
        SoakSample::new(outcome, attempts, latency_ms)
    }

    #[test]
    fn first_attempt_vs_within_retry() {
        let mut s = PanelStats::new("P", SwitchDirection::PullToSelf);
        s.record(sample(SwitchOutcome::Success, 1, 100)); // first-attempt success
        s.record(sample(SwitchOutcome::Success, 3, 200)); // within-retry success
        s.record(sample(
            SwitchOutcome::AssumedSuccessReadbackInconclusive,
            1,
            150,
        )); // first-attempt effective success
        s.record(sample(SwitchOutcome::Failed, 3, 300)); // failure
                                                         // 4 samples: 3 effective successes (75%), 2 of them first-attempt (50%).
        assert_eq!(s.count(), 4);
        assert!((s.within_retry_pct() - 75.0).abs() < 1e-9);
        assert!((s.first_attempt_pct() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn latency_percentiles_use_nearest_rank() {
        let mut s = PanelStats::new("P", SwitchDirection::PullToSelf);
        for ms in [10u64, 20, 30, 40, 50, 60, 70, 80, 90, 100] {
            s.record(sample(SwitchOutcome::Success, 1, ms));
        }
        // n = 10: median rank = ceil(0.5*10) = 5 -> idx 4 -> 50; p90 rank = 9 -> idx 8 -> 90.
        assert_eq!(s.latency_median_ms(), Some(50));
        assert_eq!(s.latency_p90_ms(), Some(90));
    }

    #[test]
    fn empty_stats_are_safe() {
        let s = PanelStats::new("P", SwitchDirection::PullToSelf);
        assert_eq!(s.latency_median_ms(), None);
        assert_eq!(s.first_attempt_pct(), 0.0);
        assert!(!s.is_pull_to_self_capable());
    }

    #[test]
    fn soak_runs_sample_size_times_and_scopes_pull_to_self() {
        // Deterministic sampler: 1-in-5 fails, latency = i*10.
        let stats = soak_panel("P", SwitchDirection::PullToSelf, 200, |i| {
            let outcome = if i % 5 == 0 {
                SwitchOutcome::Failed
            } else {
                SwitchOutcome::Success
            };
            sample(outcome, 1, (i as u64) * 10)
        });
        assert_eq!(stats.count(), 200);
        assert!(stats.is_pull_to_self_capable());
        assert!((stats.within_retry_pct() - 80.0).abs() < 1e-9);

        let mut report = SoakReport::new();
        report.add(stats);
        // A push-release panel is NOT in the pull-to-self headline scope.
        let mut pr = PanelStats::new("Q", SwitchDirection::PushRelease);
        pr.record(sample(SwitchOutcome::Success, 1, 10));
        report.add(pr);
        assert_eq!(report.pull_to_self_panels().count(), 1);
    }
}

//! Policies: rules that match a condition and propose an action.

use std::collections::HashMap;
use std::time::Instant;

use serde::Serialize;

pub use crate::config::PolicyAction as ActionKind;

use crate::config::{Policy, PolicyAction};
use crate::core::metrics::SystemSample;
use crate::core::process::ProcessSample;

/// An action a policy would take, if it were allowed to.
#[derive(Debug, Clone, Serialize)]
pub struct Proposal {
    pub policy: String,
    pub pid: u32,
    pub process_name: String,
    pub action: PolicyAction,
    pub reason: String,
    pub current_value: f64,
}

/// Matches the policies against the machine.
pub struct PolicyEngine {
    policies: Vec<Policy>,
    /// When each policy last acted, so it stays quiet for its cooldown.
    fired: HashMap<String, Instant>,
}

impl PolicyEngine {
    pub fn new(policies: Vec<Policy>) -> Self {
        Self {
            policies,
            fired: HashMap::new(),
        }
    }

    /// The policies being watched.
    pub fn policies(&self) -> &[Policy] {
        &self.policies
    }

    /// The actions the matching policies would take.
    ///
    /// A policy that is disabled, or whose action is still in its cooldown, or
    /// whose target is excluded, proposes nothing.
    pub fn propose(
        &mut self,
        sample: &SystemSample,
        processes: &[ProcessSample],
        now: Instant,
    ) -> Vec<Proposal> {
        let mut proposals = Vec::new();
        for policy in &self.policies {
            if !policy.enabled {
                continue;
            }
            if let Some(last) = self.fired.get(&policy.name)
                && now.duration_since(*last) < policy.cooldown
            {
                continue;
            }
            let Some((process, value)) = match_process(policy, sample, processes) else {
                continue;
            };
            if policy
                .exclude
                .iter()
                .any(|name| name == &process.name || process.name.starts_with(name.as_str()))
            {
                continue;
            }
            self.fired.insert(policy.name.clone(), now);
            proposals.push(Proposal {
                policy: policy.name.clone(),
                pid: process.pid,
                process_name: process.name.clone(),
                action: policy.action,
                reason: format!(
                    "{} was {value:.1}, past the {} of {:.1}",
                    process.name, policy.metric, policy.threshold
                ),
                current_value: value,
            });
        }
        proposals
    }
}

/// The process a policy is about, and the value that matched.
fn match_process(
    policy: &Policy,
    sample: &SystemSample,
    processes: &[ProcessSample],
) -> Option<(ProcessSample, f64)> {
    if let Some(name) = policy.metric.strip_prefix("process_") {
        let measure = |process: &ProcessSample| match name {
            "cpu_percent" => Some(process.cpu_percent),
            "memory_percent" => Some(process.mem_percent),
            "fd_count" => Some(process.fd_count as f64),
            "thread_count" => Some(process.thread_count as f64),
            "pressure_score" => Some(process.pressure_score),
            _ => None,
        };
        let matching = processes
            .iter()
            .filter_map(|process| measure(process).map(|value| (process, value)))
            .filter(|(_, value)| policy.condition.holds(*value, policy.threshold))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        return matching.map(|(process, value)| (process.clone(), value));
    }

    let (value, _, _) = crate::alert::metric_value(sample, processes, &policy.metric)?;
    if !policy.condition.holds(value, policy.threshold) {
        return None;
    }
    // A system-wide condition is about the worst offender.
    let process = processes
        .iter()
        .max_by(|a, b| {
            a.pressure_score
                .partial_cmp(&b.pressure_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })?
        .clone();
    Some((process, value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Condition;
    use crate::config::PolicyAction as ConfigAction;
    use std::time::Duration;

    fn policy(name: &str, enabled: bool) -> Policy {
        Policy {
            name: name.to_string(),
            enabled,
            metric: "process_cpu_percent".to_string(),
            condition: Condition::Above,
            threshold: 50.0,
            duration: Duration::from_secs(0),
            action: ConfigAction::Kill,
            cooldown: Duration::from_secs(60),
            require_confirmation: false,
            exclude: Vec::new(),
        }
    }

    fn processes() -> Vec<ProcessSample> {
        vec![ProcessSample {
            pid: 42,
            name: "backup".into(),
            cpu_percent: 80.0,
            pressure_score: 3.0,
            ..Default::default()
        }]
    }

    #[test]
    fn a_disabled_policy_never_matches() {
        let mut engine = PolicyEngine::new(vec![policy("kill-hog", false)]);

        let proposals = engine.propose(
            &SystemSample::default(),
            &processes(),
            Instant::now() + Duration::from_secs(60),
        );

        assert!(proposals.is_empty(), "{proposals:?}");
    }

    #[test]
    fn an_enabled_policy_proposes_an_action() {
        let mut engine = PolicyEngine::new(vec![policy("kill-hog", true)]);

        let proposals = engine.propose(
            &SystemSample::default(),
            &processes(),
            Instant::now() + Duration::from_secs(60),
        );

        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].pid, 42);
        assert_eq!(proposals[0].process_name, "backup");
        assert_eq!(proposals[0].action, PolicyAction::Kill);
        assert!(
            proposals[0].reason.contains("80.0"),
            "{}",
            proposals[0].reason
        );
    }

    #[test]
    fn a_proposal_does_not_perform_anything() {
        let mut engine = PolicyEngine::new(vec![policy("kill-hog", true)]);

        let proposals = engine.propose(
            &SystemSample::default(),
            &processes(),
            Instant::now() + Duration::from_secs(60),
        );

        // The process is still there: a proposal is only a proposal.
        assert_eq!(proposals.len(), 1);
        assert!(
            std::path::Path::new(&format!("/proc/{}", proposals[0].pid)).exists(),
            "the policy acted on the process"
        );
    }

    #[test]
    fn an_excluded_process_is_not_matched() {
        let mut excluded = policy("kill-hog", true);
        excluded.exclude = vec!["backup".to_string()];
        let mut engine = PolicyEngine::new(vec![excluded]);

        let proposals = engine.propose(
            &SystemSample::default(),
            &processes(),
            Instant::now() + Duration::from_secs(60),
        );

        assert!(proposals.is_empty(), "{proposals:?}");
    }

    #[test]
    fn a_policy_waits_for_its_cooldown() {
        let mut engine = PolicyEngine::new(vec![policy("kill-hog", true)]);
        let start = Instant::now();

        let first = engine.propose(&SystemSample::default(), &processes(), start);
        let second = engine.propose(
            &SystemSample::default(),
            &processes(),
            start + Duration::from_secs(10),
        );
        let later = engine.propose(
            &SystemSample::default(),
            &processes(),
            start + Duration::from_secs(120),
        );

        assert_eq!(first.len(), 1);
        assert!(second.is_empty(), "{second:?}");
        assert_eq!(later.len(), 1);
    }

    #[test]
    fn a_process_below_the_threshold_is_not_matched() {
        let mut quiet = policy("kill-hog", true);
        quiet.threshold = 95.0;
        let mut engine = PolicyEngine::new(vec![quiet]);

        let proposals = engine.propose(
            &SystemSample::default(),
            &processes(),
            Instant::now() + Duration::from_secs(60),
        );

        assert!(proposals.is_empty(), "{proposals:?}");
    }
}

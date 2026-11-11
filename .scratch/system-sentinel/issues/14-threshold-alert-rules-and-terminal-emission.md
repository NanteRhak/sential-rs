# 14: Threshold alert rules and terminal emission

**What to build:** The tool raises alerts when a configured threshold is crossed, and says so on the terminal while it happens.

**Blocked by:** 04: `sentinel health`: CPU and memory summary; 08: `sentinel ps`: process identity and memory listing.

**Status:** ready-for-agent

- [ ] Alert rules are read from config: metric, condition, threshold, duration, severity and message
- [ ] A rule fires only after its condition has held for the configured duration, not on a single noisy sample
- [ ] Each raised alert carries timestamp, severity, category, metric name, affected process, current value, threshold and a suggested action
- [ ] Alerts are printed to the terminal as they are raised, with the severity visible
- [ ] Rules can be disabled individually, and a disabled rule never fires
- [ ] A test feeds a synthetic sample series into the engine and asserts the alert fires once the duration is met and not before

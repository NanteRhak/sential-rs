# 15: Alert cooldown, log file and `sentinel alerts`

**What to build:** Alerts that do not flood the terminal, and a record of what was raised: a cooldown per rule, an alert log on disk, and a command to read the recent ones back.

**Blocked by:** 14: Threshold alert rules and terminal emission.

**Status:** done

- [x] A cooldown period per rule prevents the same alert from repeating until the cooldown expires
- [x] Alerts are appended to the configured log file in a machine-readable format
- [x] `sentinel alerts --last 1h` lists recent alerts filtered by the requested time window
- [x] Alert history survives a restart when the log file is configured
- [x] A test asserts a rule does not re-fire inside its cooldown and that the log file receives the alert

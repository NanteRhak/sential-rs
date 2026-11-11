# 18: TUI: alert pane and action menu

**What to build:** Alerts and actions reachable without leaving the dashboard: a pane listing recent alerts, and a menu offering kill, suspend, resume and renice on the selected process.

**Blocked by:** 13: TUI: process detail panel; 15: Alert cooldown, log file and `sentinel alerts`; 17: Manual signals, suspend, resume and renice.

**Status:** ready-for-agent

- [ ] The TUI has a pane listing recent alerts with their severity, and it fills as alerts are raised
- [ ] The action menu offers kill, suspend, resume and renice for the selected process
- [ ] The action menu asks for confirmation before a destructive action and offers a dry-run preview
- [ ] Actions taken from the TUI are written to the same action log as CLI actions
- [ ] A test asserts the action menu lists the available actions for a selected process

# 29: Daemon mode

**What to build:** The tool runs in the background with no terminal attached, collecting, detecting, storing, alerting and applying policies around the clock.

**Blocked by:** 15: Alert cooldown, log file and `sentinel alerts`; 26: Automatic actions and protected processes; 28: Disk persistence with retention limits.

**Status:** done

- [x] `sentinel daemon` runs in the background, collecting metrics, detecting anomalies, storing history, emitting alerts and applying policies
- [x] It logs to the configured destination and runs until asked to stop, shutting down cleanly on a stop signal
- [x] It keeps running when the terminal that started it goes away
- [x] Its resource use stays within the stated lightweight budget, and that budget is documented
- [x] A test starts the daemon, lets it run briefly, and asserts it produced samples and stayed alive

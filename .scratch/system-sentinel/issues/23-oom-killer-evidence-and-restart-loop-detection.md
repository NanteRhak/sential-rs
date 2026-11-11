# 23: OOM-killer evidence and restart-loop detection

**What to build:** Two diagnoses that explain the worst cases: the OOM killer took this process, and this process is crash-looping.

**Blocked by:** 22: Process exit and crash detection.

**Status:** ready-for-agent

- [ ] When the OOM killer is the likely cause of an exit, the exit record says so and names the evidence
- [ ] The tool looks for OOM evidence in the kernel log where permissions allow, and degrades quietly where they do not
- [ ] A process that restarts repeatedly within a window is reported as a restart loop
- [ ] The restart count and the window are configurable
- [ ] A test feeds a synthetic exit sequence and asserts a restart loop is reported

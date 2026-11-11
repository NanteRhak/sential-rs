# 17: Manual signals, suspend, resume and renice

**What to build:** The rest of the manual controls: send any signal, stop a process without killing it, start it again, and change its priority.

**Blocked by:** 16: `sentinel kill`: safe manual termination with action log.

**Status:** ready-for-agent

- [ ] `sentinel kill --signal <sig>` sends a chosen signal to the target process
- [ ] `sentinel suspend <pid>` and `sentinel resume <pid>` stop and continue the process
- [ ] `sentinel renice <pid> <value>` changes the process priority
- [ ] Each action re-validates the PID, supports `--dry-run` and is written to the action log
- [ ] Actions needing extra privileges report the missing permission clearly instead of failing silently
- [ ] Protected processes are refused

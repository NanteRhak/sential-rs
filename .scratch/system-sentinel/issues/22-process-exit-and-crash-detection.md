# 22: Process exit and crash detection

**What to build:** When a monitored process dies unexpectedly, the tool remembers it: who died, how it died, and what it was using right up to the end.

**Blocked by:** 08: `sentinel ps`: process identity and memory listing.

**Status:** done

- [x] The tool notices when a process it has seen before exits, and records the exit time
- [x] Each exit record carries the process name, PID, parent PID, exit code or terminating signal, and the resource usage seen immediately before the exit
- [x] A normal, expected exit and an unexpected exit are distinguishable
- [x] The exit record survives long enough to be reported and is visible as a recent event
- [x] A test starts a short-lived process and asserts an exit event is recorded for it

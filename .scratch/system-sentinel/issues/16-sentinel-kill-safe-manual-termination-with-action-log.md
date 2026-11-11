# 16: `sentinel kill`: safe manual termination with action log

**What to build:** `sentinel kill <pid>` ends a runaway process, safely: it refuses to hit the wrong process, asks before acting, and records what it did.

**Blocked by:** 08: `sentinel ps`: process identity and memory listing.

**Status:** done

- [x] `sentinel kill <pid>` terminates the process and reports the outcome
- [x] The PID is re-validated against the recorded start time before the signal is sent, so a reused PID is not killed
- [x] The command asks for confirmation before killing, and a `--yes` flag skips the prompt for scripted use
- [x] A `--dry-run` flag reports what would happen without acting
- [x] Protected processes are refused, naming the rule that protects them
- [x] An unknown PID or a missing permission produces a clear message and a non-zero exit, not a crash
- [x] Every action is written to the action log with timestamp, action, PID, process name, reason, whether it was user-requested and the result

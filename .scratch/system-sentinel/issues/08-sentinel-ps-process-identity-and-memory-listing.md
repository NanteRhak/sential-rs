# 08: `sentinel ps`: process identity and memory listing

**What to build:** `sentinel ps` lists every process with who it is and how much memory it holds: PID, name, parent PID, user, state, uptime, RSS, VSZ and memory percentage.

**Blocked by:** 03: Sampling loop with delta and rate computation.

**Status:** ready-for-agent

- [ ] The command prints a table of PID, name, parent PID, user, state, uptime and memory (RSS, VSZ, percentage)
- [ ] Processes that exit during the scan are skipped instead of aborting the listing
- [ ] Command lines that are not valid UTF-8 are handled without failing the scan
- [ ] Each process is identified by PID plus start time, so a reused PID is never confused with the original process
- [ ] A test asserts the table header and at least one row, and that the current process appears in its own listing

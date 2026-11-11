# 04: `sentinel health`: CPU and memory summary

**What to build:** `sentinel health` prints the CPU and memory picture of the machine in the layout the specification shows: total CPU percentage, load average, memory used and available, and swap usage.

**Blocked by:** 03: Sampling loop with delta and rate computation.

**Status:** ready-for-agent

- [ ] The command prints total CPU percentage, per-core percentages, the user/system/iowait/idle breakdown and load average
- [ ] Memory rows show total, used, available, cached and swap usage
- [ ] CPU percentages are computed from deltas between ticks, not from counters accumulated since boot
- [ ] `sentinel health --interval <duration>` overrides the configured sampling interval for one run
- [ ] The command exits non-zero with a clear message when the metrics cannot be read
- [ ] A test asserts the output contains the CPU, memory and swap rows

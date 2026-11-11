# 31: Per-process network attribution

**What to build:** Which process is using the network — on the machines where the platform can tell us, and honestly marked where it cannot.

**Blocked by:** 09: `sentinel ps`: per-process CPU percentage, sorting and JSON output.

**Status:** ready-for-agent

- [ ] Where the platform can attribute network traffic to a process, the process list shows bytes sent and received
- [ ] Where it cannot, the columns are clearly marked as unavailable rather than showing zero as if it were a measurement
- [ ] The feature is opt-in and off by default, because it costs more than the other collectors
- [ ] The low-resource profile keeps it disabled
- [ ] A test asserts the unavailable case is labelled clearly

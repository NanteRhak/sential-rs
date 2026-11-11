# 27: In-memory history and `sentinel report`

**What to build:** "What happened in the 60 seconds before the system froze?" — the tool keeps the recent past in memory and can summarise it.

**Blocked by:** 04: `sentinel health`: CPU and memory summary.

**Status:** ready-for-agent

- [ ] Recent samples are kept in an in-memory history whose length is set by the configured history duration
- [ ] `sentinel report --last 10m` summarises what happened over the requested window
- [ ] The report can be emitted as JSON for scripting
- [ ] The report covers system metrics, the processes that were busiest, and any alerts raised in that window
- [ ] A test asserts the report covers the requested window and is valid JSON

# 13: TUI: process detail panel

**What to build:** Selecting a process opens a detail view: what it actually is, what it is doing right now, and what it looked like a moment ago.

**Blocked by:** 11: TUI: sorting, filtering and search; 12: TUI: usage graphs.

**Status:** ready-for-agent

- [ ] Selecting a process opens a detail panel showing the full command line, start time, parent PID, user, current CPU and memory usage, file descriptor count, thread count and cgroup
- [ ] The same detail is available outside the TUI as `sentinel inspect <pid>`
- [ ] The panel includes a short recent-usage graph for the selected process
- [ ] The detail panel updates as the selected process's usage changes
- [ ] A process the user is not allowed to read shows what is available and no more
- [ ] A test renders the detail panel for a known process and asserts the command line is present

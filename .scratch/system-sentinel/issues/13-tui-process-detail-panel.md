# 13: TUI: process detail panel

**What to build:** Selecting a process opens a detail view: what it actually is, what it is doing right now, and what it looked like a moment ago.

**Blocked by:** 11: TUI: sorting, filtering and search; 12: TUI: usage graphs.

**Status:** done

- [x] Selecting a process opens a detail panel showing the full command line, start time, parent PID, user, current CPU and memory usage, file descriptor count, thread count and cgroup
- [x] The same detail is available outside the TUI as `sentinel inspect <pid>`
- [x] The panel includes a short recent-usage graph for the selected process
- [x] The detail panel updates as the selected process's usage changes
- [x] A process the user is not allowed to read shows what is available and no more
- [x] A test renders the detail panel for a known process and asserts the command line is present
